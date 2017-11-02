/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

extern crate backtrace;

use backtrace::Backtrace;
use std::cell::{Cell, RefCell as StdRefCell, Ref as StdRef, RefMut as StdRefMut};
use std::env;
use std::fmt::{Display, Debug, Formatter, Error};
use std::ops::{Deref, DerefMut};

/// A RefCell that tracks outstanding borrows and reports stack traces for dynamic borrow failures.
pub struct RefCell<T: ?Sized> {
    borrows: StdRefCell<Vec<BorrowRecord>>,
    next_id: Cell<usize>,
    inner: StdRefCell<T>,
}

struct BorrowRecord {
    id: usize,
    backtrace: Backtrace,
}

impl<T> RefCell<T> {
    /// Create a new RefCell value.
    pub fn new(value: T) -> RefCell<T> {
        RefCell {
            inner: StdRefCell::new(value),
            borrows: StdRefCell::new(vec![]),
            next_id: Cell::new(0),
        }
    }

    /// Discard this RefCell and return the value stored inside of it.
    pub fn into_inner(self) -> T {
        self.inner.into_inner()
    }
}

/// An immutable reference to the value stored in a RefCell.
pub struct Ref<'a, T: ?Sized + 'a> {
    inner: StdRef<'a, T>,
    cell: &'a RefCell<T>,
    id: usize,
}

impl<'a, T: ?Sized> Ref<'a, T> {
    /// Clone the provided Ref value. This is treated as a separate borrow record from
    /// the original cloned reference.
    pub fn clone(orig: &Ref<'a, T>) -> Ref<'a, T> {
        let id = orig.cell.record();
        Ref {
            inner: StdRef::clone(&orig.inner),
            cell: orig.cell,
            id: id,
        }
    }
}

impl<'a, T: ?Sized + Display> Display for Ref<'a, T> {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        self.inner.fmt(f)
    }
}

impl<'b, T: ?Sized + Debug> Debug for Ref<'b, T> {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        self.inner.fmt(f)
    }
}

impl<'a, T: ?Sized> Deref for Ref<'a, T> {
    type Target = T;

    fn deref(&self) -> &T {
        &*self.inner
    }
}

impl<'a, T: ?Sized> Drop for Ref<'a, T> {
    fn drop(&mut self) {
        self.cell.remove_matching_record(self.id);
    }
}

/// A mutable reference to the value stored in the associated RefCell.
pub struct RefMut<'a, T: ?Sized + 'a> {
    inner: StdRefMut<'a, T>,
    cell: &'a RefCell<T>,
    id: usize,
}

impl<'a, T: ?Sized> Deref for RefMut<'a, T> {
    type Target = T;

    fn deref(&self) -> &T {
        &*self.inner
    }
}

impl<'a, T: ?Sized> DerefMut for RefMut<'a, T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut *self.inner
    }
}


impl<'a, T: ?Sized> Drop for RefMut<'a, T> {
    fn drop(&mut self) {
        self.cell.remove_matching_record(self.id);
    }
}

impl<T: ?Sized> RefCell<T> {
    fn remove_matching_record(&self, id: usize) {
        let idx = self.borrows.borrow().iter().position(|record| record.id == id);
        self.borrows.borrow_mut().remove(idx.expect("missing borrow record"));
    }
    
    #[inline(always)]
    fn record(&self) -> usize {
        let id = self.next_id();
        self.borrows.borrow_mut().push(BorrowRecord {
            id: id,
            backtrace: Backtrace::new(),
        });
        id
    }

    fn next_id(&self) -> usize {
        let id = self.next_id.get();
        self.next_id.set(id.wrapping_add(1));
        id
    }

    /// Borrow the value stored in this cell immutably. Panics if any outstanding mutable
    /// borrows of the same cell exist.
    pub fn borrow(&self) -> Ref<T> {
        if let Ok(r) = self.inner.try_borrow() {
            let id = self.record();
            Ref {
                inner: r,
                cell: self,
                id: id,
            }
        } else {
            if let Ok(var) = env::var("RUST_BACKTRACE") {
                if !var.is_empty() {
                    eprintln!("Outstanding borrow:");
                    print_filtered_backtrace(&self.borrows.borrow()[0].backtrace);
                }
            }
            panic!("RefCell is already mutably borrowed.");
        }
    }

    /// Borrow the value stored in this cell mutably. Panics if any outstanding immutable
    /// borrows of the same cell exist.
    pub fn borrow_mut(&self) -> RefMut<T> {
        if let Ok(r) = self.inner.try_borrow_mut() {
            let id = self.record();
            RefMut {
                inner: r,
                cell: self,
                id: id,
            }
        } else {
            if let Ok(var) = env::var("RUST_BACKTRACE") {
                if !var.is_empty() {
                    eprintln!("Outstanding borrows:");
                    for borrow in &*self.borrows.borrow() {
                        print_filtered_backtrace(&borrow.backtrace);
                        eprintln!("");
                    }
                }
            }
            panic!("RefCell is already immutably borrowed.");
        }
    }
}

/// Print a backtrace without any frames from the backtrace library.
fn print_filtered_backtrace(backtrace: &Backtrace) {
    let mut idx = 1;
    for frame in backtrace.frames().iter() {
        let symbol = frame.symbols().first();
        let repr = match symbol {
            None => "<no-info>".to_owned(),
            Some(symbol) => {
                let mut repr = if let Some(name) = symbol.name() {
                    if name.as_str().unwrap_or("").starts_with("backtrace::") {
                        continue;
                    }
                    name.as_str().unwrap_or("").to_owned()
                } else {
                    "<unknown>".to_owned()
                };
                if let (Some(file), Some(line)) = (symbol.filename(), symbol.lineno()) {
                    repr.push_str(&format!(" at {:?}:{}", file, line));
                }
                repr
            }
        };
        eprintln!("{:4}: {}", idx, repr);
        idx += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::{RefCell, Ref};

    #[test]
    #[should_panic(expected = "RefCell is already immutably borrowed")]
    fn cannot_borrow_mutably() {
        let c = RefCell::new(5);
        let _b = c.borrow();
        let _b2 = c.borrow_mut();
    }

    #[test]
    #[should_panic(expected = "RefCell is already mutably borrowed")]
    fn cannot_borrow_immutably() {
        let c = RefCell::new(5);
        let _b = c.borrow_mut();
        let _b2 = c.borrow();
    }

    #[inline(never)]
    fn borrow_immutably<T>(cell: &RefCell<T>) -> Ref<T> {
        cell.borrow()
    }

    #[test]
    #[should_panic]
    fn cannot_borrow_mutably_multi_borrow() {
        let c = RefCell::new(5);
        let _b = borrow_immutably(&c);
        let _b2 = borrow_immutably(&c);
        let _b2 = c.borrow_mut();
    }

    #[test]
    #[should_panic]
    fn clone_records_borrow() {
        let c = RefCell::new(5);
        let _b2 = {
            let _b = borrow_immutably(&c);
            Ref::clone(&_b)
        };
        let _b2 = c.borrow_mut();
    }
}
