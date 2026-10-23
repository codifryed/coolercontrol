// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Refilling a device's per-tick status lists in place.

use super::{ChannelSample, ChannelStatus, Temp, TempStatus};

/// Refills a device's status lists in place, leaving them as a rebuild from this tick's reports
/// would, while keeping each entry's name allocation.
///
/// A source reports its names in the same order every tick, so a normal tick writes no name. When
/// it is dropped, whatever this tick did not report is cut off.
pub struct StatusRefill<'a> {
    channels: &'a mut Vec<ChannelStatus>,
    temps: &'a mut Vec<TempStatus>,
    channel_count: usize,
    temp_count: usize,
}

impl<'a> StatusRefill<'a> {
    #[must_use = "the lists are only cut to this tick's reports when the refill is dropped"]
    pub fn new(channels: &'a mut Vec<ChannelStatus>, temps: &'a mut Vec<TempStatus>) -> Self {
        Self {
            channels,
            temps,
            channel_count: 0,
            temp_count: 0,
        }
    }

    pub fn temp(&mut self, name: &str, temp: Temp) {
        debug_assert!(self.temp_count <= self.temps.len());
        if let Some(entry) = self.temps.get_mut(self.temp_count) {
            rename(&mut entry.name, name);
            entry.temp = temp;
        } else {
            self.temps.push(TempStatus {
                name: name.to_owned(),
                temp,
            });
        }
        self.temp_count += 1;
        debug_assert_eq!(self.temps[self.temp_count - 1].name, name);
    }

    pub fn channel(&mut self, name: &str, sample: ChannelSample) {
        debug_assert!(self.channel_count <= self.channels.len());
        if let Some(entry) = self.channels.get_mut(self.channel_count) {
            rename(&mut entry.name, name);
            sample.apply_to(entry);
        } else {
            self.channels.push(sample.to_status(name));
        }
        self.channel_count += 1;
        debug_assert_eq!(self.channels[self.channel_count - 1].name, name);
    }

    /// This tick's channels, for a caller that must change them before they are kept.
    pub fn channels_mut(&mut self) -> &mut [ChannelStatus] {
        &mut self.channels[..self.channel_count]
    }
}

impl Drop for StatusRefill<'_> {
    fn drop(&mut self) {
        self.channels.truncate(self.channel_count);
        self.temps.truncate(self.temp_count);
        debug_assert_eq!(self.channels.len(), self.channel_count);
        debug_assert_eq!(self.temps.len(), self.temp_count);
    }
}

/// Writes `name` over `current`, in its allocation when it fits.
fn rename(current: &mut String, name: &str) {
    if current != name {
        current.clear();
        current.push_str(name);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rpm(rpm: u32) -> ChannelSample {
        ChannelSample {
            rpm: Some(rpm),
            ..Default::default()
        }
    }

    fn refill_with(
        channels: &mut Vec<ChannelStatus>,
        temps: &mut Vec<TempStatus>,
        channel_reports: &[(&str, u32)],
        temp_reports: &[(&str, Temp)],
    ) {
        let mut refill = StatusRefill::new(channels, temps);
        for &(name, value) in channel_reports {
            refill.channel(name, rpm(value));
        }
        for &(name, temp) in temp_reports {
            refill.temp(name, temp);
        }
    }

    fn rebuilt(
        channel_reports: &[(&str, u32)],
        temp_reports: &[(&str, Temp)],
    ) -> (Vec<ChannelStatus>, Vec<TempStatus>) {
        let channels = channel_reports
            .iter()
            .map(|&(name, value)| rpm(value).to_status(name))
            .collect();
        let temps = temp_reports
            .iter()
            .map(|&(name, temp)| TempStatus {
                name: name.to_owned(),
                temp,
            })
            .collect();
        (channels, temps)
    }

    #[test]
    fn a_refill_leaves_what_a_rebuild_would() {
        // Goal: whatever was there before, the lists end as a rebuild from this tick's reports.
        // Method: refill one pair through a full tick, a tick missing a middle channel and a temp,
        // a tick with a new channel, and an empty tick, comparing each against a rebuild.
        let ticks: [(&[(&str, u32)], &[(&str, Temp)]); 4] = [
            (
                &[("fan1", 900), ("fan2", 1200), ("fan3", 700)],
                &[("temp1", 40.0)],
            ),
            (&[("fan1", 910), ("fan3", 710)], &[]),
            (
                &[("fan1", 920), ("fan2", 1220), ("fan3", 720), ("pump", 2000)],
                &[("temp1", 41.0), ("temp2", 30.0)],
            ),
            (&[], &[]),
        ];
        let mut channels = Vec::new();
        let mut temps = Vec::new();
        for (channel_reports, temp_reports) in ticks {
            refill_with(&mut channels, &mut temps, channel_reports, temp_reports);
            assert_eq!(
                (channels.clone(), temps.clone()),
                rebuilt(channel_reports, temp_reports)
            );
        }
    }

    #[test]
    fn a_refill_overwrites_every_value_of_a_kept_channel() {
        // Goal: a kept entry carries nothing over from the last tick, so a field absent this tick
        // is absent, not stale. Method: a channel with every field, then one with rpm alone.
        let mut channels = vec![ChannelStatus {
            name: "fan1".to_owned(),
            rpm: Some(900),
            duty: Some(50.0),
            freq: Some(1),
            watts: Some(2.0),
            pwm_mode: Some(1),
        }];
        let mut temps = Vec::new();

        refill_with(&mut channels, &mut temps, &[("fan1", 950)], &[]);

        assert_eq!(channels, vec![rpm(950).to_status("fan1")]);
    }

    #[test]
    fn channels_mut_covers_only_this_tick() {
        // Goal: a caller adjusting this tick's channels cannot reach the ones about to be cut.
        // Method: refill two of three kept channels and look at the slice.
        let mut channels = vec![
            rpm(1).to_status("fan1"),
            rpm(2).to_status("fan2"),
            rpm(3).to_status("fan3"),
        ];
        let mut temps = Vec::new();
        let mut refill = StatusRefill::new(&mut channels, &mut temps);
        refill.channel("fan1", rpm(10));
        refill.channel("fan2", rpm(20));

        assert_eq!(refill.channels_mut().len(), 2);
        drop(refill);
        assert_eq!(channels.len(), 2);
    }

    #[cfg(feature = "gated-tests")]
    #[test]
    fn a_refill_with_unchanged_names_allocates_nothing() {
        // Goal: a tick reporting the same names in the same order as the last allocates nothing.
        // Method: refill once to warm the lists, then count the allocations of a second refill.
        let channel_reports = [("fan1", 900), ("fan2", 1200)];
        let temp_reports = [("temp1", 40.0)];
        let mut channels = Vec::new();
        let mut temps = Vec::new();
        refill_with(&mut channels, &mut temps, &channel_reports, &temp_reports);

        let ((), allocations) = crate::alloc_count::count_allocations(|| {
            refill_with(&mut channels, &mut temps, &channel_reports, &temp_reports);
        });

        assert_eq!(allocations, 0);
        assert_eq!((channels, temps), rebuilt(&channel_reports, &temp_reports));
    }
}
