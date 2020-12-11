/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

extern crate backtrace;

use backtrace::Backtrace;
use std::cell::{
    BorrowError, BorrowMutError, Ref as StdRef, RefCell as StdRefCell, RefMut as StdRefMut,
};
use std::fmt::{Debug, Display, Error, Formatter};
use std::ops::{Deref, DerefMut};
use std::{env, mem};

/// A RefCell that tracks outstanding borrows and reports stack traces for dynamic borrow failures.
#[derive(Debug)]
pub struct RefCell<T: ?Sized> {
    borrows: StdRefCell<BorrowData>,
    inner: StdRefCell<T>,
}

#[derive(Debug)]
struct BorrowData {
    next_id: usize,
    borrows: Vec<BorrowRecord>,
}

#[derive(Debug)]
struct BorrowRecord {
    id: usize,
    backtrace: Backtrace,
}

impl BorrowData {
    fn record(&mut self) -> usize {
        let id = self.next_id();
        self.borrows.push(BorrowRecord {
            id: id,
            backtrace: Backtrace::new(),
        });
        id
    }

    fn next_id(&mut self) -> usize {
        let id = self.next_id;
        self.next_id = id.wrapping_add(1);
        id
    }

    fn remove_matching_record(&mut self, id: usize) {
        let idx = self.borrows.iter().position(|record| record.id == id);
        self.borrows.remove(idx.expect("missing borrow record"));
    }
}

