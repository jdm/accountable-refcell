# Accountable RefCell

This crate wraps the standard library's [RefCell](https://doc.servo.org/core/cell/struct.RefCell.html) type while making it easier to deal with dynamic borrow failures. Each immutable or mutable borrow of the cell records the stack trace of the code that performed the borrow, and this record is destroyed when the borrow ends. In the event of a dynamic borrow failure (either a mutable borrow while there are outstanding immutable borrows, or an immutable borrow while there is an outstanding mutable borrow), the conflicting stack traces will be printed to stderr if the RUST_BACKTRACE environment variable is present.

Example (two outstanding immutable borrows when a mutable borrow is attempted):
```
Outstanding borrows:
   1: _$LT$accountable_refcell..RefCell$LT$T$GT$$GT$::borrow::h93de6dc5716214a2
   2: accountable_refcell::tests::cannot_borrow_mutably_multi_borrow::hd9184755b4f98dae
   3: _$LT$F$u20$as$u20$test..FnBox$LT$T$GT$$GT$::call_box::h30f93c5e44004cdd (.llvm.6B7221EB)
   4: __rust_maybe_catch_panic
   5: std::sys_common::backtrace::__rust_begin_short_backtrace::ha384908c78afca63
   6: std::panicking::try::do_call::h7da6a9b8bfb2762c (.llvm.E1945E4B)
   7: __rust_maybe_catch_panic
   8: _$LT$F$u20$as$u20$alloc..boxed..FnBox$LT$A$GT$$GT$::call_box::hd53410bd165f5d82 (.llvm.B1C468B7)
   9: std::sys::imp::thread::Thread::new::thread_start::hf16f292ea51f5fa0
  10: _pthread_body
  11: _pthread_start

   1: _$LT$accountable_refcell..RefCell$LT$T$GT$$GT$::borrow::h93de6dc5716214a2
   2: accountable_refcell::tests::cannot_borrow_mutably_multi_borrow::hd9184755b4f98dae
   3: _$LT$F$u20$as$u20$test..FnBox$LT$T$GT$$GT$::call_box::h30f93c5e44004cdd (.llvm.6B7221EB)
   4: __rust_maybe_catch_panic
   5: std::sys_common::backtrace::__rust_begin_short_backtrace::ha384908c78afca63
   6: std::panicking::try::do_call::h7da6a9b8bfb2762c (.llvm.E1945E4B)
   7: __rust_maybe_catch_panic
   8: _$LT$F$u20$as$u20$alloc..boxed..FnBox$LT$A$GT$$GT$::call_box::hd53410bd165f5d82 (.llvm.B1C468B7)
   9: std::sys::imp::thread::Thread::new::thread_start::hf16f292ea51f5fa0
  10: _pthread_body
  11: _pthread_start

thread 'tests::cannot_borrow_mutably_multi_borrow' panicked at 'RefCell is already immutably borrowed.', src/lib.rs:170:12
```

The public API of this crate's RefCell types mirrors the public API of std::cell::RefCell.