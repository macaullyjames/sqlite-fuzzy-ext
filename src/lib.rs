use std::{
    collections::HashMap,
    ffi::{c_char, c_int},
    time::Instant,
};

use rusqlite::{
    ffi,
    functions::{Context, FunctionFlags},
    types::{ToSqlOutput, Value},
    Connection,
};

/// # build
/// ```sh
/// cargo build --example loadable_extension --features "loadable_extension functions trace"
/// ```
/// # test
/// ```sh
/// sqlite> .log on
/// sqlite> .load target/debug/examples/libloadable_extension.so
/// (28) Rusqlite extension initialized
/// sqlite> SELECT rusqlite_test_function();
/// Rusqlite extension loaded correctly!
/// ```
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
    let pattern: String = ctx.get(0)?;
    let text: String = ctx.get(1)?;

    let score = determine_score(&pattern, &text);
    let out = ToSqlOutput::Owned(Value::Integer(score));

    return Ok(out);
}

fn determine_score(pattern: &str, text: &str) -> i64 {
    // look for 2 groups
    // a streak doubles in points 1, 2, 4, etc.
    // if a character is a miss it will reduce => 4, 2, 1, 0
    // if 0 a streak is finished.

    // find the three highest streaks in a text
    // the highest multiply by (10?), the second by (4?) and add them up.
    // Invert score to keep asc order.
    // The results with the shortest length should win in

    if pattern.is_empty() {
        return text.len() as i64;
    } else if text.is_empty() {
        return 0;
    } else if text == pattern {
        return -1_000;
    }

    //let now = Instant::now();
    //let begin = now.elapsed();

    //let pattern = pattern.to_lowercase();
    //let text = text.to_lowercase();

    let mut all_matches = HashMap::new();

    for chr in pattern.chars() {
        if !all_matches.contains_key(&chr) {
            all_matches.insert(chr, CharMatch::new(chr));
        }
    }

    //let after_insert = now.elapsed();
    //println!("insert: {}", (after_insert - begin).as_micros());

    for (idx, chr) in text.char_indices() {
        if let Some(chr_match) = all_matches.get_mut(&chr) {
            chr_match.indices.push(idx);
        }
    }

    //let after_indices = now.elapsed();
    //println!("indices: {}", (after_indices - after_insert).as_micros());

    let mut streaks = vec![];
    let mut valid_from = 0;

    for chr in pattern.chars() {
        let current = all_matches.get(&chr).expect("should exist");
        add_streaks(&current, &mut streaks, &mut valid_from);
    }

    //streaks.sort_unstable();

    //let after_streaks = now.elapsed();
    //println!("streaks: {}", (after_streaks - after_indices).as_micros());

    let mut total = 0;

    for streak in streaks.iter() {
        total += streak.end - streak.start;
    }

    //let mut visited = vec![];
    //CharMatch::print(&root, &mut visited);

    //println!("{streaks:?}");

    let score = (total / text.len()) * 1_000;

    -(score as i64)
}

/**
  current: the char its matched against
  iter: the remaining iterator of chars
  streaks: streaks to be used to score
  valid_from: ignore indices before
*/
fn add_streaks(current: &CharMatch, streaks: &mut Vec<Streak>, valid_from: &mut usize) {
    let mut update = true;

    for idx in current.indices.iter() {
        if *idx < *valid_from {
            continue;
        }

        if update {
            *valid_from = *idx;
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

#[derive(Clone, Debug)]
struct CharMatch {
    chr: char,
    indices: Vec<usize>,
    // TODO point to indices in array for children
}

/// Begin - end
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
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
        let a = self.end - self.start;
        let b = other.end - other.start;

        a.cmp(&b)
    }
}

impl PartialOrd for Streak {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl CharMatch {
    fn new(chr: char) -> Self {
        CharMatch {
            chr,
            indices: vec![],
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::*;

    #[test]
    fn test_one() {
        // TODO test individual scores

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
        //assert!(score_b < score_a, "Wrong order: {}, {}", score_b, score_a);
    }

    //#[test]
    //fn test_if_children_correctly_added() {
    //// TODO
    //let a = "Projects/config/nvim";
    //let b = "Projects/neovim";

    //let pattern = "pr";

    //let score_a = determine_score(pattern, a);
    //let score_b = determine_score(pattern, b);

    //assert_eq!(score_a, score_b);
    //}

    //#[test]
    //fn test_complex_pattern() {
    //// TODO
    //let a = "Projects/config/nvim";
    ////let b = "Projects/neovim";

    //let pattern = "proconnv";

    //let score_a = determine_score(pattern, a);
    ////let score_b = determine_score(pattern, b);

    ////assert!(score_a < score_b, "Wrong order: {}, {}", score_a, score_b);
    //}

    //#[test]
    //fn test_two_peaks() {
    //// TODO test individual scores

    //let a = "projects/neovim";
    //let b = "pgrojects/neovim";

    //let pattern = "prvim";

    //let score_a = determine_score(pattern, a);
    //let score_b = determine_score(pattern, b);
    //assert!(score_a < score_b, "Wrong order: {}, {}", score_a, score_b);
    ////assert!(score_b < score_a, "Wrong order: {}, {}", score_b, score_a);
    //}
}
