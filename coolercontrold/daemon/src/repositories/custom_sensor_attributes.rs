// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The driver limits a Custom Sensor carries.
//!
//! A sensor with a single source (Scale & Offset, Time Average, Exponential Moving Average)
//! reports its source's limits as its own, in its own unit: a pressure scaled from microbar
//! to millibar shows its minimum and maximum in millibar. Mix and File sensors have no
//! single source to take limits from.

use crate::device::{ChannelAttribute, ChannelAttributeKind, DeviceUID, MAX_CHANNEL_ATTRIBUTES};
use crate::setting::{CustomSensor, CustomSensorKind, CustomSensorMetric, SensorSource};

/// A sensor may read another Custom Sensor, which reads a hardware source: two steps.
const CHAIN_STEPS_MAX: usize = 2;

/// Where a sensor's limits come from, and how its values follow that source's:
/// `sensor = source * scale + offset`.
#[derive(Debug, Clone, PartialEq)]
pub struct AttributeForward {
    pub source_device_uid: DeviceUID,
    /// The temp or channel on that device whose attributes are read.
    pub source_name: String,
    pub metric: CustomSensorMetric,
    pub scale: f64,
    pub offset: f64,
}

/// How one attribute follows the sensor's values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Forwarding {
    /// A value in the sensor's unit: scaled and offset like a reading.
    Absolute,
    /// A width in the sensor's unit: scaled by magnitude, never offset.
    Margin,
    /// Describes the source hardware, or is a value of another metric of the channel.
    Dropped,
}

/// Follows `sensor_id` down to the hardware source whose limits it carries. `None` for a
/// sensor without a single source, an unknown sensor, or a chain deeper than the hierarchy
/// allows.
pub fn attribute_forward(
    sensors: &[CustomSensor],
    custom_sensors_device_uid: &str,
    sensor_id: &str,
) -> Option<AttributeForward> {
    let metric = find_sensor(sensors, sensor_id)?.metric;
    let mut scale = 1.;
    let mut offset = 0.;
    let mut current_id = sensor_id;
    for _ in 0..CHAIN_STEPS_MAX {
        let sensor = find_sensor(sensors, current_id)?;
        let source = match &sensor.kind {
            CustomSensorKind::Offset {
                scale: step_scale,
                offset: step_offset,
                sources,
            } => {
                // outer = (inner * step_scale + step_offset) * scale + offset
                offset += scale * step_offset;
                scale *= step_scale.get();
                single_source(sources)?
            }
            CustomSensorKind::TimeAverage { sources, .. }
            | CustomSensorKind::ExponentialMovingAvg { sources, .. } => single_source(sources)?,
            CustomSensorKind::Mix { .. } | CustomSensorKind::File { .. } => return None,
        };
        if source.device_uid != custom_sensors_device_uid {
            debug_assert!(scale.is_finite());
            debug_assert!(offset.is_finite());
            return Some(AttributeForward {
                source_device_uid: source.device_uid.clone(),
                source_name: source.name.clone(),
                metric,
                scale,
                offset,
            });
        }
        current_id = &source.name;
    }
    None
}

/// The source's attributes as the sensor's limits: only values in the sensor's unit, each
/// passed through the sensor's scale and offset. Names and kinds are the source's, so a
/// negative scale leaves a `_min` above its `_max`.
pub fn forward_attributes(
    forward: &AttributeForward,
    attributes: Vec<ChannelAttribute>,
) -> Vec<ChannelAttribute> {
    debug_assert!(attributes.len() <= MAX_CHANNEL_ATTRIBUTES);
    let mut forwarded = Vec::with_capacity(attributes.len());
    for attribute in attributes {
        let value = match forwarding(forward.metric, attribute.kind) {
            Forwarding::Absolute => attribute.value * forward.scale + forward.offset,
            Forwarding::Margin => attribute.value * forward.scale.abs(),
            Forwarding::Dropped => continue,
        };
        if value.is_finite() {
            forwarded.push(ChannelAttribute { value, ..attribute });
        }
    }
    forwarded
}

fn find_sensor<'a>(sensors: &'a [CustomSensor], sensor_id: &str) -> Option<&'a CustomSensor> {
    sensors.iter().find(|sensor| sensor.id == sensor_id)
}

fn single_source(sources: &[SensorSource]) -> Option<&SensorSource> {
    match sources {
        [source] => Some(source),
        _ => None,
    }
}

