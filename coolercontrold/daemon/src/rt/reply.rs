// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Reply channels from worker threads that are kept and reused, so a warm round trip allocates
//! nothing, where a oneshot allocates once per request.
//!
//! A slot goes back to its pool only once its reply has been read. A request given up on (timed
//! out, cancelled) drops its half instead, so the worker's late reply lands in a slot no later
//! request will ever read, which is freed when the worker lets go of it.

use futures_util::task::AtomicWaker;
use std::cell::RefCell;
use std::future::Future;
use std::ops::Not;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::task::{Context, Poll};

/// Spare slots kept per pool. One per request in flight at once is enough; more are dropped.
const SPARE_MAX: usize = 8;

/// The worker dropped its sender without replying: the worker is gone.
#[derive(Debug, PartialEq, Eq)]
pub struct Closed;

struct Slot<T> {
    value: Mutex<Option<T>>,
    /// Set only by a sender dropped without replying.
    closed: AtomicBool,
    /// Set only by a reply dropped before it resolved, so the worker can skip the work.
    abandoned: AtomicBool,
    waker: AtomicWaker,
}

impl<T> Slot<T> {
    fn take(&self) -> Option<T> {
        self.value
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
    }
}

/// The main-thread side: slots whose replies were read, ready for the next request.
pub struct ReplyPool<T> {
    spare: RefCell<Vec<Arc<Slot<T>>>>,
}

impl<T> Default for ReplyPool<T> {
    fn default() -> Self {
        Self {
            spare: RefCell::new(Vec::with_capacity(SPARE_MAX)),
        }
    }
}

impl<T> std::fmt::Debug for ReplyPool<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReplyPool")
            .field("spare", &self.spare.borrow().len())
            .finish()
    }
}

impl<T> ReplyPool<T> {
    /// A reply channel, on a spare slot when there is one.
    pub fn channel(&self) -> (ReplySender<T>, Reply<'_, T>) {
        let slot = self.spare.borrow_mut().pop().unwrap_or_else(|| {
            Arc::new(Slot {
                value: Mutex::new(None),
                closed: AtomicBool::new(false),
                abandoned: AtomicBool::new(false),
                waker: AtomicWaker::new(),
            })
        });
        debug_assert!(slot.closed.load(Ordering::Acquire).not());
        debug_assert!(slot.abandoned.load(Ordering::Acquire).not());
        let sender = ReplySender {
            slot: Some(Arc::clone(&slot)),
        };
        let reply = Reply {
            slot: Some(slot),
            pool: self,
        };
        (sender, reply)
    }

    fn give_back(&self, slot: Arc<Slot<T>>) {
        let mut spare = self.spare.borrow_mut();
        if spare.len() < SPARE_MAX {
            spare.push(slot);
        }
        debug_assert!(spare.len() <= SPARE_MAX);
    }

    #[cfg(test)]
    pub fn spare_count(&self) -> usize {
        self.spare.borrow().len()
    }
}

/// The worker's side: replies once, or reports the worker gone when dropped unsent.
pub struct ReplySender<T> {
    /// `None` once sent.
    slot: Option<Arc<Slot<T>>>,
}

impl<T> std::fmt::Debug for ReplySender<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReplySender")
            .field("sent", &self.slot.is_none())
            .finish()
    }
}

impl<T> ReplySender<T> {
    /// Whether the request was given up on, so its reply would never be read.
    pub fn is_closed(&self) -> bool {
        self.slot
            .as_ref()
            .is_none_or(|slot| slot.abandoned.load(Ordering::Acquire))
    }

    /// Hands the value to the waiting request. A request already given up on never reads it.
    pub fn send(mut self, value: T) {
        // Only `send` and `drop` clear the slot, and both consume the sender.
        let slot = self.slot.take().expect("a sender replies once");
        *slot.value.lock().unwrap_or_else(PoisonError::into_inner) = Some(value);
        slot.waker.wake();
    }
}

impl<T> Drop for ReplySender<T> {
    fn drop(&mut self) {
        // A sent sender touches nothing, so its slot can already be serving the next request.
        let Some(slot) = self.slot.take() else {
            return;
        };
        slot.closed.store(true, Ordering::Release);
        slot.waker.wake();
    }
}

/// Resolves to the worker's reply, or `Closed` when the worker dropped its sender unsent.
pub struct Reply<'pool, T> {
    /// `None` once resolved.
    slot: Option<Arc<Slot<T>>>,
    pool: &'pool ReplyPool<T>,
}

