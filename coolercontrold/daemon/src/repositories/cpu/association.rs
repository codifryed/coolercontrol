// SPDX-FileCopyrightText: 2022 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::ops::Not;

use crate::repositories::cpu::topology::{CpuTopology, PhysicalID};
use crate::repositories::cpu::INTEL_DEVICE_NAME;

/// A driver's own package zone or node id. Numbered by the driver, not by cpuinfo, so it is only
/// a physical id on the hardware where the two happen to coincide:
pub type ZoneID = u8;

/// Which vendor's temperature driver a CPU device belongs to. The two number their devices by
/// different quantities, so the driver decides both where the id is read from and how it ranks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CpuDriver {
    /// `coretemp`, one platform device per die zone.
    Intel,
    /// `k10temp` and `zenpower`, one PCI device per node.
    Amd,
}

impl CpuDriver {
    pub fn of(device_name: &str) -> Self {
        if device_name == INTEL_DEVICE_NAME {
            Self::Intel
        } else {
            Self::Amd
        }
    }
}

/// How confidently a CPU hwmon device is tied to a physical processor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CpuAssociation {
    /// Tied to a cpuinfo physical id. Load, frequency and power all describe this processor.
    Socket(PhysicalID),
    /// Keyed by the driver's own zone id instead: nothing says which processor the temps came
    /// from, or they are a later die of a processor whose first die already holds its `Socket`.
    /// The temps are real, but the signals keyed by physical id are left off rather than attached
    /// to a guess or shown twice.
    Zone(ZoneID),
}

impl CpuAssociation {
    /// The cpuinfo physical id this device measures, or `None` when nothing ties it to one.
    /// Everything keyed by physical id, i.e. load, frequency and package power, goes through here.
    pub fn physical_id(self) -> Option<PhysicalID> {
        match self {
            Self::Socket(physical_id) => Some(physical_id),
            Self::Zone(_) => None,
        }
    }

    pub fn is_socket(self) -> bool {
        matches!(self, Self::Socket(_))
    }
}

/// What the association needs to know about one driver's devices as a whole.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DriverCensus {
    /// The hwmon devices the driver registered.
    pub device_count: usize,
    /// Whether every zone the driver created has an online CPU. Only `coretemp` zones follow CPU
    /// hotplug: a `k10temp` node is a PCI device and stays registered either way.
    pub all_zones_online: bool,
    /// Whether an online CPU sits on a later die of its package. Only read when a zone is offline.
    pub multi_die: bool,
}

/// Ties a CPU hwmon device to the processor it measures.
///
/// The zone id and the census are read from sysfs by the caller and passed in, so every rule here
/// is a pure function of what cpuinfo and the driver said. The census describes the driver's
/// devices as a whole, which is what says whether it reports per package or per die.
pub fn associate(
    topology: &CpuTopology,
    driver: CpuDriver,
    zone_id: Option<ZoneID>,
    census: DriverCensus,
) -> Option<CpuAssociation> {
    match driver {
        CpuDriver::Intel => intel_association(topology, zone_id, census),
        CpuDriver::Amd => amd_association(topology, zone_id, census.device_count),
    }
}

/// The 1-based number a CPU device is presented under, which its UID is derived from.
///
/// A socket keeps `physical id + 1` so that existing device UIDs, and the settings saved
/// against them, do not change. A zone is numbered past every physical id: a zone id and a
/// physical id are different quantities, so the two would otherwise collide wherever cpuinfo
/// numbers its packages sparsely.
///
/// This is the only place a device number and a processor id are converted into each other.
/// Nothing recovers a physical id back out of a `type_index`.
pub fn device_type_index(topology: &CpuTopology, association: CpuAssociation) -> Option<u8> {
    match association {
        CpuAssociation::Socket(physical_id) => {
            debug_assert!(topology.contains(physical_id));
            physical_id.checked_add(1)
        }
        CpuAssociation::Zone(zone_id) => {
            let highest_physical_id = topology.packages().next_back()?;
            // One past the highest socket number, then the zone's own offset.
            let type_index = highest_physical_id.checked_add(2)?.checked_add(zone_id)?;
            debug_assert!(type_index > highest_physical_id + 1);
            Some(type_index)
        }
    }
}

