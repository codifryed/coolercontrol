// SPDX-FileCopyrightText: 2023 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use anyhow::{anyhow, bail, Context, Result};
use cc_image::{ImageTemplate, LcdImageGenerator};
use log::{debug, error, trace, warn};
use moro_local::Scope;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::LazyLock;
use std::time::{Duration, Instant};

use crate::api::CCError;
use crate::device::{ChannelName, DeviceUID, Temp, TempLabel, UID};
use crate::engine::main::ReposByType;
use crate::engine::processors;
use crate::paths;
use crate::setting::{LcdModeKind, LcdModeName, LcdSettings};
use crate::AllDevices;

pub const DEFAULT_LCD_SHUTDOWN_IMAGE: &[u8] = cc_image::DEFAULT_LCD_SHUTDOWN_IMAGE;

/// Each channel renders to its own file: concurrent blocking-pool renders for different
/// channels must never interleave create/write on one shared path (torn image), and a
/// channel's device must never read another channel's temp.
fn single_temp_image_filename(device_uid: &UID, channel_name: &str) -> String {
    format!("single_temp_{device_uid}_{channel_name}.png")
}

/// Stateless and `Send + Sync` (the `static` enforces both at compile time), so the
/// blocking closures borrow it instead of moving fonts across threads per generation.
/// First deref parses the embedded fonts, on a blocking thread.
static IMAGE_GENERATOR: LazyLock<LcdImageGenerator> = LazyLock::new(LcdImageGenerator::new);

/// This enables regularly updated LCD screen changes
pub struct LcdCommander {
    all_devices: AllDevices,
    repos: ReposByType,
    pub scheduled_settings: RefCell<HashMap<UID, HashMap<String, LcdSettings>>>,
    scheduled_settings_metadata: RefCell<HashMap<UID, HashMap<String, SettingMetadata>>>,
}

impl LcdCommander {
    pub fn new(all_devices: AllDevices, repos: ReposByType) -> Self {
        Self {
            all_devices,
            repos,
            scheduled_settings: RefCell::new(HashMap::new()),
            scheduled_settings_metadata: RefCell::new(HashMap::new()),
        }
    }

    pub fn schedule_single_temp(
        &self,
        device_uid: &UID,
        channel_name: &str,
        lcd_settings: &LcdSettings,
    ) -> Result<()> {
        let temp_source = lcd_settings
            .temp_source()
            .cloned()
            .with_context(|| "Temp Source should be present for LCD Temp Scheduling")?;
        let _ = self
            .all_devices
            .get(temp_source.device_uid.as_str())
            .with_context(|| {
                format!(
                    "temp_source Device must currently be present to schedule lcd update: {}",
                    temp_source.device_uid
                )
            })?;
        let _ = self.all_devices.get(device_uid).with_context(|| {
            format!("Target Device to schedule lcd update must be present: {device_uid}")
        })?;
        self.scheduled_settings
            .borrow_mut()
            .entry(device_uid.clone())
            .or_default()
            .insert(channel_name.to_string(), lcd_settings.clone());
        self.scheduled_settings_metadata
            .borrow_mut()
            .entry(device_uid.clone())
            .or_default()
            .insert(channel_name.to_string(), SettingMetadata::default());
        Ok(())
    }

