// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::ops::Not;

use crate::device::ChannelName;

/// The channels of one device that the daemon has driven since it started.
///
/// A reset only writes to these, so a channel left to the firmware or to another tool is not
/// overwritten at boot, on resume, on a Mode change or at shutdown. This lives in memory: a clean
/// shutdown hands everything back, so a new run starts with nothing taken.
#[derive(Debug, Default, Clone)]
pub struct TakenChannels {
    /// Bounded by the device's channels: a name is validated before it is marked.
    channel_names: RefCell<HashSet<ChannelName>>,
    /// Set by a write that covers every channel at once, such as a firmware fan curve.
    whole_device: Cell<bool>,
}

impl TakenChannels {
    /// Marked before the write, since a write that fails part way has still changed the fan.
    pub fn mark(&self, channel_name: &str) {
        debug_assert!(channel_name.is_empty().not());
        let mut channel_names = self.channel_names.borrow_mut();
        // Looked up first, so only a channel's first write builds a key.
        if channel_names.contains(channel_name).not() {
            channel_names.insert(channel_name.to_owned());
        }
    }

    /// For a write that covers every channel, where the name the caller passed says nothing.
    /// Marked before the write, as with `mark`.
    pub fn mark_device(&self) {
        self.whole_device.set(true);
    }

    pub fn is_taken(&self, channel_name: &str) -> bool {
        if self.whole_device.get() {
            return true;
        }
        self.channel_names.borrow().contains(channel_name)
    }

    /// For after a successful reset only: a failed one stays taken, so shutdown tries it again.
    pub fn release(&self, channel_name: &str) {
        self.channel_names.borrow_mut().remove(channel_name);
    }

    /// As `release`, for a reset that covers every channel.
    pub fn release_device(&self) {
        self.channel_names.borrow_mut().clear();
        self.whole_device.set(false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Goal: a channel is taken from its mark until its release, and marking it twice takes one
    /// release. Method: walk one channel through the whole cycle.
    #[test]
    fn a_channel_is_taken_from_mark_to_release() {
        let taken = TakenChannels::default();
        assert!(taken.is_taken("fan1").not());

        taken.mark("fan1");
        taken.mark("fan1");
        assert!(taken.is_taken("fan1"));

        taken.release("fan1");
        assert!(taken.is_taken("fan1").not());
    }

    /// Goal: one channel's marks say nothing about another. Method: mark one of two.
    #[test]
    fn channels_are_tracked_apart() {
        let taken = TakenChannels::default();

        taken.mark("fan1");

        assert!(taken.is_taken("fan2").not());
        taken.release("fan2");
        assert!(taken.is_taken("fan1"));
    }

    /// Goal: a whole device mark covers every channel, and only a whole device release ends it.
    /// Method: mark the device, try to release one channel, then release the device.
    #[test]
    fn a_device_mark_covers_every_channel_until_released() {
        let taken = TakenChannels::default();

        taken.mark_device();
        taken.release("fan1");
        assert!(taken.is_taken("fan1"));
        assert!(taken.is_taken("fan2"));

        taken.release_device();
        assert!(taken.is_taken("fan1").not());
    }

    /// Goal: a whole device release also ends channel marks, since the reset covered them too.
    /// Method: mark a channel, then release the device.
    #[test]
    fn a_device_release_ends_channel_marks() {
        let taken = TakenChannels::default();
        taken.mark("fan1");

        taken.release_device();

        assert!(taken.is_taken("fan1").not());
    }
}
