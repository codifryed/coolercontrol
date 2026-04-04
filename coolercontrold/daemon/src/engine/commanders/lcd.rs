/*
 * CoolerControl - monitor and control your cooling and other devices
 * Copyright (c) 2021-2025  Guy Boldon, Eren Simsek and contributors
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation, either version 3 of the License, or
 * (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

use anyhow::{bail, Context, Result};
use cc_image::{ImageTemplate, LcdImageGenerator};
use log::{debug, error, trace, warn};
use moro_local::Scope;
use std::cell::RefCell;
use std::collections::HashMap;
use std::io::Write;
use std::rc::Rc;
use std::time::Duration;
use tokio::time::Instant;

use crate::api::CCError;
use crate::device::{ChannelName, DeviceUID, Temp, TempLabel, UID};
use crate::engine::main::ReposByType;
use crate::engine::processors;
use crate::paths;
use crate::setting::{LcdModeName, LcdSettings};
use crate::AllDevices;

const IMAGE_FILENAME_SINGLE_TEMP: &str = "single_temp.png";
pub const DEFAULT_LCD_SHUTDOWN_IMAGE: &[u8] = cc_image::DEFAULT_LCD_SHUTDOWN_IMAGE;

/// This enables regularly updated LCD screen changes
pub struct LcdCommander {
    all_devices: AllDevices,
    repos: ReposByType,
    pub scheduled_settings: RefCell<HashMap<UID, HashMap<String, LcdSettings>>>,
    scheduled_settings_metadata: RefCell<HashMap<UID, HashMap<String, SettingMetadata>>>,
    image_generator: LcdImageGenerator,
}

impl LcdCommander {
    pub fn new(all_devices: AllDevices, repos: ReposByType) -> Self {
        Self {
            all_devices,
            repos,
            scheduled_settings: RefCell::new(HashMap::new()),
            scheduled_settings_metadata: RefCell::new(HashMap::new()),
            image_generator: LcdImageGenerator::new(),
        }
    }

    pub fn schedule_single_temp(
        &self,
        device_uid: &UID,
        channel_name: &str,
        lcd_settings: &LcdSettings,
    ) -> Result<()> {
        let temp_source = lcd_settings
            .temp_source
            .clone()
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
            .carousel
            .clone()
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
            .lcd_info
            .clone()
            .ok_or_else(|| CCError::NotFound {
                msg: format!("LCD INFO; UID:{device_uid}; Channel Name: {channel_name}"),
            })?;
        let processed_images =
            processors::image::process_carousel_images(images_path, lcd_info).await?;
        // This makes it so the carousel starts right after scheduling:
        let interval_instant = Instant::now() - Duration::from_secs(carousel.interval);
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
                if lcd_settings.mode != LcdModeName::Temp {
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
                        trace!("lcd scheduler skipping image update as there is no temperature change: {}", current_source_temp_data.temp);
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
        let setting_temp_source = lcd_settings.temp_source.as_ref().unwrap();
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
                .status_history
                .iter()
                .last()
                .and_then(|status| {
                    status
                        .temps
                        .iter()
                        .rfind(|temp_status| temp_status.name == setting_temp_source.temp_name)
                })
                .map(|temp_status|
                    // rounded to nearest 10th degree to avoid updating on minuscule degree changes
                    (temp_status.temp * 10.).round() / 10.)?;
            Some(TempData { temp, label })
        } else {
            error!(
                "Temperature Source Device for LCD Scheduler is currently not present: {}",
                setting_temp_source.device_uid
            );
            None
        }
    }

    /// The self: Rc<Self> is a 'trick' to be able to call methods that belong to self in another thread.
    #[allow(clippy::too_many_lines)] // Linear flow with early returns and async scope boilerplate.
    async fn set_single_temp_lcd_image(
        self: Rc<Self>,
        device_uid: UID,
        channel_name: ChannelName,
        lcd_settings: LcdSettings,
        temp_data_to_display: Rc<TempData>,
    ) {
        if lcd_settings.mode != LcdModeName::Temp {
            return;
        }
        let start = Instant::now();
        let image_template = self
            .scheduled_settings_metadata
            .borrow()
            .get(&device_uid)
            .unwrap()
            .get(&channel_name)
            .unwrap()
            .image_template
            .clone();
        let temp = temp_data_to_display.temp;
        let label = temp_data_to_display.label.clone();
        let self_clone = Rc::clone(&self);
        let generate_result =
            moro_local::async_scope!(|scope| -> Result<(Vec<u8>, ImageTemplate)> {
                scope
                    .spawn(async move {
                        self_clone.image_generator.generate_single_temp_image(
                            temp,
                            &label,
                            image_template,
                        )
                    })
                    .await
            })
            .await;
        let Ok((image_bytes, image_template)) = generate_result
            .inspect_err(|err| error!("Error generating image for lcd scheduler: {err}"))
        else {
            return;
        };

        let image_path = paths::config_dir().join(IMAGE_FILENAME_SINGLE_TEMP);
        // std blocking write:
        if let Err(err) =
            std::fs::File::create(&image_path).and_then(|mut f| f.write_all(&image_bytes))
        {
            error!("Error writing LCD image to disk: {err}");
            return;
        }
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

        let is_first_application = self
            .scheduled_settings_metadata
            .borrow()
            .get(&device_uid)
            .unwrap()
            .get(&channel_name)
            .unwrap()
            .is_first_application;
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
            mode: LcdModeName::Image,
            brightness,
            orientation,
            image_file_processed: Some(image_path_str),
            carousel: None,
            colors: Vec::new(),
            temp_source: None,
        };
        {
            let mut metadata_lock = self.scheduled_settings_metadata.borrow_mut();
            let metadata = metadata_lock
                .get_mut(&device_uid)
                .unwrap()
                .get_mut(&channel_name)
                .unwrap();
            metadata.last_temp_set = temp_data_to_display.temp;
            metadata.image_template = Some(image_template);
            metadata.is_first_application = false;
        }
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

    /// Applies all Carousel scheduled settings
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    fn set_carousel_lcd_image<'s>(&'s self, scope: &'s Scope<'s, 's, ()>) {
        for (device_uid, channel_settings) in self.scheduled_settings.borrow().iter() {
            for (channel_name, lcd_settings) in channel_settings {
                if lcd_settings.mode != LcdModeName::Carousel {
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
                        .carousel
                        .as_ref()
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
                    mode: LcdModeName::Image,
                    brightness,
                    orientation,
                    image_file_processed: Some(image_path),
                    carousel: None,
                    colors: Vec::new(),
                    temp_source: None,
                };
                let device_type = self.all_devices[device_uid].borrow().d_type;
                debug!("Applying scheduled LCD setting. Device: {device_uid}, Setting: {lcd_settings:?}");
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