/// The physical processors that no CPU hwmon device was matched to, ascending.
pub fn unmatched_physical_ids(
    topology: &CpuTopology,
    matched: impl Iterator<Item = CpuAssociation>,
) -> Vec<PhysicalID> {
    let matched_sockets = matched
        .filter_map(CpuAssociation::physical_id)
        .collect::<Vec<PhysicalID>>();
    // `packages()` is already ascending, so the result needs no sort of its own.
    let unmatched = topology
        .packages()
        .filter(|physical_id| matched_sockets.contains(physical_id).not())
        .collect::<Vec<PhysicalID>>();
    debug_assert!(unmatched.len() <= topology.package_count());
    unmatched
}

/// Whether every `coretemp` zone has an online CPU, so that the APIC ranking covers them all.
///
/// cpuinfo lists only online CPUs, while zone ids count every package. An offline package
/// below an online one therefore shifts the ranking by one. The platform devices outlive
/// offlining on current kernels. Where they do not, a gap in the zone ids still exposes an
/// offline zone below an online one, which is the case that shifts the ranking.
pub fn zones_all_online(zone_ids: &[ZoneID], platform_zone_count: usize) -> bool {
    let zone_total = platform_zone_count.max(zone_ids.len());
    let mut sorted_ids = zone_ids.to_vec();
    sorted_ids.sort_unstable();
    sorted_ids.dedup();
    if sorted_ids.len() != zone_total {
        return false;
    }
    // Every id below the total, with none repeated, means exactly 0 through total - 1.
    sorted_ids
        .last()
        .is_none_or(|id| usize::from(*id) < zone_total)
}

/// Intel registers one `coretemp` platform device per die zone, and the kernel numbers those
/// zones by ascending APIC id, so the device's instance id divided by the dies per package is
/// its package's rank in the APIC order.
///
/// The temp labels cannot answer this, which is why the `Package id N` label that this used
/// to parse is gone. That label was the wrong quantity: `coretemp_device_add()` sets
/// `pdata->pkg_id = zoneid`, so it prints the zone number and not cpuinfo's physical id. The
/// two coincide on most hardware, which is the only reason parsing it worked. It is also
/// absent entirely on packages without the PTS feature (pre-Sandy Bridge), and the remaining
/// `Core N` labels carry core ids that repeat identically across sockets. The zone ranking
/// gives the same answer wherever the label worked, and an answer where it did not.
fn intel_association(
    topology: &CpuTopology,
    zone_id: Option<ZoneID>,
    census: DriverCensus,
) -> Option<CpuAssociation> {
    // A single package needs no ranking, and its physical id is not always 0.
    if let Some(only_package) = topology.only_package() {
        return Some(CpuAssociation::Socket(only_package));
    }
    let zone_id = zone_id?;
    if census.all_zones_online.not() {
        return Some(offline_zone_association(
            topology,
            zone_id,
            census.multi_die,
        ));
    }
    // Logical die ids rank system wide and a package's dies are consecutive, since die bits
    // sit below package bits in the APIC id. This division is exact, not an assumption.
    let Some(package_rank) =
        first_die_package_rank(zone_id, census.device_count, topology.package_count())
    else {
        return Some(CpuAssociation::Zone(zone_id));
    };
    if let Some(physical_id) = topology.apic_order().get(usize::from(package_rank)) {
        if topology.contains(*physical_id) {
            return Some(CpuAssociation::Socket(*physical_id));
        }
    }
    // A zone past the ranking cannot be placed. The temps are still real, so keep them.
    Some(CpuAssociation::Zone(zone_id))
}