    pub async fn schedule_carousel(
        &self,
        device_uid: &UID,
        channel_name: &str,
        lcd_settings: &LcdSettings,
    ) -> Result<()> {
        let carousel = lcd_settings
            .carousel()
            .cloned()
            .with_context(|| "CarouselSettings should be present for LCD Carousel Scheduling")?;
        let images_path = carousel
            .images_path
            .as_ref()
            .with_context(|| "Images Path should be present for LCD Carousel Scheduling")?;
        if carousel.interval < 5 || carousel.interval > 900 {
            bail!("Interval should be between 5 and 900 for LCD Carousel Scheduling");
        }
        let lcd_info = self
            .all_devices
            .get(device_uid)
            .ok_or_else(|| CCError::NotFound {
                msg: format!("Device with UID:{device_uid}"),
            })?
            .borrow()
            .info
            .channels
            .get(channel_name)
            .ok_or_else(|| CCError::NotFound {
                msg: format!("Channel info; UID:{device_uid}; Channel Name: {channel_name}"),
            })?
            .lcd_info()
            .cloned()
            .ok_or_else(|| CCError::NotFound {
                msg: format!("LCD INFO; UID:{device_uid}; Channel Name: {channel_name}"),
            })?;
        let processed_images =
            processors::image::process_carousel_images(images_path, lcd_info).await?;
        // Backdate one interval so the carousel starts right after scheduling. Saturate at "now"
        // on the early-boot chance the subtraction would underflow the monotonic clock.
        let interval_instant = Instant::now()
            .checked_sub(Duration::from_secs(carousel.interval))
            .unwrap_or_else(Instant::now);
        let setting_metadata = SettingMetadata {
            interval_instant,
            processed_images,
            ..Default::default()
        };
        self.scheduled_settings
            .borrow_mut()
            .entry(device_uid.clone())
            .or_default()
            .insert(channel_name.to_string(), lcd_settings.clone());
        self.scheduled_settings_metadata
            .borrow_mut()
            .entry(device_uid.clone())
            .or_default()
            .insert(channel_name.to_string(), setting_metadata);
        Ok(())
    }

    pub fn clear_channel_setting(&self, device_uid: &UID, channel_name: &str) {
        if let Some(device_channel_settings) =
            self.scheduled_settings.borrow_mut().get_mut(device_uid)
        {
            device_channel_settings.remove(channel_name);
        }
        if let Some(device_channel_settings) = self
            .scheduled_settings_metadata
            .borrow_mut()
            .get_mut(device_uid)
        {
            device_channel_settings.remove(channel_name);
        }
    }

    pub async fn update_lcd(self: Rc<Self>) {
        moro_local::async_scope!(|scope| {
            self.set_single_temp_image(scope);
            self.set_carousel_lcd_image(scope);
        })
        .await;
    }

