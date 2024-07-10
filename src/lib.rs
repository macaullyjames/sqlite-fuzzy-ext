use std::ffi::c_char;
use std::usize;
use std::{collections::HashMap, ffi::c_int};

use rusqlite::{
    ffi,
    functions::{Context, FunctionFlags},
    types::{ToSqlOutput, Value},
    Connection,
};

#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[no_mangle]
pub unsafe extern "C" fn sqlite3_extension_init(
    db: *mut ffi::sqlite3,
    pz_err_msg: *mut *mut c_char,
    p_api: *mut ffi::sqlite3_api_routines,
) -> c_int {
    Connection::extension_init2(db, pz_err_msg, p_api, extension_init)
}

fn extension_init(db: Connection) -> rusqlite::Result<bool> {
    db.create_scalar_function(
        "fuzzy_score",
        2,
        FunctionFlags::SQLITE_DETERMINISTIC,
        fuzzy_search,
    )?;
    rusqlite::trace::log(ffi::SQLITE_WARNING, "Rusqlite extension initialized");
    Ok(false)
}

fn fuzzy_search(ctx: &Context) -> rusqlite::Result<ToSqlOutput<'static>> {
    let pattern: Box<str> = ctx.get(0)?;
    let text: Box<str> = ctx.get(1)?;

    let score = determine_best_streak(&pattern, &text);

    let score = score.total();
    let out = ToSqlOutput::Owned(Value::Integer(score));

    return Ok(out);
}

fn determine_best_streak(pattern: &str, text: &str) -> Score {
    if pattern.is_empty() || text.is_empty() {
        return Score {
            direct_bonus: !text.contains('/'),
            ..Default::default()
        };
    } else if text == pattern {
        return Score {
            streak_len: pattern.len(),
            len_bonus: 1000,
            end_bonus: 1000,
            direct_bonus: true,
        };
    }

    let mut all_matches: HashMap<char, CharMatch> = HashMap::new();

    for (i, chr) in pattern.char_indices() {
        if let Some(chr_match) = all_matches.get_mut(&chr) {
            chr_match.pattern_indices.push(i);
        } else {
            all_matches.insert(chr, CharMatch::new(i));
        }
    }

    for (idx, chr) in text.char_indices() {
        if let Some(chr_match) = all_matches.get_mut(&chr) {
            chr_match.indices.push(idx);
        } else if let Some(chr) = chr.to_lowercase().next() {
            if let Some(chr_match) = all_matches.get_mut(&chr) {
                chr_match.indices.push(idx);
            }
        }
    }

    //println!("{all_matches:?}");

    let mut streaks = vec![];
    let mut valid_after = 0;

    for (i, chr) in pattern.char_indices() {
        let current = all_matches.get(&chr).expect("should exist");
        let next_chr = pattern.chars().nth(i + 1);

        let valid_before = next_chr
            .map(|c| {
                let next = all_matches.get(&c).unwrap();
                next.indices[next.indices.len() - 1]
            })
            .unwrap_or(usize::MAX);

        add_streaks(&current, &mut streaks, &mut valid_after, valid_before);
    }

    //println!("{streaks:?}");

    let best_streak = streaks.into_iter().reduce(|acc, item| {
        let a = acc.len();
        let b = item.len();

        if a < b {
            item
        } else if a == b && acc.start < item.start {
            item
        } else {
            acc
        }
    });

    if let Some(best_streak) = best_streak {
        let text_len = text.len() as f32;
        let end_bonus = (best_streak.end as f32 / text_len * 100.) as usize;
        let len_bonus = (best_streak.len() as f32 / text_len * 200.) as usize;
        let direct_bonus = !text.contains('/');

        Score {
            streak_len: best_streak.len(),
            end_bonus,
            len_bonus,
            direct_bonus,
        }
    } else {
        Score::default()
    }
}

#[derive(Default, Debug)]
struct Score {
    streak_len: usize,
    end_bonus: usize,
    len_bonus: usize,
    direct_bonus: bool,
}

impl Score {
    fn total(&self) -> i64 {
        let direct_bonus = if self.direct_bonus { 200 } else { 0 };

        let total = self.streak_len * 100 + self.end_bonus + self.len_bonus + direct_bonus;
        -(total as i64)
    }
}

/**
  current: the char its matched against
  iter: the remaining iterator of chars
  streaks: streaks to be used to score
  valid_from: ignore indices before
*/
fn add_streaks(
    current: &CharMatch,
    streaks: &mut Vec<Streak>,
    valid_after: &mut usize,
    valid_before: usize,
) {
    let mut update = true;

    for idx in current.indices.iter() {
        if *idx < *valid_after || valid_before < *idx {
            continue;
        }

        if update {
            *valid_after = *idx;
            update = false;
        }

        let mut add_new_streak = true;

        for streak in streaks.iter_mut() {
            if streak.try_extend(*idx, current.pattern_indices.clone()) {
                add_new_streak = false;
                break;
            }
        }

        if add_new_streak {
            streaks.push(Streak::new(*idx, current.pattern_indices.clone()));
        }
    }
}