/// With a zone offline, the APIC ranking of the online packages no longer matches the zone
/// ids, so this falls back to what the `Package id N` label gave before: the zone id is the
/// physical id. That holds wherever physical ids are dense, and it keeps device UIDs exactly
/// as they were for as long as the zone stays offline. Several dies per package break it, as
/// zone 1 is then the first package's second die, so those keep their temps under the zone.
fn offline_zone_association(
    topology: &CpuTopology,
    zone_id: ZoneID,
    multi_die: bool,
) -> CpuAssociation {
    if multi_die {
        return CpuAssociation::Zone(zone_id);
    }
    if topology.contains(zone_id) {
        return CpuAssociation::Socket(zone_id);
    }
    CpuAssociation::Zone(zone_id)
}

/// AMD registers one `k10temp` device per node against that node's northbridge or data
/// fabric PCI function, which sits at a slot fixed by the hardware.
///
/// The node id read from that slot replaces the hwmon enumeration index this used to assume.
/// Hwmon paths are sorted as strings, so `hwmon10` precedes `hwmon2` and that index inverts
/// as soon as a machine has ten or more hwmon devices.
///
// NOTE: the node cpulist was the other way to do this, and is not used due to an apparent
// bug in the amd hwmon kernel driver. Kept as a reference to the alternative:
// let cpu_list: Vec<ProcessorID> = devices::get_processor_ids_from_node_cpulist(index).await?;
// for (physical_id, processor_list) in &topology {
//     if cpu_list.iter().eq(processor_list.iter()) {
//         return Ok(physical_id.clone());
//     }
// }
fn amd_association(
    topology: &CpuTopology,
    node_id: Option<ZoneID>,
    node_count: usize,
) -> Option<CpuAssociation> {
    // A single node needs no id at all, and its physical id is not always 0 (AMD APU).
    if let Some(only_package) = topology.only_package() {
        return Some(CpuAssociation::Socket(only_package));
    }
    let node_id = node_id?;
    // Pre-Zen multi-chip parts and Zen 1 EPYC hold several nodes per package. This assumes
    // their nodes are numbered package by package, as on Zen 1 where the node id is
    // {socket, die}. That is not confirmed from kernel source for every family.
    // Known gap: nodes are PCI devices and stay registered while a package is offline, but
    // cpuinfo drops that package. On a four socket Opteron with a package offline, the node
    // count then no longer divides by the package count, and nothing tells an offline package
    // from a multi-node one. Two sockets need no division, since one online package takes
    // the single package path above.
    let Some(package_rank) = first_die_package_rank(node_id, node_count, topology.package_count())
    else {
        return Some(CpuAssociation::Zone(node_id));
    };
    if topology.contains(package_rank) {
        return Some(CpuAssociation::Socket(package_rank));
    }
    // A node cpuinfo has no physical id for cannot be placed. The temps are still real.
    Some(CpuAssociation::Zone(node_id))
}

/// The rank of the package a driver device is the first die of.
///
/// The drivers register one device per die and number a package's dies consecutively, so
/// each package holds `device_count / package_count` devices. Only a package's first die is
/// tied to it: its later dies would repeat the same load, frequency and power. Returns `None`
/// for a later die, and for an uneven split, which means a die is missing and the numbering
/// cannot be trusted.
fn first_die_package_rank(
    device_id: ZoneID,
    device_count: usize,
    package_count: usize,
) -> Option<u8> {
    if package_count == 0 {
        return None;
    }
    // One device per package, or fewer where some failed to register: each id is a rank.
    if device_count <= package_count {
        return Some(device_id);
    }
    if device_count.is_multiple_of(package_count).not() {
        return None;
    }
    let dies_per_package = device_count / package_count;
    debug_assert!(dies_per_package > 1);
    let device_index = usize::from(device_id);
    if device_index.is_multiple_of(dies_per_package).not() {
        return None;
    }
    let package_rank = u8::try_from(device_index / dies_per_package).ok()?;
    debug_assert!(package_rank < device_id || device_id == 0);
    Some(package_rank)
}

#[cfg(test)]
mod tests {
    use super::{
        amd_association, device_type_index, first_die_package_rank, intel_association,
        unmatched_physical_ids, zones_all_online, CpuAssociation, DriverCensus, PhysicalID,
    };
    use crate::device::{Device, DeviceType};
    use crate::repositories::cpu::fixtures;
    use crate::repositories::cpu::topology::CpuTopology;
    use std::ops::Not;

