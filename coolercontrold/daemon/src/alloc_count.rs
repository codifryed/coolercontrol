// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Test-only global allocator that counts heap allocations per thread, so a test can assert that
//! a hot path allocates nothing. Each test runs on its own thread, so parallel tests never mix
//! their counts.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

thread_local! {
    static ALLOCATIONS: Cell<u64> = const { Cell::new(0) };
}

struct CountingAllocator;

// SAFETY: every call is forwarded to `System` unchanged; this only counts.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        count_one();
        // SAFETY: the caller upholds the `GlobalAlloc::alloc` contract.
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        count_one();
        // SAFETY: the caller upholds the `GlobalAlloc::alloc_zeroed` contract.
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        count_one();
        // SAFETY: the caller upholds the `GlobalAlloc::realloc` contract.
        unsafe { System.realloc(ptr, layout, new_size) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: the caller upholds the `GlobalAlloc::dealloc` contract.
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static GLOBAL: CountingAllocator = CountingAllocator;

fn count_one() {
    // `try_with`: the slot is gone while the thread tears down.
    let _ = ALLOCATIONS.try_with(|count| count.set(count.get() + 1));
}

/// Runs `f` and returns its result with the number of heap allocations (including reallocations)
/// it made on this thread.
pub fn count_allocations<R>(f: impl FnOnce() -> R) -> (R, u64) {
    let before = ALLOCATIONS.with(Cell::get);
    let result = f();
    let after = ALLOCATIONS.with(Cell::get);
    assert!(after >= before);
    (result, after - before)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_allocations_made_inside_the_closure_only() {
        // Goal: the counter sees what the closure allocates and nothing else.
        // A String allocated outside is not counted, a Vec allocated inside is, and
        // a closure that only reads allocates nothing.
        let outside = String::from("allocated before");
        let ((), none) = count_allocations(|| assert_eq!(outside.len(), 16));
        assert_eq!(none, 0);
        let (inside, one) = count_allocations(|| vec![1_u8; 16]);
        assert_eq!(inside.len(), 16);
        assert_eq!(one, 1);
    }
}