/// An attribute follows the sensor only when it is a value of the sensor's own metric. A fan
/// channel carries rpm and duty under one name, so a duty sensor on it takes none of the
/// fan's rpm limits.
fn forwarding(metric: CustomSensorMetric, kind: ChannelAttributeKind) -> Forwarding {
    use ChannelAttributeKind as K;
    let (attribute_metric, forwarding) = match kind {
        K::TempMax
        | K::TempMaxHyst
        | K::TempCrit
        | K::TempCritHyst
        | K::TempEmergency
        | K::TempEmergencyHyst
        | K::TempMin
        | K::TempMinHyst
        | K::TempLcrit
        | K::TempLcritHyst
        | K::TempLowest
        | K::TempHighest
        | K::TempRatedMin
        | K::TempRatedMax => (CustomSensorMetric::Temp, Forwarding::Absolute),
        K::FanMin | K::FanMax | K::FanTarget => (CustomSensorMetric::RPM, Forwarding::Absolute),
        K::PowerMax
        | K::PowerCrit
        | K::PowerMin
        | K::PowerLcrit
        | K::PowerCap
        | K::PowerCapMax
        | K::PowerCapMin
        | K::PowerRatedMin
        | K::PowerRatedMax => (CustomSensorMetric::Watts, Forwarding::Absolute),
        K::PowerCapHyst => (CustomSensorMetric::Watts, Forwarding::Margin),
        K::TempOffset | K::TempType | K::FanDiv | K::FanPulses => return Forwarding::Dropped,
    };
    if attribute_metric == metric {
        forwarding
    } else {
        Forwarding::Dropped
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::setting::{CustomSensorMixFunctionType, Scale};
    use std::ops::Not;
    use std::path::PathBuf;

    const CS_UID: &str = "custom-sensors-uid";
    const HW_UID: &str = "hardware-uid";

    fn source(device_uid: &str, name: &str) -> SensorSource {
        SensorSource {
            device_uid: device_uid.to_string(),
            name: name.to_string(),
            weight: 1,
        }
    }

    fn sensor(id: &str, metric: CustomSensorMetric, kind: CustomSensorKind) -> CustomSensor {
        CustomSensor {
            id: id.to_string(),
            metric,
            kind,
            children: Vec::new(),
            parents: Vec::new(),
        }
    }

    fn scaled(
        id: &str,
        metric: CustomSensorMetric,
        scale: f64,
        offset: f64,
        from: SensorSource,
    ) -> CustomSensor {
        sensor(
            id,
            metric,
            CustomSensorKind::Offset {
                scale: Scale::try_from(scale).unwrap(),
                offset,
                sources: vec![from],
            },
        )
    }

    fn averaged(id: &str, metric: CustomSensorMetric, from: SensorSource) -> CustomSensor {
        sensor(
            id,
            metric,
            CustomSensorKind::TimeAverage {
                time_window_seconds: 10,
                sources: vec![from],
            },
        )
    }

    fn attribute(name: &str, kind: ChannelAttributeKind, value: f64) -> ChannelAttribute {
        ChannelAttribute {
            name: name.to_string(),
            kind,
            value,
        }
    }

    fn assert_values(attributes: &[ChannelAttribute], expected: &[(&str, f64)]) {
        assert_eq!(attributes.len(), expected.len(), "{attributes:?}");
        for (attribute, (name, value)) in attributes.iter().zip(expected) {
            assert_eq!(attribute.name, *name);
            assert!(
                (attribute.value - value).abs() < 1e-9,
                "{name}: {} is not {value}",
                attribute.value
            );
        }
    }

    // The ticket's example: a pressure in microbar on a fan input, scaled by 0.001, carries
    // the driver's limits in millibar under their own names. The pulse count describes the
    // fan input and is not a pressure, so it is dropped.
    #[test]
    fn scaled_pressure_carries_its_limits_in_the_new_unit() {
        let sensors = vec![scaled(
            "pressure",
            CustomSensorMetric::RPM,
            0.001,
            0.,
            source(HW_UID, "fan1"),
        )];
        let forward = attribute_forward(&sensors, CS_UID, "pressure").unwrap();
        assert_eq!(forward.source_device_uid, HW_UID);
        assert_eq!(forward.source_name, "fan1");

        let limits = forward_attributes(
            &forward,
            vec![
                attribute("fan1_min", ChannelAttributeKind::FanMin, 382_800.),
                attribute("fan1_max", ChannelAttributeKind::FanMax, 472_800.),
                attribute("fan1_pulses", ChannelAttributeKind::FanPulses, 2.),
            ],
        );

        assert_values(&limits, &[("fan1_min", 382.8), ("fan1_max", 472.8)]);
        assert_eq!(limits[0].kind, ChannelAttributeKind::FanMin);
    }

    // A smoothing sensor reports its source's values, so its limits pass through unchanged.
    #[test]
    fn smoothing_sensors_carry_limits_unchanged() {
        let ema = sensor(
            "ema",
            CustomSensorMetric::Temp,
            CustomSensorKind::ExponentialMovingAvg {
                time_window_seconds: 10,
                sources: vec![source(HW_UID, "temp1")],
            },
        );
        let sensors = vec![
            averaged("avg", CustomSensorMetric::Temp, source(HW_UID, "temp1")),
            ema,
        ];
        for id in ["avg", "ema"] {
            let forward = attribute_forward(&sensors, CS_UID, id).unwrap();
            let limits = forward_attributes(
                &forward,
                vec![
                    attribute("temp1_max", ChannelAttributeKind::TempMax, 95.),
                    attribute("temp1_crit_hyst", ChannelAttributeKind::TempCritHyst, 100.),
                ],
            );
            assert_values(&limits, &[("temp1_max", 95.), ("temp1_crit_hyst", 100.)]);
        }
    }

    // A parent follows its child to the hardware source, and the steps compose: the child
    // halves and adds 10, the parent doubles and adds 1, so the parent is source + 21.
    #[test]
    fn a_chain_composes_its_steps() {
        let sensors = vec![
            scaled(
                "child",
                CustomSensorMetric::Temp,
                0.5,
                10.,
                source(HW_UID, "temp1"),
            ),
            scaled(
                "parent",
                CustomSensorMetric::Temp,
                2.,
                1.,
                source(CS_UID, "child"),
            ),
            averaged("smooth", CustomSensorMetric::Temp, source(CS_UID, "child")),
        ];

        let parent = attribute_forward(&sensors, CS_UID, "parent").unwrap();
        assert_eq!(parent.source_name, "temp1");
        assert_eq!((parent.scale, parent.offset), (1., 21.));

        let smooth = attribute_forward(&sensors, CS_UID, "smooth").unwrap();
        assert_eq!(smooth.source_device_uid, HW_UID);
        assert_eq!((smooth.scale, smooth.offset), (0.5, 10.));
    }

    // Mix and File sensors have no single source, an unknown id is no sensor, and a chain
    // deeper than the hierarchy allows is not followed.
    #[test]
    fn sensors_without_a_single_source_carry_no_limits() {
        let mix = sensor(
            "mix",
            CustomSensorMetric::Temp,
            CustomSensorKind::Mix {
                mix_function: CustomSensorMixFunctionType::Max,
                sources: vec![source(HW_UID, "temp1")],
            },
        );
        let file = sensor(
            "file",
            CustomSensorMetric::Temp,
            CustomSensorKind::File {
                file_path: PathBuf::from("/tmp/x"),
            },
        );
        let sensors = vec![
            mix,
            file,
            averaged("of_mix", CustomSensorMetric::Temp, source(CS_UID, "mix")),
            averaged("a", CustomSensorMetric::Temp, source(CS_UID, "b")),
            averaged("b", CustomSensorMetric::Temp, source(CS_UID, "c")),
            averaged("c", CustomSensorMetric::Temp, source(HW_UID, "temp1")),
        ];
        for id in ["mix", "file", "of_mix", "a", "unknown"] {
            assert!(attribute_forward(&sensors, CS_UID, id).is_none(), "{id}");
        }
        assert!(attribute_forward(&sensors, CS_UID, "b").is_some());
    }

    // Only attributes of the sensor's own metric follow it: a duty sensor on a fan channel
    // takes none of the fan's rpm limits, and a temperature sensor none of its source's
    // descriptive attributes.
    #[test]
    fn only_limits_of_the_sensor_metric_are_forwarded() {
        let fan_attributes = || {
            vec![
                attribute("fan1_min", ChannelAttributeKind::FanMin, 300.),
                attribute("fan1_target", ChannelAttributeKind::FanTarget, 900.),
                attribute("fan1_div", ChannelAttributeKind::FanDiv, 2.),
            ]
        };
        let forward_as = |metric| AttributeForward {
            source_device_uid: HW_UID.to_string(),
            source_name: "fan1".to_string(),
            metric,
            scale: 1.,
            offset: 0.,
        };

        let as_rpm = forward_attributes(&forward_as(CustomSensorMetric::RPM), fan_attributes());
        assert_values(&as_rpm, &[("fan1_min", 300.), ("fan1_target", 900.)]);
        for metric in [
            CustomSensorMetric::Duty,
            CustomSensorMetric::Freq,
            CustomSensorMetric::Temp,
            CustomSensorMetric::Watts,
        ] {
            assert!(forward_attributes(&forward_as(metric), fan_attributes()).is_empty());
        }

        let temp_attributes = vec![
            attribute("temp1_min", ChannelAttributeKind::TempMin, 5.),
            attribute("temp1_offset", ChannelAttributeKind::TempOffset, 3.),
            attribute("temp1_type", ChannelAttributeKind::TempType, 4.),
        ];
        let as_temp = forward_attributes(&forward_as(CustomSensorMetric::Temp), temp_attributes);
        assert_values(&as_temp, &[("temp1_min", 5.)]);
    }

    // A negative scale inverts the limits and keeps their names. A hysteresis margin is a
    // width, so it scales by magnitude and takes no offset.
    #[test]
    fn a_negative_scale_inverts_limits_and_keeps_margins_positive() {
        let forward = AttributeForward {
            source_device_uid: HW_UID.to_string(),
            source_name: "power1".to_string(),
            metric: CustomSensorMetric::Watts,
            scale: -2.,
            offset: 500.,
        };

        let limits = forward_attributes(
            &forward,
            vec![
                attribute("power1_min", ChannelAttributeKind::PowerMin, 10.),
                attribute("power1_max", ChannelAttributeKind::PowerMax, 200.),
                attribute("power1_cap_hyst", ChannelAttributeKind::PowerCapHyst, 5.),
            ],
        );

        assert_values(
            &limits,
            &[
                ("power1_min", 480.),
                ("power1_max", 100.),
                ("power1_cap_hyst", 10.),
            ],
        );
        assert!(limits.iter().all(|limit| limit.value.is_finite()));
        assert!(limits.is_empty().not());
    }
}