/// Contains all text indices that matches this char.
#[derive(Clone, Debug)]
struct CharMatch {
    indices: Vec<usize>,
    pattern_indices: Vec<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct StreakItem {
    text_idx: usize,
    pattern_indices: Vec<usize>,
}

impl StreakItem {
    fn new(text_idx: usize, pattern_indices: Vec<usize>) -> Self {
        Self {
            text_idx, pattern_indices
        }
    }

    fn valid_next(&self, maybe_next: &StreakItem) -> bool {
        for p_idx in self.pattern_indices.iter() {
            for op_idx in maybe_next.pattern_indices.iter() {
                if self.text_idx + 1 == maybe_next.text_idx && *p_idx + 1 == *op_idx {
                    return true;
                }
            }
        }

        false
    }
}

/// Begin - end
#[derive(Clone, Debug, Eq, PartialEq)]
struct Streak {
    items: Vec<StreakItem>,
}

impl Streak {
    fn new(text_idx: usize, pattern_indices: Vec<usize>) -> Self {
        Self {
            items: vec![StreakItem::new(text_idx, pattern_indices)],
        }
    }

    /// Will try to extend and returns true if succeeded
    fn try_extend(&mut self, text_idx: usize, pattern_indices: Vec<usize>) -> bool {
        let maybe_next = StreakItem::new(text_idx, pattern_indices);

        let mut valid_next = false;
        for item in self.items.iter() {
            if item.valid_next(&maybe_next) {
                valid_next = true;
                break;
            }
        }

        if valid_next {
            self.items.push(maybe_next);
            true
        } else {
            false
        }
    }

    fn len(&self) -> usize {
        let mut longest = 0;



        //self.end - self.start + 1
    }
}

impl Ord for Streak {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        let a = self.len();
        let b = other.len();

        if a == b {
            // Return shorter
            self.start.cmp(&other.start)
        } else {
            // Return higher len
            b.cmp(&a)
        }
    }
}

impl PartialOrd for Streak {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl CharMatch {
    fn new(idx: usize) -> Self {
        Self {
            indices: vec![idx],
            pattern_indices: vec![],
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::*;

    //#[test]
    //fn test_one() {
    //let a = "Projects/config/nvim";
    //let b = "Projects/neovim";

    //let pattern = "convim";

    //let score_a = determine_best_streak(pattern, a).total();
    //let score_b = determine_best_streak(pattern, b).total();

    //assert!(score_a < score_b, "Wrong order: {}, {}", score_a, score_b);
    //}

    //#[test]
    //fn test_if_children_correctly_added() {
    //let a = "Projects/config/nvim";
    //let b = "Projects/neovim";

    //let pattern = "nvim";

    //let score_a = determine_best_streak(pattern, a).total();
    //let score_b = determine_best_streak(pattern, b).total();

    //assert!(score_a < score_b);
    //}

    //#[test]
    //fn test_complex_pattern() {
    //let a = "Projects/config/nvim";
    //let b = "Projects/neovim";

    //let pattern = "prnvi";

    //let score_a = determine_best_streak(pattern, a).total();
    //let score_b = determine_best_streak(pattern, b).total();

    //assert!(score_a < score_b, "Wrong order: {}, {}", score_a, score_b);
    //}

    #[test]
    fn test_short() {
        let a = "services/update.yaml";
        let b = "gateways/delete.yaml";

        let pattern = "de";

        let score_a = determine_best_streak(pattern, a);
        println!("A: {:?}", score_a);
        let score_b = determine_best_streak(pattern, b);
        println!("B: {:?}", score_b);

        assert!(
            score_b.total() < score_a.total(),
            "Wrong order: {:?}, {:?}",
            score_b,
            score_a
        );
    }

    #[test]
    fn test_neo() {
        let a = "Projects/neovim/";
        let b = "Projects/neo-api-rs/";
        let c = "bin/google-cloud-sdk/lib/surface/monitoring/snoozes/";

        let pattern = "neo";

        let score_a = determine_best_streak(pattern, a);
        println!("A: {:?}", score_a);
        let score_b = determine_best_streak(pattern, b);
        println!("B: {:?}", score_b);
        let score_c = determine_best_streak(pattern, c);
        println!("C: {:?}", score_c);

        assert!(
            score_a.total() < score_b.total() && score_b.total() < score_c.total(),
            "Wrong order: {:?}, {:?}, {:?}",
            score_a,
            score_b,
            score_c,
        );
    }
}