    /// Applies all Single-Temp scheduled settings
    fn set_single_temp_image<'s>(self: &Rc<Self>, scope: &'s Scope<'s, 's, ()>) {
        for (device_uid, channel_name, lcd_settings, current_source_temp_data) in
            self.determine_single_temps_to_display()
        {
            scope.spawn(self.clone().set_single_temp_lcd_image(
                device_uid,
                channel_name,
                lcd_settings,
                Rc::new(current_source_temp_data),
            ));
        }
    }

    #[allow(clippy::float_cmp)]
    fn determine_single_temps_to_display(
        &self,
    ) -> Vec<(DeviceUID, ChannelName, LcdSettings, TempData)> {
        let mut temps_to_display = Vec::new();
        for (device_uid, channel_settings) in self.scheduled_settings.borrow().iter() {
            for (channel_name, lcd_settings) in channel_settings {
                if lcd_settings.mode_name() != LcdModeName::Temp {
                    continue;
                }
                if let Some(current_source_temp_data) = self.get_source_temp_data(lcd_settings) {
                    let last_temp_set = self
                        .scheduled_settings_metadata
                        .borrow()
                        .get(device_uid)
                        .expect("lcd scheduler metadata for device should be present")
                        .get(channel_name)
                        .expect("lcd scheduler metadata by channel should be present")
                        .last_temp_set;
                    if last_temp_set == current_source_temp_data.temp {
                        trace!(
                            "lcd scheduler skipping image update as there is no temperature change: {}",
                            current_source_temp_data.temp
                        );
                    } else {
                        temps_to_display.push((
                            device_uid.clone(),
                            channel_name.clone(),
                            lcd_settings.clone(),
                            current_source_temp_data.clone(),
                        ));
                    }
                }
            }
        }
        temps_to_display
    }

    fn get_source_temp_data(&self, lcd_settings: &LcdSettings) -> Option<TempData> {
        let setting_temp_source = lcd_settings.temp_source().unwrap();
        if let Some(temp_source_device_lock) = self
            .all_devices
            .get(setting_temp_source.device_uid.as_str())
        {
            let device_read_lock = temp_source_device_lock.borrow();
            let label = device_read_lock
                .info
                .temps
                .iter()
                .find_map(|(temp_name, temp_info)| {
                    if temp_name == &setting_temp_source.temp_name {
                        Some(temp_info.label.clone())
                    } else {
                        None
                    }
                })?;
            let temp = device_read_lock
                .latest_temp(&setting_temp_source.temp_name)
                .map(|temp|
                    // rounded to nearest 10th degree to avoid updating on minuscule degree changes
                    (temp * 10.).round() / 10.)?;
            Some(TempData { temp, label })
        } else {
            error!(
                "Temperature Source Device for LCD Scheduler is currently not present: {}",
                setting_temp_source.device_uid
            );
            None
        }
    }

    /// Generates and applies the single-temp image for one channel. The CPU-bound image
    /// generation and the file write run on the blocking pool via `rt::spawn_blocking`,
    /// keeping the single-threaded runtime free during the multi-millisecond render.
    async fn set_single_temp_lcd_image(
        self: Rc<Self>,
        device_uid: UID,
        channel_name: ChannelName,
        lcd_settings: LcdSettings,
        temp_data_to_display: Rc<TempData>,
    ) {
        if lcd_settings.mode_name() != LcdModeName::Temp {
            return;
        }
        let start = Instant::now();
        // A settings change can clear this channel between scheduling and this task
        // running; bail out instead of panicking on a missing entry.
        let Some(image_template) = self
            .scheduled_settings_metadata
            .borrow()
            .get(&device_uid)
            .and_then(|channels| channels.get(&channel_name))
            .map(|metadata| metadata.image_template.clone())
        else {
            return;
        };
        let image_path =
            paths::config_dir().join(single_temp_image_filename(&device_uid, &channel_name));
        let generate_result = Self::generate_single_temp_image_file(
            temp_data_to_display.temp,
            temp_data_to_display.label.clone(),
            image_template,
            image_path.clone(),
        )
        .await;
        let Ok(image_template) = generate_result
            .inspect_err(|err| error!("Error generating image for lcd scheduler: {err}"))
        else {
            return;
        };
        let Ok(image_path_str) = image_path
            .to_str()
            .map(ToString::to_string)
            .ok_or_else(|| CCError::InternalError {
                msg: "Path to str conversion".to_string(),
            })
            .inspect_err(|err| error!("Error converting image path: {err}"))
        else {
            return;
        };
        let Some(lcd_settings) = self.build_image_settings_and_update_metadata(
            &device_uid,
            &channel_name,
            &lcd_settings,
            temp_data_to_display.temp,
            image_template,
            image_path_str,
        ) else {
            debug!("LCD channel {channel_name} was unscheduled mid-render; dropping image");
            return;
        };
        let device_type = self.all_devices[&device_uid].borrow().d_type;
        trace!("Time to generate LCD image: {:?}", start.elapsed());
        debug!("Applying scheduled LCD setting. Device: {device_uid}, Setting: {lcd_settings:?}");
        if let Some(repo) = self.repos.get(&device_type) {
            if let Err(err) = repo
                .apply_setting_lcd(&device_uid, &channel_name, &lcd_settings)
                .await
            {
                warn!("Error applying scheduled lcd setting for single-temp: {err}");
            }
        }
        trace!(
            "Time to generate LCD image and update device: {:?}",
            start.elapsed()
        );
    }

    /// Generates the single-temp PNG and writes it to `image_path`, both on the blocking
    /// pool so the single-threaded runtime is not stalled. Returns the reusable template.
    /// Takes the path as a parameter so tests can target a temp directory (the production
    /// path derives from the config dir, whose base is frozen process-wide on first use).
    ///
    /// If the LCD update timeout drops this future, the blocking task still runs to
    /// completion detached (neither backend cancels blocking tasks). Metadata is then not
    /// updated and the next trigger regenerates: wasted work only, no bad state.
    async fn generate_single_temp_image_file(
        temp: Temp,
        label: TempLabel,
        image_template: Option<ImageTemplate>,
        image_path: std::path::PathBuf,
    ) -> Result<ImageTemplate> {
        match crate::rt::spawn_blocking(move || {
            let (image_bytes, template) =
                IMAGE_GENERATOR.generate_single_temp_image(temp, &label, image_template)?;
            // Already on the blocking pool, so the std write costs the runtime nothing.
            std::fs::write(&image_path, &image_bytes)?;
            Ok(template)
        })
        .await
        {
            Ok(result) => result,
            Err(err) => Err(anyhow!("Image generation task failed: {err}")),
        }
    }

    /// Records the generated image in the channel metadata and builds the Image-mode
    /// settings to apply. Brightness and orientation are only sent on the first
    /// application; afterwards the device already has them. Returns `None` when the
    /// channel was unscheduled while the image rendered on the blocking pool; the
    /// caller drops the result.
    fn build_image_settings_and_update_metadata(
        &self,
        device_uid: &UID,
        channel_name: &str,
        scheduled_settings: &LcdSettings,
        displayed_temp: Temp,
        image_template: ImageTemplate,
        image_path_str: String,
    ) -> Option<LcdSettings> {
        let mut metadata_lock = self.scheduled_settings_metadata.borrow_mut();
        let metadata = metadata_lock
            .get_mut(device_uid)
            .and_then(|channels| channels.get_mut(channel_name))?;
        let brightness = if metadata.is_first_application {
            scheduled_settings.brightness
        } else {
            None
        };
        let orientation = if metadata.is_first_application {
            scheduled_settings.orientation
        } else {
            None
        };
        metadata.last_temp_set = displayed_temp;
        metadata.image_template = Some(image_template);
        metadata.is_first_application = false;
        Some(LcdSettings {
            brightness,
            orientation,
            colors: Vec::new(),
            mode: LcdModeKind::Image {
                image_file_processed: Some(image_path_str),
            },
        })
    }

    /// Applies all Carousel scheduled settings
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    fn set_carousel_lcd_image<'s>(&'s self, scope: &'s Scope<'s, 's, ()>) {
        for (device_uid, channel_settings) in self.scheduled_settings.borrow().iter() {
            for (channel_name, lcd_settings) in channel_settings {
                if lcd_settings.mode_name() != LcdModeName::Carousel {
                    continue;
                }
                let elapsed_secs = self
                    .scheduled_settings_metadata
                    .borrow()
                    .get(device_uid)
                    .expect("lcd scheduler metadata for device should be present")
                    .get(channel_name)
                    .expect("lcd scheduler metadata by channel should be present")
                    .interval_instant
                    .elapsed()
                    .as_secs_f64()
                    .round() as u64;
                if elapsed_secs
                    < lcd_settings
                        .carousel()
                        .expect("carousel lcd settings should be present")
                        .interval
                {
                    continue;
                }
                let (is_first_application, image_path) = {
                    let mut metadata_lock = self.scheduled_settings_metadata.borrow_mut();
                    let metadata = metadata_lock
                        .get_mut(device_uid)
                        .unwrap()
                        .get_mut(channel_name)
                        .unwrap();
                    let is_first_application = metadata.is_first_application;
                    let image_path = metadata
                        .processed_images
                        .get(metadata.image_index)
                        .unwrap()
                        .to_owned();
                    // circular indexing:
                    metadata.image_index =
                        (metadata.image_index + 1) % metadata.processed_images.len();
                    metadata.interval_instant = Instant::now();
                    metadata.is_first_application = false;
                    (is_first_application, image_path)
                };
                let brightness = if is_first_application {
                    lcd_settings.brightness
                } else {
                    None
                };
                let orientation = if is_first_application {
                    lcd_settings.orientation
                } else {
                    None
                };
                let lcd_settings = LcdSettings {
                    brightness,
                    orientation,
                    colors: Vec::new(),
                    mode: LcdModeKind::Image {
                        image_file_processed: Some(image_path),
                    },
                };
                let device_type = self.all_devices[device_uid].borrow().d_type;
                debug!(
                    "Applying scheduled LCD setting. Device: {device_uid}, Setting: {lcd_settings:?}"
                );
                let device_uid = device_uid.to_owned();
                let channel_name = channel_name.to_owned();
                scope.spawn(async move {
                    if let Some(repo) = self.repos.get(&device_type) {
                        if let Err(err) = repo
                            .apply_setting_lcd(&device_uid, &channel_name, &lcd_settings)
                            .await
                        {
                            warn!("Error applying scheduled lcd setting for carousel: {err}");
                        }
                    }
                });
            }
        }
    }
}