    /// The topology a given cpuinfo parses to, which is all the association needs of a machine.
    fn topology_from(cpu_info_data: &str) -> CpuTopology {
        CpuTopology::parse(cpu_info_data).unwrap()
    }

    /// A census of `device_count` devices whose zones all have an online CPU, the usual case.
    fn online(device_count: usize) -> DriverCensus {
        DriverCensus {
            device_count,
            all_zones_online: true,
            multi_die: false,
        }
    }

    /// The associations of devices already matched as sockets.
    fn matched(physical_ids: &[PhysicalID]) -> Vec<CpuAssociation> {
        physical_ids
            .iter()
            .copied()
            .map(CpuAssociation::Socket)
            .collect()
    }

    /// Goal: the dual Xeon X5672 in the report has no `Package id` label and identical core ids
    /// on both sockets, so the zone ranking is the only thing that can tell its packages apart.
    /// Method: its real cpuinfo, checking that package 0 ranks before package 1 by APIC id
    /// (0 against 32) and that each coretemp zone resolves to the matching socket.
    #[test]
    fn test_intel_double_cpu_zones_map_to_sockets() {
        // given:
        let topology = topology_from(fixtures::INTEL_DOUBLE_CPU);

        // then: two packages, ordered by their lowest APIC id.
        assert_eq!(topology.package_count(), 2);
        assert_eq!(topology.apic_order(), [0, 1]);

        // then: each zone resolves to its own socket, with load and frequency attached.
        assert_eq!(
            intel_association(&topology, Some(0), online(2)),
            Some(CpuAssociation::Socket(0))
        );
        assert_eq!(
            intel_association(&topology, Some(1), online(2)),
            Some(CpuAssociation::Socket(1))
        );
    }

    /// Goal: the ranking must follow the APIC id, not the physical id, because that is what
    /// `topology_get_logical_id()` counts when the kernel numbers coretemp zones. Method: a
    /// cpuinfo whose physical ids run opposite to its APIC ids, which is the only shape where
    /// the two orderings disagree.
    #[test]
    fn test_zone_ranking_follows_apic_id_not_physical_id() {
        // given:
        let topology = topology_from(fixtures::INVERTED_APIC);

        // then: physical id 1 holds the lower APIC id, so it owns zone 0.
        assert_eq!(topology.apic_order(), [1, 0]);
        assert_eq!(
            intel_association(&topology, Some(0), online(2)),
            Some(CpuAssociation::Socket(1))
        );
        assert_eq!(
            intel_association(&topology, Some(1), online(2)),
            Some(CpuAssociation::Socket(0))
        );
    }

    /// Goal: a zone we cannot tie to a package must keep its temps rather than be dropped or
    /// guessed onto a socket, and a device with no readable zone at all must be dropped. Method:
    /// a two-package cpuinfo queried with a zone past its ranking, and with no zone.
    #[test]
    fn test_unresolvable_zone_falls_back_to_temps_only() {
        // given:
        let topology = topology_from(fixtures::INTEL_DOUBLE_CPU);

        // then: one zone per package, but this one ranks past them. The temps are real, so
        // they are kept under the zone with no socket claim.
        let association = intel_association(&topology, Some(2), online(2));
        assert_eq!(association, Some(CpuAssociation::Zone(2)));
        assert!(association.unwrap().is_socket().not());
        // Nothing keyed by physical id can be asked of it.
        assert_eq!(association.unwrap().physical_id(), None);

        // then: without a zone id there is nothing to key the device on at all.
        assert_eq!(intel_association(&topology, None, online(2)), None);
    }

    /// Goal: a single package must resolve without any zone id, since its own physical id is not
    /// always 0 and older parts expose no package label. Method: a single-socket cpuinfo queried
    /// with no zone and with a nonsense zone.
    #[test]
    fn test_intel_single_cpu_needs_no_zone() {
        // given:
        let topology = topology_from(fixtures::INTEL_SINGLE_CPU);

        // then:
        assert_eq!(topology.package_count(), 1);
        assert_eq!(
            intel_association(&topology, None, online(1)),
            Some(CpuAssociation::Socket(0))
        );
        assert_eq!(
            intel_association(&topology, Some(9), online(1)),
            Some(CpuAssociation::Socket(0))
        );
        // then: every die of a single package belongs to it, so a per-die driver still maps
        // to the one socket.
        assert_eq!(
            intel_association(&topology, Some(1), online(2)),
            Some(CpuAssociation::Socket(0))
        );
    }

