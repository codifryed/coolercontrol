// SPDX-FileCopyrightText: 2022 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later
use crate::device::Duty;
use crate::engine::commanders::graph::GraphProfileCommander;
use crate::engine::ChannelDuty;
use anyhow::Result;
use moro_local::Scope;
use std::cell::RefCell;

pub mod graph;
pub mod lcd;
pub mod mix;
pub mod overlay;

/// Default tick count before the safety latch forces a re-application.
/// Matches the Graph commander's default of 30 seconds at 1s poll rate.
const DEFAULT_SAFETY_LATCH_COUNT: u8 = 30;

/// Tracks output deduplication state per profile. When the computed duty is
/// unchanged between ticks, the hardware write is suppressed. After
/// `DEFAULT_SAFETY_LATCH_COUNT` consecutive suppressions the write is forced
/// through (safety latch) to re-verify hardware compliance.
pub struct OutputDedupState {
    last_applied_duty: Option<Duty>,
    no_change_counter: u8,
}

impl OutputDedupState {
    pub fn new() -> Self {
        Self {
            last_applied_duty: None,
            // Force immediate application on first use.
            no_change_counter: DEFAULT_SAFETY_LATCH_COUNT,
        }
    }

    /// Returns `true` when the duty should be written to hardware.
    pub fn should_apply(&mut self, duty: Duty) -> bool {
        debug_assert!(duty <= 100, "duty must be in 0..=100 range");
        if self.last_applied_duty == Some(duty) {
            if self.no_change_counter >= DEFAULT_SAFETY_LATCH_COUNT {
                // Safety latch: allow write through, reset counter.
                self.no_change_counter = 0;
                return true;
            }
            self.no_change_counter += 1;
            return false;
        }
        // Duty changed: apply immediately.
        self.last_applied_duty = Some(duty);
        self.no_change_counter = 0;
        true
    }
}

/// A commander's buffer for one tick's duty writes, kept between ticks so a tick that applies
/// allocates none. The writes take it while they run; a tick that starts before they end fills
/// a fresh one, and the larger of the two is kept.
#[derive(Default)]
pub struct DutyBuffer {
    spare: RefCell<Vec<ChannelDuty>>,
}

impl DutyBuffer {
    /// An empty buffer, with the room of the last one given back.
    pub fn take(&self) -> Vec<ChannelDuty> {
        let duties = self.spare.take();
        debug_assert_eq!(duties.len(), 0);
        duties
    }

    /// Keeps `duties`' room for a later tick, dropping its channels.
    pub fn give_back(&self, mut duties: Vec<ChannelDuty>) {
        duties.clear();
        let mut spare = self.spare.borrow_mut();
        if duties.capacity() > spare.capacity() {
            *spare = duties;
        }
        debug_assert_eq!(spare.len(), 0);
    }
}

/// Spawns one task that writes a tick's `duties` and then returns their buffer, so the spawn
/// costs the same whatever the device count. A tick with nothing to write spawns nothing.
pub fn spawn_duty_writes<'s>(
    scope: &'s Scope<'s, 's, Result<()>>,
    graph_commander: &'s GraphProfileCommander,
    buffer: &'s DutyBuffer,
    mut duties: Vec<ChannelDuty>,
) {
    if duties.is_empty() {
        buffer.give_back(duties);
        return;
    }
    debug_assert!(
        duties.iter().all(|(_, duty)| *duty <= 100),
        "duty must be in 0..=100 range"
    );
    scope.spawn(async move {
        graph_commander.apply_duties(&mut duties).await;
        buffer.give_back(duties);
    });
}

/// `duties` by device, as `(channel, duty)` in collection order, for asserting on a collect.
#[cfg(test)]
pub fn by_device(
    duties: &[ChannelDuty],
) -> std::collections::HashMap<crate::device::DeviceUID, Vec<(crate::device::ChannelName, Duty)>> {
    let mut grouped = std::collections::HashMap::<_, Vec<_>>::new();
    for (channel, duty) in duties {
        grouped
            .entry(channel.device_uid().clone())
            .or_default()
            .push((channel.channel_name().clone(), *duty));
    }
    grouped
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_given_back_buffer_keeps_its_room_and_drops_its_channels() {
        // Goal: the next tick reuses the room of the last, and no channel outlives its tick.
        // Method: give back a filled buffer, then take it again.
        let buffer = DutyBuffer::default();
        let channel = std::rc::Rc::new(crate::engine::DeviceChannelProfileSetting::Graph {
            device_uid: "dev1".to_string(),
            channel_name: "fan1".to_string(),
        });
        let mut duties = buffer.take();
        duties.push((std::rc::Rc::clone(&channel), 50));

        buffer.give_back(duties);
        let duties = buffer.take();

        assert!(duties.is_empty());
        assert!(duties.capacity() >= 1);
        assert_eq!(std::rc::Rc::strong_count(&channel), 1);
    }

    #[test]
    fn the_larger_of_two_buffers_is_kept() {
        // Goal: when a slow tick's writes overlap the next tick's, the room kept is the larger.
        // Method: give back a larger buffer, then a smaller one.
        let buffer = DutyBuffer::default();
        buffer.give_back(Vec::with_capacity(8));

        buffer.give_back(Vec::with_capacity(2));

        assert!(buffer.take().capacity() >= 8);
    }
}
