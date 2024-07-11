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

    let score = calculate_score(&pattern, &text);
    let out = ToSqlOutput::Owned(Value::Integer(score));

    return Ok(out);
}

fn calculate_score(pattern: &str, text: &str) -> i64 {
    if pattern.is_empty() {
        text.len() as i64
    } else if text == pattern {
        -10_000
    } else {
        let text_matches = create_matches(&pattern, &text);

        if is_valid_match(&text_matches) {
            -highest_score(text_matches, &text)
        } else {
            10_000
        }
    }
}

fn highest_score(text_matches: Vec<Option<CharMatch>>, text: &str) -> i64 {
    let mut highest_score = 0;
    let mut i = 0;

    let direct_bonus = if text.ends_with('/') {
        let mut text = text.to_string();
        text.pop();

        !text.contains('/')
    } else {
        !text.contains('/')
    };

    while i < text_matches.len() {
        if let Some(current) = &text_matches[i] {
            let mut splits = current.split_indices();
            let mut streak_len = 0;

            for other in text_matches.iter().skip(i + 1) {
                if let Some(other) = other {
                    splits.retain_mut(|streak| other.try_extend(streak, &mut streak_len));
                } else {
                    if 0 < streak_len {
                        let score = streak_score(i, streak_len, text_matches.len(), direct_bonus);
                        highest_score = highest_score.max(score);
                    }
                    break;
                }
            }
        }

        i += 1;
    }

    highest_score
}

fn streak_score(idx: usize, streak_len: usize, text_len: usize, direct_bonus: bool) -> i64 {
    //let direct_bonus = if self.direct_bonus { 200 } else { 0 };
    // TODO direct bonus
    let len_bonus = streak_len as f32 / text_len as f32 * 200.;
    //println!("lenb: {len_bonus}");
    let last_idx = idx + streak_len;
    let end_bonus = last_idx as f32 / text_len as f32 * 100.;
    //println!("endb: {end_bonus}");

    let direct_bonus = if direct_bonus { 100. } else { 0. };
    //println!("dir: {direct_bonus}");

    (streak_len as f32 * 50. + len_bonus + end_bonus + direct_bonus) as i64
}

fn is_valid_match(text_matches: &Vec<Option<CharMatch>>) -> bool {
    text_matches.iter().any(|x| x.is_some())
}

fn create_matches(pattern: &str, text: &str) -> Vec<Option<CharMatch>> {
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
                match tex_match.valid_before(i) {
                    Ordering::Less => {
                        //println!("low: less: {:?}, {}", tex_match, i);
                    }
                    Ordering::Equal => {
                        start_from = text_i + 1;
                        break;
                    }
                    Ordering::Greater => {
                        delete = true;
                        //println!("low: greater: {:?}, {}", tex_match, i);
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
                match tex_match.valid_after(i) {
                    Ordering::Less => {
                        //println!("lesser: {:?}, {}", tex_match, i);
                        delete = true;
                    }
                    Ordering::Equal => {
                        start_from = text_i;
                        break;
                    }
                    Ordering::Greater => {
                        //println!("greater: {:?}, {}", tex_match, i);
                    }
                }
            }

            if delete {
                text_matches[text_i] = None;
            }
        }
    }

    //println!("{text_matches:?}");

    text_matches
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

    fn valid_before(&self, idx: usize) -> Ordering {
        let mut ordering = Ordering::Greater;

        for pattern_idx in self.iter() {
            if *pattern_idx == idx {
                return Ordering::Equal;
            } else if *pattern_idx < idx {
                ordering = Ordering::Less;
            }
        }

        ordering
    }

    fn valid_after(&self, idx: usize) -> Ordering {
        for pattern_idx in self.iter() {
            if *pattern_idx == idx {
                return Ordering::Equal;
            } else if idx < *pattern_idx {
                return Ordering::Greater;
            }
        }

        Ordering::Less
    }

    fn split_indices(&self) -> Vec<Vec<usize>> {
        self.iter().map(|i| vec![*i]).collect()
    }

    fn try_extend(&self, streak: &mut Vec<usize>, max_len: &mut usize) -> bool {
        let last = streak[streak.len() - 1];

        for p_idx in self.iter() {
            if last + 1 == *p_idx {
                streak.push(*p_idx);
                *max_len = max_len.clone().max(streak.len());
                return true;
            }
        }

        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_one() {
        let a = "Projects/config/nvim";
        let b = "Projects/neovim";

        let pattern = "convim";

        let score_a = calculate_score(pattern, a);
        let score_b = calculate_score(pattern, b);

        assert!(score_a < score_b, "Wrong order: {}, {}", score_a, score_b);
    }

    #[test]
    fn test_upper_bad() {
        let a = "Projects/config/nvim";

        let pattern = "PRnvim";

        let text_matches = create_matches(pattern, a);

        assert!(!is_valid_match(&text_matches));
    }

    #[test]
    fn test_search() {
        let a = "Projects/nvim-traveller-rs";
        let b = "/home/norlock/bin/google-cloud-sdk/lib/third_party/setuptools/_vendor/importlib_resources-5.10.2.dist-info";
        let pattern = "nvim-t-";

        println!("A: {a}");
        println!("b: {b}");
        println!("Pattern: {pattern}");

        let score_a = calculate_score(pattern, a);
        let score_b = calculate_score(pattern, b);

        assert!(score_a < score_b, "Wrong order: {}, {}", score_a, score_b);
    }

    #[test]
    fn test_complex_pattern() {
        let a = "Projects/config/nvim";
        let b = "Projects/neovim";

        let pattern = "prnvi";
        println!("A: {a}");
        println!("b: {b}");
        println!("Pattern: {pattern}");

        let score_a = calculate_score(pattern, a);
        let score_b = calculate_score(pattern, b);

        assert!(score_a < score_b, "Wrong order: {}, {}", score_a, score_b);
    }

    #[test]
    fn test_recurring_char() {
        let a = "Proasdlmasd/o";

        let pattern = "olmo";
        println!("{pattern}");

        let text_matches = create_matches(pattern, a);

        assert!(is_valid_match(&text_matches));
    }

    #[test]
    fn test_short() {
        let a = "services/update.yaml";
        let b = "gateways/delete.yaml";

        let pattern = "de";

        let score_a = calculate_score(pattern, a);
        let score_b = calculate_score(pattern, b);

        assert!(
            score_b < score_a,
            "Wrong order: {:?}, {:?}",
            score_b,
            score_a
        );
    }

    #[test]
    fn test_database() {
        let a = "Projects/neo-api-rs/database.rs";
        let b = "Android/Sdk/platform-tools/fastboot";

        let pattern = "datab";

        let score_a = calculate_score(pattern, a);
        let score_b = calculate_score(pattern, b);

        //assert!(false);
        assert!(
            score_a < score_b,
            "Wrong order: {:?}, {:?}",
            score_a,
            score_b
        );
    }

    #[test]
    fn test_neo() {
        let a = "Projects/neovim/";
        let b = "Projects/neo-api-rs/";
        let c = "bin/google-cloud-sdk/lib/surface/monitoring/snoozes/";

        let pattern = "neo";

        let score_a = calculate_score(pattern, a);
        println!("A: {:?}", score_a);
        let score_b = calculate_score(pattern, b);
        println!("B: {:?}", score_b);
        let score_c = calculate_score(pattern, c);
        println!("C: {:?}", score_c);

        assert!(
            score_a < score_b && score_b < score_c,
            "Wrong order: {:?}, {:?}, {:?}",
            score_a,
            score_b,
            score_c,
        );
    }
}