    /// Goal: a driver that registers more devices than there are packages is reporting per die,
    /// and a package's dies are numbered consecutively, so with two dies per package zone 1 is
    /// the first package's second die, not the second package. Only each package's first die may
    /// claim its socket. Method: two-package cpuinfo queried for every zone of a four zone Intel
    /// driver (2P Cascade Lake-AP) and every node of an eight node AMD driver (2P Zen 1 EPYC),
    /// then for an uneven split and at the boundary of one device per package.
    #[test]
    fn test_first_die_of_each_package_claims_its_socket() {
        // given: cpuinfo has no die id, so a two-package dump is all these paths can see.
        let intel = topology_from(fixtures::INTEL_DOUBLE_CPU);
        let amd = topology_from(fixtures::AMD_DOUBLE_CPU);

        // then: two dies per package, so zones 0 and 2 are the first dies.
        let intel_expected = [
            CpuAssociation::Socket(0),
            CpuAssociation::Zone(1),
            CpuAssociation::Socket(1),
            CpuAssociation::Zone(3),
        ];
        for (zone_id, expected) in (0..4).zip(intel_expected) {
            assert_eq!(
                intel_association(&intel, Some(zone_id), online(4)),
                Some(expected)
            );
        }
        // then: four nodes per package, so nodes 0 and 4 are the first dies.
        for node_id in 0..8 {
            let expected = match node_id {
                0 => CpuAssociation::Socket(0),
                4 => CpuAssociation::Socket(1),
                _ => CpuAssociation::Zone(node_id),
            };
            assert_eq!(amd_association(&amd, Some(node_id), 8), Some(expected));
        }
        // then: an uneven split means a die is missing, so nothing is tied to a socket.
        for zone_id in 0..3 {
            assert_eq!(
                intel_association(&intel, Some(zone_id), online(3)),
                Some(CpuAssociation::Zone(zone_id))
            );
            assert_eq!(
                amd_association(&amd, Some(zone_id), 3),
                Some(CpuAssociation::Zone(zone_id))
            );
        }
        // then: one device per package ranks directly.
        assert_eq!(
            intel_association(&intel, Some(1), online(2)),
            Some(CpuAssociation::Socket(1))
        );
        assert_eq!(
            amd_association(&amd, Some(1), 2),
            Some(CpuAssociation::Socket(1))
        );
    }

    /// Goal: the die arithmetic on its own, including the inputs the matchers cannot reach.
    /// Method: a table of (device id, device count, package count) against the expected rank.
    #[test]
    fn test_first_die_package_rank() {
        let cases: [(u8, usize, usize, Option<u8>); 12] = [
            // One device per package, or fewer: the id is the rank.
            (0, 2, 2, Some(0)),
            (1, 2, 2, Some(1)),
            (1, 1, 2, Some(1)),
            // Two dies per package: only even ids are first dies.
            (0, 4, 2, Some(0)),
            (1, 4, 2, None),
            (2, 4, 2, Some(1)),
            (3, 4, 2, None),
            // Four nodes per package, as on 2P Zen 1 EPYC.
            (4, 8, 2, Some(1)),
            (7, 8, 2, None),
            // An uneven split cannot be trusted.
            (0, 3, 2, None),
            (2, 5, 2, None),
            // No packages at all.
            (0, 1, 0, None),
        ];
        for (device_id, device_count, package_count, expected) in cases {
            assert_eq!(
                first_die_package_rank(device_id, device_count, package_count),
                expected,
                "device {device_id} of {device_count} over {package_count} packages"
            );
        }
    }

