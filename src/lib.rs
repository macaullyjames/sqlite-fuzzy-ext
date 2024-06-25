use std::{
    borrow::Borrow, cell::{Ref, RefCell}, ffi::{c_char, c_int}, fmt, ops::Deref, rc::Rc, slice::Iter
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
        return -1_000_000;
    }

    fn create(chr: char) -> Rc<RefCell<CharMatch>> {
        Rc::new(RefCell::new(CharMatch::new(chr)))
    }

    let first_chr = pattern.chars().nth(0).unwrap();
    let root = create(first_chr);

    if 1 < pattern.chars().count() {
        for i in 1..pattern.chars().count() {
            let current_chr = pattern.chars().nth(i).unwrap();
            let previous_chr = pattern.chars().nth(i - 1).unwrap();

            if previous_chr == current_chr {
                continue;
            }

            let mut visited = vec![];
            let mut previous = CharMatch::find_item(&root, previous_chr, &mut visited);
            let mut visited = vec![];
            let mut current = CharMatch::find_item(&root, current_chr, &mut visited);

            let previous = previous.get_or_insert(create(previous_chr)).clone();
            let current = current.get_or_insert(create(current_chr)).clone();

            if !previous.deref().borrow().has_child(current_chr) {
                previous.borrow_mut().children.push(current.clone());
            }
        }
    }

    //let mut streaks = vec![];

    //CharMatch::add_streaks(&all_matches, &mut streaks);

    //println!("{all_matches:?}");
    //println!("{root:?}");
    let mut visited = vec![];
    CharMatch::print(&root, &mut visited);

    0
}

#[derive(Clone)]
struct CharMatch {
    chr: char,
    indices: Vec<usize>,
    children: Vec<Rc<RefCell<CharMatch>>>,
}

impl CharMatch {
    fn new(chr: char) -> Self {
        CharMatch {
            chr,
            indices: vec![],
            children: vec![],
        }
    }

    fn print(current: &Rc<RefCell<CharMatch>>, visited: &mut Vec<char>) {
        let item = current.deref().borrow();

        if visited.contains(&item.chr) {
            return;
        }

        visited.push(item.chr);

        println!("item: {}", item.chr);
        println!("indices: {:?}", item.indices);

        let children: Vec<char> = item.children.iter().map(|child| child.deref().borrow().chr).collect();
        println!("children: {:?}", children);
        println!("\n");

        for child in item.children.iter() {
            Self::print(child, visited);
        }

    }

    fn has_child(&self, chr: char) -> bool {
        self.children
            .iter()
            .any(|child| child.deref().borrow().chr == chr)
    }

    fn find_item(
        current: &Rc<RefCell<CharMatch>>,
        chr: char,
        visited: &mut Vec<char>,
    ) -> Option<Rc<RefCell<CharMatch>>> {
        let item: Ref<CharMatch> = current.deref().borrow();

        if visited.contains(&item.chr) {
            return None;
        }

        if item.chr == chr {
            return Some(current.clone());
        } else {
            visited.push(item.chr);

            for child in item.children.iter() {
                let child_ref: Ref<CharMatch> = child.deref().borrow();

                if child_ref.chr == chr {
                    return Some(child.clone());
                } else if let Some(item) = Self::find_item(child, chr, visited) {
                    return Some(item);
                }
            }
        }

        None
    }

    //fn insert(&mut self, chr: char, previous_chr: char) {
    //if self.chr == previous_chr {
    //if !self.has_child(chr) {
    //let child = Rc::new(RefCell::new(CharMatch::new(chr)));
    //self.children.push(child);
    //}
    //} else {
    //for child in self.children.iter_mut() {
    //let child = child.clone();

    //child.borrow_mut().insert(chr, previous_chr);
    //}
    //}
    //}

    fn add_streaks(all_matches: &[CharMatch]) {
        //if all_matches.is_empty() {
        //return;
        //}

        //let mut iter = all_matches.iter();

        //if let Some(next) = iter.next() {
        //for s_idx in next.indexes.iter() {
        //if let Some(len) = next.calc_len(1, iter.clone()) {
        //streaks.push(Streak { len, idx: *s_idx })
        //}
        //}
        //}
    }

    fn calc_len(&self, len: usize, mut iter: Iter<CharMatch>) {
        //println!("{iter:?} ");
        //for s_idx in self.indexes.iter() {
        //if let Some(next) = iter.next() {
        //let is_connected = next.indexes.iter().any(|o_idx| *s_idx + 1 == *o_idx);

        //if is_connected {
        //next.calc_len(len + 1, iter);
        //} else {
        //return Some(len);
        //}
        //} else {
        //return streaks.push(Streak {
        //len: len + 1,
        //idx: s_idx - (len + 1),
        //});
        //}
        //}

        //

        //let last = self.indexes.last()?;
        //let valid = o_idx < *last;
        //if valid {
        //Some(len)
        //} else {
        //None
        //}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_one_peak() {
        // TODO test individual scores

        let a = "Projects/config/nvim";
        let b = "Projects/neovim";

        let pattern = "nvnim";

        let score_a = determine_score(pattern, a);
        let score_b = determine_score(pattern, b);
        assert!(score_a < score_b, "Wrong order: {}, {}", score_a, score_b);
        //assert!(score_b < score_a, "Wrong order: {}, {}", score_b, score_a);
    }

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
