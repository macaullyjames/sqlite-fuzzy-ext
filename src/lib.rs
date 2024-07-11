use std::cmp::Ordering;
use std::ffi::c_char;
use std::ops::{Deref, DerefMut};
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

    let mut pattern_chrs: HashMap<char, CharMatch> = HashMap::new();
    let mut text_matches: Vec<Option<CharMatch>> = vec![];

    for (i, chr) in pattern.char_indices() {
        if let Some(chr_match) = pattern_chrs.get_mut(&chr) {
            chr_match.push(i);
        } else {
            pattern_chrs.insert(chr, CharMatch::new(i));
        }
    }

    for chr in text.chars() {
        if let Some(chr_match) = pattern_chrs.get(&chr) {
            text_matches.push(Some(chr_match.clone()));
        } else if let Some(chr) = chr.to_lowercase().next() {
            if let Some(chr_match) = pattern_chrs.get(&chr) {
                text_matches.push(Some(chr_match.clone()));
            } else {
                text_matches.push(None);
            }
        } else {
            text_matches.push(None);
        }
    }

    //println!("{all_matches:?}");

    // Remove illegal pattern indices before
    let mut start_from = 0;
    for (i, _) in pattern.char_indices() {
        for text_i in start_from..text_matches.len() {
            let mut delete = false;

            if let Some(tex_match) = &text_matches[text_i] {
                match tex_match.lowest(i) {
                    Ordering::Less => {
                        //println!("less: {:?}, {}", tex_match, i);
                    }
                    Ordering::Equal => {
                        start_from = text_i + 1;
                        break;
                    }
                    Ordering::Greater => {
                        delete = true;
                        //println!("greater: {:?}, {}", tex_match, i);
                    }
                }
            }

            if delete {
                text_matches[text_i] = None;
            }
        }
    }

    let mut start_from = text_matches.len();
    for (i, _) in pattern.char_indices().rev() {
        for text_i in (0..start_from).rev() {
            let mut delete = false;

            if let Some(tex_match) = &text_matches[text_i] {
                match tex_match.highest(i) {
                    Ordering::Less => {
                        println!("lesser: {:?}, {}", tex_match, i);
                        delete = true;
                    }
                    Ordering::Equal => {
                        start_from = text_i;
                        break;
                    }
                    Ordering::Greater => {
                        println!("greater: {:?}, {}", tex_match, i);
                    }
                }
            }

            if delete {
                text_matches[text_i] = None;
            }
        }
    }

    println!("{text_matches:?}");

    //let mut streaks = vec![];

    //let mut current_streak: Option<> = None;

    //for item in text_matches.iter() {
    //if let Some(chr_match) = item {
    //if let Some(streak) = current_streak {
    //streak

    //}
    //}
    //}

    ////println!("{streaks:?}");

    //let best_streak = streaks.into_iter().reduce(|acc, item| {
    //let a = acc.len();
    //let b = item.len();

    //if a < b {
    //item
    //} else if a == b && acc.start < item.start {
    //item
    //} else {
    //acc
    //}
    //});

    //if let Some(best_streak) = best_streak {
    //let text_len = text.len() as f32;
    //let end_bonus = (best_streak.end as f32 / text_len * 100.) as usize;
    //let len_bonus = (best_streak.len() as f32 / text_len * 200.) as usize;
    //let direct_bonus = !text.contains('/');

    //Score {
    //streak_len: best_streak.len(),
    //end_bonus,
    //len_bonus,
    //direct_bonus,
    //}
    //} else {
    //Score::default()
    //}
    Score::default()
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

/// Contains all text indices that matches this char.
#[derive(Clone, Debug)]
struct CharMatch(Vec<usize>);

impl Deref for CharMatch {
    type Target = Vec<usize>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for CharMatch {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl CharMatch {
    fn new(idx: usize) -> Self {
        Self(vec![idx])
    }

    fn lowest(&self, idx: usize) -> Ordering {
        let pattern_idx = self.0[0];

        pattern_idx.cmp(&idx)
    }

    fn highest(&self, idx: usize) -> Ordering {
        let pattern_idx = self.0[self.len() - 1];

        pattern_idx.cmp(&idx)
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

        let score_a = determine_best_streak(pattern, a).total();
        let score_b = determine_best_streak(pattern, b).total();

        assert!(score_a < score_b, "Wrong order: {}, {}", score_a, score_b);
    }

    //#[test]
    //fn test_if_children_correctly_added() {
    //let a = "Projects/config/nvim";
    //let b = "Projects/neovim";

    //let pattern = "nvim";

    //let score_a = determine_best_streak(pattern, a).total();
    //let score_b = determine_best_streak(pattern, b).total();

    //assert!(score_a < score_b);
    //}

    #[test]
    fn test_complex_pattern() {
        let a = "Projects/config/nvim";
        let b = "Projects/neovim";

        let pattern = "prnvi";
        println!("{pattern}");

        let score_a = determine_best_streak(pattern, a).total();
        let score_b = determine_best_streak(pattern, b).total();

        assert!(score_a < score_b, "Wrong order: {}, {}", score_a, score_b);
    }

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
