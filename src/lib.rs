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

    let score = determine_score(&pattern, &text);
    let out = ToSqlOutput::Owned(Value::Integer(score));

    return Ok(out);
}

fn determine_score(pattern: &str, text: &str) -> i64 {
    if pattern.is_empty() {
        return text.len() as i64;
    } else if text.is_empty() {
        return 0;
    } else if text == pattern {
        return -100_000;
    }

    let mut all_matches = HashMap::new();

    for chr in pattern.chars() {
        if !all_matches.contains_key(&chr) {
            all_matches.insert(chr, CharMatch::new());
        }
    }

    for (idx, chr) in text.char_indices() {
        if let Some(chr_match) = all_matches.get_mut(&chr) {
            chr_match.0.push(idx);
        }
    }

    let mut streaks = vec![];
    let mut valid_after = 0;

    for (i, chr) in pattern.char_indices() {
        let current = all_matches.get(&chr).expect("should exist");
        let next_chr = pattern.chars().nth(i + 1);

        let valid_before = next_chr
            .map(|c| {
                let next = all_matches.get(&c).unwrap();
                if next.0.is_empty() {
                    0
                } else {
                    next.0[next.0.len() - 1]
                }
            })
            .unwrap_or(usize::MAX);

        add_streaks(&current, &mut streaks, &mut valid_after, valid_before);
    }

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
        let direct_bonus = if text.contains('/') { 0 } else { 200 };

        let score = best_streak.len() * 50 + end_bonus + len_bonus + direct_bonus;

        -(score as i64)
    } else {
        0
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

    for idx in current.0.iter() {
        if *idx < *valid_after || valid_before < *idx {
            continue;
        }

        if update {
            *valid_after = *idx;
            update = false;
        }

        let mut add_new_streak = true;

        for streak in streaks.iter_mut() {
            if streak.try_extend(*idx) {
                add_new_streak = false;
                break;
            }
        }

        if add_new_streak {
            streaks.push(Streak::new(*idx));
        }
    }
}

/// Contains all text indices that matches this char.
#[derive(Clone, Debug)]
struct CharMatch(Vec<usize>);

/// Begin - end
#[derive(Clone, Debug, Eq, PartialEq)]
struct Streak {
    start: usize,
    end: usize,
}

impl Streak {
    fn new(start: usize) -> Self {
        Self { start, end: start }
    }

    /// Will try to extend and returns true if succeeded
    fn try_extend(&mut self, idx: usize) -> bool {
        if self.end + 1 == idx {
            self.end += 1;
            true
        } else {
            false
        }
    }

    fn len(&self) -> usize {
        self.end - self.start + 1
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
    fn new() -> Self {
        Self(vec![])
    }
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::*;

    #[test]
    fn test_one() {
        let a = "Projects/config/nvim";
        let b = "Projects/neovim";

        let pattern = "convim";

        let now = Instant::now();
        let before = now.elapsed();
        let score_a = determine_score(pattern, a);
        let after = now.elapsed();

        let us = after.as_micros() - before.as_micros();

        println!("Micro secs: {}", us);

        let score_b = determine_score(pattern, b);
        assert!(score_a < score_b, "Wrong order: {}, {}", score_a, score_b);
    }

    #[test]
    fn test_if_children_correctly_added() {
        let a = "Projects/config/nvim";
        let b = "Projects/neovim";

        let pattern = "nvim";

        let score_a = determine_score(pattern, a);
        let score_b = determine_score(pattern, b);

        assert!(score_a < score_b);
    }

    #[test]
    fn test_complex_pattern() {
        let a = "Projects/config/nvim";
        let b = "Projects/neovim";

        let pattern = "prnvi";

        let score_a = determine_score(pattern, a);
        let score_b = determine_score(pattern, b);

        assert!(score_a < score_b, "Wrong order: {}, {}", score_a, score_b);
    }
}