impl<T> Reply<'_, T> {
    fn ready(&mut self) -> Option<Result<T, Closed>> {
        // The slot is cleared only on resolving, and a resolved future is never polled again.
        let slot = self
            .slot
            .as_ref()
            .expect("a reply is polled after it resolved");
        if let Some(value) = slot.take() {
            // Borrowed as `Some` just above, and nothing clears it in between.
            let slot = self.slot.take().expect("checked above");
            self.pool.give_back(slot);
            return Some(Ok(value));
        }
        if slot.closed.load(Ordering::Acquire) {
            // Only an unsent sender closes a slot, so there is no value behind it. Not recycled.
            self.slot = None;
            return Some(Err(Closed));
        }
        None
    }
}

impl<T> Drop for Reply<'_, T> {
    fn drop(&mut self) {
        // A resolved reply has already given its slot back or let it go.
        if let Some(slot) = self.slot.take() {
            slot.abandoned.store(true, Ordering::Release);
        }
    }
}

impl<T> Future for Reply<'_, T> {
    type Output = Result<T, Closed>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        if let Some(ready) = this.ready() {
            return Poll::Ready(ready);
        }
        // `ready` clears the slot only when it resolves, which returned above.
        this.slot
            .as_ref()
            .expect("unresolved")
            .waker
            .register(cx.waker());
        // Asked again: a reply sent between the first look and the register woke nobody.
        match this.ready() {
            Some(ready) => Poll::Ready(ready),
            None => Poll::Pending,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::task::Waker;

    #[test]
    fn a_reply_from_another_thread_arrives() {
        let pool = ReplyPool::default();
        let (sender, reply) = pool.channel();
        let worker = std::thread::spawn(move || sender.send(42_u32));

        let value = crate::rt::test_runtime(reply);

        worker.join().unwrap();
        assert_eq!(value, Ok(42));
    }

    #[test]
    fn a_read_reply_frees_its_slot_for_the_next_request() {
        let pool = ReplyPool::default();
        let (sender, reply) = pool.channel();
        sender.send(1_u32);
        let (value, _) = crate::rt::poll_to_end(reply);
        assert_eq!(value, Ok(1));
        assert_eq!(pool.spare_count(), 1);

        let (sender, reply) = pool.channel();
        assert_eq!(pool.spare_count(), 0);
        sender.send(2);
        assert_eq!(crate::rt::poll_to_end(reply).0, Ok(2));
    }

    #[test]
    fn a_sender_dropped_unsent_closes_the_reply() {
        let pool = ReplyPool::<u32>::default();
        let (sender, reply) = pool.channel();
        drop(sender);

        assert_eq!(crate::rt::poll_to_end(reply).0, Err(Closed));
        assert_eq!(pool.spare_count(), 0, "a closed slot is not reused");
    }

    #[test]
    fn a_late_reply_never_reaches_the_next_request() {
        // Goal: a request given up on must not leave its reply for a later one. Method: give up
        // on the first, start the second, and only then let the first's sender answer.
        let pool = ReplyPool::default();
        let (late_sender, abandoned) = pool.channel();
        drop(abandoned);
        let (sender, reply) = pool.channel();
        late_sender.send("late");
        let mut reply = std::pin::pin!(reply);
        let mut context = Context::from_waker(Waker::noop());

        assert!(reply.as_mut().poll(&mut context).is_pending());
        sender.send("fresh");
        assert_eq!(reply.poll(&mut context), Poll::Ready(Ok("fresh")));
    }

    #[test]
    fn a_dropped_reply_tells_the_worker_to_skip_the_work() {
        let pool = ReplyPool::<u32>::default();
        let (sender, reply) = pool.channel();
        assert!(sender.is_closed().not());

        drop(reply);

        assert!(sender.is_closed());
    }

    #[test]
    fn a_recycled_slot_starts_open() {
        let pool = ReplyPool::default();
        let (sender, reply) = pool.channel();
        sender.send(1_u32);
        crate::rt::poll_to_end(reply).0.unwrap();

        let (sender, _reply) = pool.channel();

        assert!(sender.is_closed().not());
    }

    #[test]
    #[cfg(feature = "gated-tests")]
    fn a_warm_round_trip_allocates_nothing() {
        let pool = ReplyPool::default();
        let (sender, reply) = pool.channel();
        sender.send(0_u32);
        crate::rt::poll_to_end(reply).0.unwrap();

        let (value, allocations) = crate::alloc_count::count_allocations(|| {
            let (sender, reply) = pool.channel();
            sender.send(7);
            crate::rt::poll_to_end(reply).0
        });

        assert_eq!(value, Ok(7));
        assert_eq!(allocations, 0);
    }
}