    /// Goal: the APIC ranking is only trusted when every zone has an online CPU, since cpuinfo
    /// lists online packages only. Method: zone ids against the platform device count, covering
    /// an offline zone in the middle, at the end, and a platform count that could not be read.
    #[test]
    fn test_zones_all_online() {
        let cases: [(&[u8], usize, bool); 8] = [
            // Every zone online.
            (&[0, 1], 2, true),
            (&[1, 0], 2, true),
            (&[0, 1, 2, 3], 4, true),
            // Zone 1 offline, below an online one: the ranking would shift.
            (&[0, 2, 3], 4, false),
            // The last zone offline, which only the platform count can show.
            (&[0, 1], 3, false),
            // No platform count, as when it cannot be read: a gap still shows.
            (&[0, 1], 0, true),
            (&[0, 2], 0, false),
            // A repeated id is not a full set.
            (&[0, 0], 2, false),
        ];
        for (zone_ids, platform_zone_count, expected) in cases {
            assert_eq!(
                zones_all_online(zone_ids, platform_zone_count),
                expected,
                "zones {zone_ids:?} of {platform_zone_count}"
            );
        }
    }

    /// Goal: with a package offline, its zone id still counts it while cpuinfo does not, so the
    /// APIC ranking ties zone 2 to physical id 3. The fallback must instead take the zone id as
    /// the physical id, as the `Package id N` label did, and must not guess on multi-die parts.
    /// Method: a four package cpuinfo with package 1 offline, asked for each online zone with
    /// the ranking, then with the fallback, then with the fallback on a multi-die machine.
    #[test]
    fn test_offline_zone_falls_back_to_the_zone_id() {
        // given: physical ids 0, 2 and 3, and zones 0, 2 and 3 with an online CPU.
        let topology = topology_from(fixtures::PACKAGE_1_OFFLINE);
        assert_eq!(topology.apic_order(), [0, 2, 3]);
        let offline = |multi_die: bool| DriverCensus {
            device_count: 3,
            all_zones_online: false,
            multi_die,
        };

        // then: the ranking alone would tie zone 2 to package 3, which is why it is not used.
        assert_eq!(
            intel_association(&topology, Some(2), online(3)),
            Some(CpuAssociation::Socket(3))
        );
        // then: the fallback ties each zone to the physical id of the same number.
        for zone_id in [0, 2, 3] {
            assert_eq!(
                intel_association(&topology, Some(zone_id), offline(false)),
                Some(CpuAssociation::Socket(zone_id))
            );
        }
        // then: a zone with no online package of that id keeps its temps under the zone.
        assert_eq!(
            intel_association(&topology, Some(1), offline(false)),
            Some(CpuAssociation::Zone(1))
        );
        // then: with several dies per package a zone id is not a physical id, so no socket.
        for zone_id in [0, 2, 3] {
            assert_eq!(
                intel_association(&topology, Some(zone_id), offline(true)),
                Some(CpuAssociation::Zone(zone_id))
            );
        }
    }

    /// Goal: on the 5.0 code path a two-package machine always had CPU devices numbered 1 and 2,
    /// including multi-die machines, where zones 0 and 1 matched physical ids 0 and 1. Saved
    /// settings hang off the UIDs built from those numbers, so each processor's socket device
    /// must still produce exactly the same UID. Method: build the UID the way 5.0.0 did, from
    /// the physical id's model name and `physical id + 1`, and compare it with the UID built from
    /// what each first die resolves to, for one and for several dies per package.
    #[test]
    fn test_socket_devices_keep_their_5_0_uids() {
        for (cpu_info_data, is_intel) in [
            (fixtures::INTEL_DOUBLE_CPU, true),
            (fixtures::AMD_DOUBLE_CPU, false),
        ] {
            // given:
            let topology = topology_from(cpu_info_data);
            let uid_5_0 = |physical_id: PhysicalID| {
                let cpu_name = topology.model_name(Some(physical_id)).unwrap();
                Device::create_uid_from(&cpu_name, DeviceType::CPU, physical_id + 1, None)
            };
            let uid_now = |association: CpuAssociation| {
                let type_index = device_type_index(&topology, association).unwrap();
                let cpu_name = topology.model_name(association.physical_id()).unwrap();
                Device::create_uid_from(&cpu_name, DeviceType::CPU, type_index, None)
            };

            // then: (first die of package 1, device count), from one to four dies each.
            for (second_first_die, device_count) in [(1, 2), (2, 4), (4, 8)] {
                let associations = if is_intel {
                    [
                        intel_association(&topology, Some(0), online(device_count)),
                        intel_association(&topology, Some(second_first_die), online(device_count)),
                    ]
                } else {
                    [
                        amd_association(&topology, Some(0), device_count),
                        amd_association(&topology, Some(second_first_die), device_count),
                    ]
                };
                assert_eq!(uid_now(associations[0].unwrap()), uid_5_0(0));
                assert_eq!(uid_now(associations[1].unwrap()), uid_5_0(1));
            }
        }
    }