impl<T> RefCell<T> {
    /// Create a new RefCell value.
    pub fn new(value: T) -> RefCell<T> {
        RefCell {
            inner: StdRefCell::new(value),
            borrows: StdRefCell::new(BorrowData {
                borrows: vec![],
                next_id: 0,
            }),
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
    data: RefBorrowData<'a>,
}

impl<'a, T: ?Sized> Ref<'a, T> {
    /// Clone the provided Ref value. This is treated as a separate borrow record from
    /// the original cloned reference.
    pub fn clone(orig: &Ref<'a, T>) -> Ref<'a, T> {
        let id = orig.data.cell.borrow_mut().record();
        Ref {
            inner: StdRef::clone(&orig.inner),
            data: RefBorrowData {
                cell: orig.data.cell,
                id: id,
            },
        }
    }

    pub fn map<U: ?Sized, F>(orig: Ref<'a, T>, f: F) -> Ref<'a, U>
    where
        F: FnOnce(&T) -> &U,
    {
        Ref {
            inner: StdRef::map(StdRef::clone(&orig.inner), f),
            data: orig.data,
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

/// A mutable reference to the value stored in the associated RefCell.
pub struct RefMut<'a, T: ?Sized + 'a> {
    inner: StdRefMut<'a, T>,
    data: RefBorrowData<'a>,
}

struct RefBorrowData<'a> {
    cell: &'a StdRefCell<BorrowData>,
    id: usize,
}

impl<'a> Drop for RefBorrowData<'a> {
    fn drop(&mut self) {
        self.cell.borrow_mut().remove_matching_record(self.id);
    }
}

impl<'a, T: ?Sized> RefMut<'a, T> {
    pub fn map<U: ?Sized, F>(orig: RefMut<'a, T>, f: F) -> RefMut<'a, U>
    where
        F: FnOnce(&mut T) -> &mut U,
    {
        let RefMut { inner, data } = orig;
        RefMut {
            inner: StdRefMut::map(inner, f),
            data,
        }
    }
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

impl<T: ?Sized> RefCell<T> {
    /// Borrow the value stored in this cell immutably. Panics if any outstanding mutable
    /// borrows of the same cell exist.
    pub fn borrow(&self) -> Ref<'_, T> {
        if let Ok(r) = self.inner.try_borrow() {
            let id = self.borrows.borrow_mut().record();
            Ref {
                inner: r,
                data: RefBorrowData {
                    cell: &self.borrows,
                    id: id,
                },
            }
        } else {
            if let Ok(var) = env::var("RUST_BACKTRACE") {
                if !var.is_empty() {
                    eprintln!("Outstanding borrow:");
                    print_filtered_backtrace(&self.borrows.borrow().borrows[0].backtrace);
                }
            }
            panic!("RefCell is already mutably borrowed.");
        }
    }

    pub fn try_borrow(&self) -> Result<Ref<T>, BorrowError> {
        self.inner.try_borrow().map(|r| {
            let id = self.borrows.borrow_mut().record();
            Ref {
                inner: r,
                data: RefBorrowData {
                    cell: &self.borrows,
                    id: id,
                },
            }
        })
    }

    /// Borrow the value stored in this cell mutably. Panics if there are any other outstanding
    /// borrows of this cell (mutable borrows are unique, i.e. there can only be one).
    pub fn borrow_mut(&self) -> RefMut<T> {
        if let Ok(r) = self.inner.try_borrow_mut() {
            let id = self.borrows.borrow_mut().record();
            RefMut {
                inner: r,
                data: RefBorrowData {
                    cell: &self.borrows,
                    id: id,
                },
            }
        } else {
            if let Ok(var) = env::var("RUST_BACKTRACE") {
                if !var.is_empty() {
                    eprintln!("Outstanding borrows:");
                    for borrow in &*self.borrows.borrow().borrows {
                        print_filtered_backtrace(&borrow.backtrace);
                        eprintln!("");
                    }
                }
            }
            panic!("RefCell is already borrowed.");
        }
    }

    pub fn try_borrow_mut(&self) -> Result<RefMut<'_, T>, BorrowMutError> {
        self.inner.try_borrow_mut().map(|r| {
            let id = self.borrows.borrow_mut().record();
            RefMut {
                inner: r,
                data: RefBorrowData {
                    cell: &self.borrows,
                    id: id,
                },
            }
        })
    }

    pub fn as_ptr(&self) -> *mut T {
        self.inner.as_ptr()
    }

    pub unsafe fn try_borrow_unguarded(&self) -> Result<&T, BorrowError> {
        self.inner.try_borrow_unguarded()
    }
}

impl<T> RefCell<T> {
    /// Corresponds to https://doc.rust-lang.org/std/cell/struct.RefCell.html#method.replace.
    pub fn replace(&self, t: T) -> T {
        mem::replace(&mut *self.borrow_mut(), t)
    }

    /// Corresponds to https://doc.rust-lang.org/std/cell/struct.RefCell.html#method.replace_with.
    pub fn replace_with<F: FnOnce(&mut T) -> T>(&self, f: F) -> T {
        let mut_borrow = &mut *self.borrow_mut();
        let replacement = f(mut_borrow);
        mem::replace(mut_borrow, replacement)
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

impl<T: Clone> Clone for RefCell<T> {
    fn clone(&self) -> RefCell<T> {
        RefCell::new(self.borrow().clone())
    }
}

impl<T: Default> RefCell<T> {
    /// Corresponds to https://doc.rust-lang.org/std/cell/struct.RefCell.html#method.take.
    pub fn take(&self) -> T {
        self.replace(Default::default())
    }
}

impl<T: Default> Default for RefCell<T> {
    fn default() -> RefCell<T> {
        RefCell::new(Default::default())
    }
}

impl<T: ?Sized + PartialEq> PartialEq for RefCell<T> {
    fn eq(&self, other: &RefCell<T>) -> bool {
        *self.borrow() == *other.borrow()
    }
}

pub fn ref_filter_map<T: ?Sized, U: ?Sized, F: FnOnce(&T) -> Option<&U>>(
    orig: Ref<T>,
    f: F,
) -> Option<Ref<U>> {
    f(&orig)
        .map(|new| new as *const U)
        .map(|raw| Ref::map(orig, |_| unsafe { &*raw }))
}

pub fn ref_mut_filter_map<T: ?Sized, U: ?Sized, F: FnOnce(&mut T) -> Option<&mut U>>(
    mut orig: RefMut<T>,
    f: F,
) -> Option<RefMut<U>> {
    f(&mut orig)
        .map(|new| new as *mut U)
        .map(|raw| RefMut::map(orig, |_| unsafe { &mut *raw }))
}

#[cfg(test)]
mod tests {
    use super::{Ref, RefCell};

    #[test]
    #[should_panic(expected = "RefCell is already borrowed")]
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

    #[test]
    #[should_panic(expected = "RefCell is already borrowed")]
    fn cannot_double_borrow_mut() {
        let c = RefCell::new(5);
        let _b = c.borrow_mut();
        let _b2 = c.borrow_mut();
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

    #[test]
    fn take_refcell_returns_correct_value() {
        let c: RefCell<i32> = RefCell::new(5);
        assert_eq!(5, c.take());
        assert_eq!(i32::default(), *c.borrow());
    }

    #[test]
    #[should_panic(expected = "RefCell is already borrowed")]
    fn cannot_take_borrowed_refcell() {
        let c = RefCell::new(5);
        let _b = c.borrow();
        c.take();
    }

    #[test]
    #[should_panic(expected = "RefCell is already borrowed")]
    fn cannot_take_mut_borrowed_refcell() {
        let c = RefCell::new(5);
        let _b = c.borrow_mut();
        c.take();
    }

    #[test]
    fn replace_refcell_properly_replaces_contents() {
        let c = RefCell::new(5);
        c.replace(12);
        assert_eq!(12, *c.borrow());
    }

    #[test]
    #[should_panic(expected = "RefCell is already borrowed")]
    fn cannot_replace_borrowed_refcell() {
        let c = RefCell::new(5);
        let _b = c.borrow();
        c.replace(12);
    }

    #[test]
    #[should_panic(expected = "RefCell is already borrowed")]
    fn cannot_replace_mut_borrowed_refcell() {
        let c = RefCell::new(5);
        let _b = c.borrow_mut();
        c.replace(12);
    }

    #[test]
    fn replace_with_refcell_properly_replaces_contents() {
        let c = RefCell::new(5);
        c.replace_with(|&mut old_value| old_value + 1);
        assert_eq!(6, *c.borrow());
    }

    #[test]
    #[should_panic(expected = "RefCell is already borrowed")]
    fn cannot_replace_with_borrowed_refcell() {
        let c = RefCell::new(5);
        let _b = c.borrow();
        c.replace_with(|&mut old_val| old_val + 1);
    }

    #[test]
    #[should_panic(expected = "RefCell is already borrowed")]
    fn cannot_replace_with_mut_borrowed_refcell() {
        let c = RefCell::new(5);
        let _b = c.borrow_mut();
        c.replace_with(|&mut old_val| old_val + 1);
    }
}