#[derive(Clone, Debug)]
struct TempData {
    pub temp: Temp,
    pub label: TempLabel,
}

#[derive(Clone)]
pub struct SettingMetadata {
    /// single-temp metadata
    pub last_temp_set: f64,
    pub image_template: Option<ImageTemplate>,

    /// carousel metadata
    pub interval_instant: Instant,
    pub processed_images: Vec<String>,
    pub image_index: usize,

    /// All
    pub is_first_application: bool,
}

impl Default for SettingMetadata {
    fn default() -> Self {
        Self {
            last_temp_set: f64::default(),
            image_template: None,
            interval_instant: Instant::now(),
            processed_images: Vec::new(),
            image_index: 0,
            is_first_application: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cc_fs;
    use crate::device::{Device, DeviceInfo, DeviceType, Status, TempInfo, TempStatus};
    use crate::setting::TempSource;
    use serial_test::serial;
    use std::ops::Not;

    const PNG_MAGIC_BYTES: [u8; 8] = [0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1A, b'\n'];

    /// A device that serves as both the LCD target and the temp source, reporting
    /// temp "cpu" at the given value in its current status.
    fn make_lcd_device(temp: f64) -> (UID, AllDevices) {
        let mut temps = HashMap::new();
        temps.insert(
            "cpu".to_string(),
            TempInfo {
                label: "CPU".to_string(),
                number: 1,
            },
        );
        let mut device = Device::new(
            "MockLcd".to_string(),
            DeviceType::Hwmon,
            1,
            None,
            DeviceInfo {
                temps,
                temp_min: 0,
                temp_max: 150,
                ..Default::default()
            },
            None,
            1.0,
        );
        let uid = device.uid.clone();
        device.initialize_status_history_with(
            Status {
                temps: vec![TempStatus {
                    name: "cpu".to_string(),
                    temp,
                }],
                ..Default::default()
            },
            1.0,
        );
        let mut all_devices = HashMap::new();
        all_devices.insert(uid.clone(), Rc::new(RefCell::new(device)));
        (uid, Rc::new(all_devices))
    }

    fn temp_lcd_settings(source_device_uid: &UID) -> LcdSettings {
        LcdSettings {
            brightness: None,
            orientation: None,
            colors: Vec::new(),
            mode: LcdModeKind::Temp {
                temp_source: Some(TempSource {
                    device_uid: source_device_uid.clone(),
                    temp_name: "cpu".to_string(),
                }),
            },
        }
    }

    /// Goal: distinct channels must never share an image file; sharing one path lets
    /// concurrent blocking-pool renders tear each other's writes. Method: assert the
    /// filename differs across devices and channels.
    #[test]
    fn single_temp_image_filenames_are_per_channel() {
        let uid_a = "a".repeat(64);
        let uid_b = "b".repeat(64);
        assert_ne!(
            single_temp_image_filename(&uid_a, "lcd1"),
            single_temp_image_filename(&uid_b, "lcd1")
        );
        assert_ne!(
            single_temp_image_filename(&uid_a, "lcd1"),
            single_temp_image_filename(&uid_a, "lcd2")
        );
    }

    /// Goal: an unchanged rounded temp must be filtered out before any task or blocking
    /// spawn is paid; the early-out lives upstream of the thread hop. Method: schedule a
    /// Temp setting, assert one display entry, then mark the current rounded temp as
    /// already set and assert the display list is empty. Pure sync test, no runtime.
    #[test]
    fn skip_unchanged_temp_before_any_spawn() {
        let (uid, all_devices) = make_lcd_device(45.64);
        let commander = LcdCommander::new(all_devices, HashMap::new());
        commander
            .schedule_single_temp(&uid, "lcd1", &temp_lcd_settings(&uid))
            .unwrap();
        assert_eq!(commander.determine_single_temps_to_display().len(), 1);
        // 45.64 rounds to the displayed 45.6; matching last_temp_set must skip.
        commander
            .scheduled_settings_metadata
            .borrow_mut()
            .get_mut(&uid)
            .unwrap()
            .get_mut("lcd1")
            .unwrap()
            .last_temp_set = 45.6;
        assert!(commander.determine_single_temps_to_display().is_empty());
    }

    /// Goal: the blocking-pool path must produce a valid PNG on disk and hand back the
    /// reusable template, on either runtime backend. Method: run the generation helper
    /// against a tempdir path, without and then with the returned template, asserting the
    /// PNG magic bytes both times.
    #[test]
    #[serial]
    fn single_temp_image_generated_on_blocking_pool() {
        cc_fs::test_runtime(async {
            let tmp_dir = tempfile::tempdir().unwrap();
            let image_path = tmp_dir
                .path()
                .join(single_temp_image_filename(&"uid1".to_string(), "lcd1"));
            let template = LcdCommander::generate_single_temp_image_file(
                45.6,
                "CPU".to_string(),
                None,
                image_path.clone(),
            )
            .await
            .unwrap();
            let image_bytes = std::fs::read(&image_path).unwrap();
            assert_eq!(image_bytes[..8], PNG_MAGIC_BYTES);
            LcdCommander::generate_single_temp_image_file(
                46.1,
                "CPU".to_string(),
                Some(template),
                image_path.clone(),
            )
            .await
            .unwrap();
            let image_bytes = std::fs::read(&image_path).unwrap();
            assert_eq!(image_bytes[..8], PNG_MAGIC_BYTES);
        });
    }

    /// Goal: recording a generated image must update the channel metadata exactly as the
    /// old inline block did: temp recorded, template stored, and brightness/orientation
    /// sent only on the first application. Method: schedule a Temp setting carrying both,
    /// record twice, and compare the built settings and metadata after each pass.
    #[test]
    fn image_metadata_updated_and_first_application_flips() {
        let (uid, all_devices) = make_lcd_device(45.6);
        let commander = LcdCommander::new(all_devices, HashMap::new());
        let mut scheduled = temp_lcd_settings(&uid);
        scheduled.brightness = Some(80);
        scheduled.orientation = Some(90);
        commander
            .schedule_single_temp(&uid, "lcd1", &scheduled)
            .unwrap();
        let (_, template) = IMAGE_GENERATOR
            .generate_single_temp_image(45.6, "CPU", None)
            .unwrap();
        let first = commander
            .build_image_settings_and_update_metadata(
                &uid,
                "lcd1",
                &scheduled,
                45.6,
                template,
                "/tmp/single_temp.png".to_string(),
            )
            .unwrap();
        assert_eq!(first.brightness, Some(80));
        assert_eq!(first.orientation, Some(90));
        assert_eq!(first.mode_name(), LcdModeName::Image);
        {
            let metadata_lock = commander.scheduled_settings_metadata.borrow();
            let metadata = metadata_lock.get(&uid).unwrap().get("lcd1").unwrap();
            assert!((metadata.last_temp_set - 45.6).abs() < f64::EPSILON);
            assert!(metadata.image_template.is_some());
            assert!(metadata.is_first_application.not());
        }
        let (_, template) = IMAGE_GENERATOR
            .generate_single_temp_image(46.1, "CPU", None)
            .unwrap();
        let second = commander
            .build_image_settings_and_update_metadata(
                &uid,
                "lcd1",
                &scheduled,
                46.1,
                template,
                "/tmp/single_temp.png".to_string(),
            )
            .unwrap();
        assert_eq!(second.brightness, None);
        assert_eq!(second.orientation, None);
    }

    /// Goal: a channel unscheduled while its image rendered must be dropped, not panic
    /// the daemon: the metadata entry is gone when the blocking task returns. Method:
    /// schedule, clear the channel, then record; expect None and no metadata entry.
    #[test]
    fn cleared_channel_mid_render_drops_result_without_panic() {
        let (uid, all_devices) = make_lcd_device(45.6);
        let commander = LcdCommander::new(all_devices, HashMap::new());
        let scheduled = temp_lcd_settings(&uid);
        commander
            .schedule_single_temp(&uid, "lcd1", &scheduled)
            .unwrap();
        commander.clear_channel_setting(&uid, "lcd1");
        let (_, template) = IMAGE_GENERATOR
            .generate_single_temp_image(45.6, "CPU", None)
            .unwrap();
        let result = commander.build_image_settings_and_update_metadata(
            &uid,
            "lcd1",
            &scheduled,
            45.6,
            template,
            "/tmp/single_temp.png".to_string(),
        );
        assert!(result.is_none());
    }
}