    /// Goal: a device keyed by a zone rather than a physical id still needs a name to show, and
    /// that name must not vary between runs with `HashMap` order. Method: a two-package cpuinfo
    /// asked for a known id and for an id it has no entry for.
    #[test]
    fn test_model_name_falls_back_for_zone_keyed_devices() {
        // given:
        let topology = topology_from(fixtures::INTEL_DOUBLE_CPU);
        let expected = topology.model_name(Some(0)).unwrap();

        // then: a known id returns its own entry.
        assert_eq!(topology.model_name(Some(0)).as_ref(), Some(&expected));
        assert_eq!(topology.model_name(Some(1)).as_ref(), Some(&expected));
        // then: an unknown id falls back to the lowest known entry, not to nothing.
        assert_eq!(topology.model_name(Some(7)).as_ref(), Some(&expected));
        // then: a zone-keyed device has no id at all, and takes the same fallback.
        assert_eq!(topology.model_name(None).as_ref(), Some(&expected));
    }

    /// Goal: a zone id must never reach a lookup keyed by physical id. `Zone` is produced only for
    /// an id cpuinfo has no entry for, so such a lookup always misses and the device would be
    /// built and then dropped. Method: take the associations both drivers return for an
    /// unresolvable zone on a two-package machine, and assert the zone id cannot be spent as a
    /// physical id while the name resolver still answers.
    #[test]
    fn test_zone_id_is_never_a_physical_id() {
        // given: two packages, so the single-package short circuit does not apply.
        let topology = topology_from(fixtures::INTEL_DOUBLE_CPU);

        // when: a zone beyond the packages cpuinfo knows about.
        let associations = [
            intel_association(&topology, Some(2), online(3)).unwrap(),
            amd_association(&topology, Some(2), 3).unwrap(),
        ];

        // then:
        for association in associations {
            assert!(association.is_socket().not());
            // The zone id is not available as a physical id, so nothing keyed by one can be
            // asked with it.
            assert_eq!(association.physical_id(), None);
            assert_eq!(CpuAssociation::Zone(2), association);
            assert!(topology.contains(2).not());
            // The name resolver must still answer, or the device is built and then skipped.
            assert!(topology.model_name(association.physical_id()).is_some());
        }
    }

    /// Goal: a two-socket AMD system must key its k10temp devices by node id, not by the hwmon
    /// enumeration index the old code assumed. Method: a two-package cpuinfo against the node ids
    /// a dual EPYC reports from PCI slots 0x18 and 0x19.
    #[test]
    fn test_amd_double_cpu_nodes_map_to_sockets() {
        // given:
        let topology = topology_from(fixtures::AMD_DOUBLE_CPU);

        // then:
        assert_eq!(topology.package_count(), 2);
        assert_eq!(
            amd_association(&topology, Some(0), 2),
            Some(CpuAssociation::Socket(0))
        );
        assert_eq!(
            amd_association(&topology, Some(1), 2),
            Some(CpuAssociation::Socket(1))
        );
        // then: a node cpuinfo has no physical id for keeps its temps under the node rather
        // than guessing a socket for them.
        assert_eq!(
            amd_association(&topology, Some(2), 2),
            Some(CpuAssociation::Zone(2))
        );
        // then: no readable node id leaves nothing to key the device on.
        assert_eq!(amd_association(&topology, None, 2), None);
    }

