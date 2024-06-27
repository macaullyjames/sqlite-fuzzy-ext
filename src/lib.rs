use std::{
    cell::{Ref, RefCell, RefMut}, ffi::{c_char, c_int}, fmt, iter::Skip, ops::Deref, rc::Rc, slice::Iter, str::Chars
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

            let mut previous = previous.get_or_insert(create(previous_chr)).clone();
            let current = current.get_or_insert(create(current_chr)).clone();

            if !previous.rent().has_child(current_chr) {
                previous.rent_mut().children.push(current.clone());
            }
        }
    }

    for (idx, chr) in text.char_indices() {
        let mut visited = vec![];
        if let Some(item) = CharMatch::find_item(&root, chr, &mut visited) {
            let mut current = item.deref().borrow_mut();
            current.indices.push(idx);
        }
    }

    let mut streaks = vec![];

    let iter = pattern.chars().skip(1);
    CharMatch::add_streaks(&root, iter, &mut streaks);

    let mut visited = vec![];
    CharMatch::print(&root, &mut visited);

    println!("{streaks:?}");

    0
}

pub trait ShortRef<T> {
    fn rent(&self) -> Ref<T>;
    fn rent_mut(&mut self) -> RefMut<T>;
}

impl<T> ShortRef<T> for Rc<RefCell<T>> {
    fn rent(&self) -> Ref<T> {
        self.as_ref().borrow()
    }

    fn rent_mut(&mut self) -> RefMut<T> {
        self.borrow_mut()
    }
}

#[derive(Clone)]
struct CharMatch {
    chr: char,
    indices: Vec<usize>,
    children: Vec<Rc<RefCell<CharMatch>>>,
}

/// Begin - end
#[derive(Clone, Copy, Debug)]
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
}

impl fmt::Debug for CharMatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Please use the print function on CharMatch")
    }
}

impl CharMatch {
    fn new(chr: char) -> Self {
        CharMatch {
            chr,
            indices: vec![],
            children: vec![],
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
        let item: Ref<CharMatch> = current.rent();

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

    fn print(current: &Rc<RefCell<CharMatch>>, visited: &mut Vec<char>) {
        let item = current.rent();

        if visited.contains(&item.chr) {
            return;
        }

        visited.push(item.chr);

        println!("item: {}", item.chr);
        println!("indices: {:?}", item.indices);

        let children: Vec<char> = item
            .children
            .iter()
            .map(|child| child.rent().chr)
            .collect();
        println!("children: {:?}", children);
        println!("\n");

        for child in item.children.iter() {
            Self::print(child, visited);
        }
    }

    fn add_streaks(
        current: &Rc<RefCell<CharMatch>>,
        mut iter: Skip<Chars>,
        streaks: &mut Vec<Streak>,
    ) {
        let item = current.deref().borrow();

        for idx in item.indices.iter() {
            for streak in streaks.iter_mut() {
                if streak.try_extend(*idx) {
                    break;
                }
            }

            streaks.push(Streak::new(*idx));
        }

        if let Some(chr) = iter.next() {
            // TODO check on self instead of child
            let child = item
                .children
                .iter()
                .find(|child| child.rent().chr == chr)
                .unwrap();

            Self::add_streaks(child, iter, streaks);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::*;

    #[test]
    fn test_one_peak() {
        // TODO test individual scores

        let a = "Projects/confvig/nvim";
        let b = "Projects/neovim";

        let pattern = "nvnim";

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

    #[test]
    fn test_if_children_correctly_added() {
        // TODO
    }

    #[test]
    fn test_if_indices_correctly_added() {
        // TODO
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