    /// Goal: the single-socket AMD path must be untouched by the node id change, since that is
    /// every AMD desktop and the short circuit runs before any id is consulted. Method: a
    /// single-package cpuinfo with no node id and with a node id that does not exist.
    #[test]
    fn test_amd_single_cpu_ignores_the_node_id() {
        // given:
        let topology = topology_from(fixtures::AMD_SINGLE_CPU);

        // then:
        assert_eq!(topology.package_count(), 1);
        assert_eq!(
            amd_association(&topology, None, 1),
            Some(CpuAssociation::Socket(0))
        );
        assert_eq!(
            amd_association(&topology, Some(3), 1),
            Some(CpuAssociation::Socket(0))
        );
    }

    /// Goal: a partial match must be reported precisely rather than failing the repository, so
    /// the caller can name the processors whose temps could not be placed. Method: a two-socket
    /// cpuinfo against each possible set of matched devices, covering both the fully matched and
    /// the fully unmatched ends.
    #[test]
    fn test_unmatched_physical_ids_double_cpu() {
        // given:
        let topology = topology_from(fixtures::AMD_DOUBLE_CPU);

        let unmatched = |associations: Vec<CpuAssociation>| {
            unmatched_physical_ids(&topology, associations.into_iter())
        };

        // then: nothing matched, so both sockets are reported, ascending.
        assert_eq!(unmatched(matched(&[])), vec![0, 1]);
        // then: a partial match reports only the socket left over.
        assert_eq!(unmatched(matched(&[0])), vec![1]);
        assert_eq!(unmatched(matched(&[1])), vec![0]);
        // then: a full match reports nothing.
        assert!(unmatched(matched(&[0, 1])).is_empty());
        // then: a device we have no cpuinfo entry for must not appear as unmatched.
        assert!(unmatched(matched(&[0, 1, 7])).is_empty());
        // then: a zone-keyed device places no processor, so it must not mark one matched
        // even where its id happens to equal a physical id.
        assert_eq!(unmatched(vec![CpuAssociation::Zone(0)]), vec![0, 1]);
        // then: zones beside a socket still leave the other processor unmatched. The device
        // search stops on an empty result, so this is what lets it register every die.
        let socket_and_zones = vec![
            CpuAssociation::Socket(0),
            CpuAssociation::Zone(1),
            CpuAssociation::Zone(2),
        ];
        assert_eq!(unmatched(socket_and_zones), vec![1]);
    }

    /// Goal: a socket and a zone that share a number are two different devices and must each get
    /// their own device number, since that number is what the device UID is built from. Method: a
    /// two-package cpuinfo, asked to number both kinds across the whole id range.
    #[test]
    fn test_zone_and_socket_device_numbers_cannot_collide() {
        // given: physical ids 0 and 1.
        let topology = topology_from(fixtures::AMD_DOUBLE_CPU);

        // then: a socket keeps its historical number, so saved settings still resolve.
        assert_eq!(
            device_type_index(&topology, CpuAssociation::Socket(0)),
            Some(1)
        );
        assert_eq!(
            device_type_index(&topology, CpuAssociation::Socket(1)),
            Some(2)
        );
        // then: a zone sharing a socket's id gets a different number anyway.
        assert_ne!(
            device_type_index(&topology, CpuAssociation::Zone(0)),
            device_type_index(&topology, CpuAssociation::Socket(0))
        );
        // then: every zone number sits above every socket number, so no device number can be
        // mistaken for another device's, whichever kind either is.
        // Every zone the numbering has room for, given the highest physical id here is 1.
        for zone_id in 0..=(u8::MAX - 3) {
            let type_index = device_type_index(&topology, CpuAssociation::Zone(zone_id)).unwrap();
            assert!(type_index > 2);
            assert!(topology.contains(type_index - 1).not());
        }
        // then: a zone that would number past the range is refused, not wrapped.
        assert_eq!(
            device_type_index(&topology, CpuAssociation::Zone(u8::MAX)),
            None
        );
    }
}
