/*
 * CoolerControl - monitor and control your cooling and other devices
 * Copyright (c) 2021-2024  Guy Boldon, Eren Simsek and contributors
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

use anyhow::{anyhow, bail, Context, Result};
use log::{debug, error, trace, warn};
use moro_local::Scope;
use ril::{Draw, Font, Image, ImageFormat, Rgba, TextAlign, TextLayout, TextSegment};
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::Path;
use std::rc::Rc;
use std::time::Duration;
use tiny_skia::{
    Color, FillRule, FilterQuality, GradientStop, Mask, Paint, PathBuilder, Pattern, Pixmap, Point,
    PremultipliedColorU8, Rect, SpreadMode, Transform,
};
use tokio::time::Instant;

use crate::api::CCError;
use crate::config::DEFAULT_CONFIG_DIR;
use crate::device::{ChannelName, DeviceUID, Temp, TempLabel, UID};
use crate::engine::processors;
use crate::engine::settings::ReposByType;
use crate::setting::LcdSettings;
use crate::AllDevices;

const IMAGE_WIDTH: u32 = 320;
const IMAGE_HEIGHT: u32 = 320;
const IMAGE_FILENAME_SINGLE_TEMP: &str = "single_temp.png";
const FONT_MONO_BYTES: &[u8] = include_bytes!("../../../resources/RobotoMono-Medium.ttf");
const FONT_VARIABLE_BYTES: &[u8] = include_bytes!("../../../resources/Roboto-Regular.ttf");

/// This enables regularly updated LCD screen changes
pub struct LcdCommander {
    all_devices: AllDevices,
    repos: ReposByType,
    pub scheduled_settings: RefCell<HashMap<UID, HashMap<String, LcdSettings>>>,
    scheduled_settings_metadata: RefCell<HashMap<UID, HashMap<String, SettingMetadata>>>,
    font_mono: Font,
    font_variable: Font,
}

impl LcdCommander {
    pub fn new(all_devices: AllDevices, repos: ReposByType) -> Self {
        Self {
            all_devices,
            repos,
            scheduled_settings: RefCell::new(HashMap::new()),
            scheduled_settings_metadata: RefCell::new(HashMap::new()),
            font_mono: Font::from_bytes(FONT_MONO_BYTES, 100.0).expect("font to load"),
            font_variable: Font::from_bytes(FONT_VARIABLE_BYTES, 100.0).expect("font to load"),
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
                if lcd_settings.mode != "temp" {
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
                        .filter(|temp_status| temp_status.name == setting_temp_source.temp_name)
                        .next_back()
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

    /// The self: Rc<Self> is a 'trick' to be able to call methods that belong to self in another thread
    async fn set_single_temp_lcd_image(
        self: Rc<Self>,
        device_uid: UID,
        channel_name: ChannelName,
        lcd_settings: LcdSettings,
        temp_data_to_display: Rc<TempData>,
    ) {
        if lcd_settings.mode != "temp" {
            return;
        }
        // generating an image is a blocking operation, tokio spawn its own thread for this
        let start = Instant::now();
        let self_clone = Rc::clone(&self);
        let temp_data = Rc::clone(&temp_data_to_display);
        let image_template = self
            .scheduled_settings_metadata
            .borrow()
            .get(&device_uid)
            .unwrap()
            .get(&channel_name)
            .unwrap()
            .image_template
            .clone();
        let generate_image =
            moro_local::async_scope!(|scope| -> Result<(String, Option<Image<Rgba>>)> {
                scope
                    .spawn(async move {
                        self_clone.generate_single_temp_image(&temp_data, image_template)
                    })
                    .await
            })
            .await;
        let Ok((image_path, image_template)) = generate_image
            .inspect_err(|err| error!("Error generating image for lcd scheduler: {err}"))
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
            mode: "image".to_string(),
            brightness,
            orientation,
            image_file_processed: Some(image_path),
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
            metadata.image_template = image_template;
            metadata.is_first_application = false;
        }
        let device_type = self.all_devices[&device_uid].borrow().d_type.clone();
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

    /// Generates and saves an appropriate image and returns the path location for liquidctl
    /// INFO: this is a blocking call, takes CPU resources, and writes to the file system.
    fn generate_single_temp_image(
        &self,
        temp_data_to_display: &TempData,
        image_template: Option<Image<Rgba>>,
    ) -> Result<(String, Option<Image<Rgba>>)> {
        let mut now = Instant::now();

        let mut image = if let Some(image_template) = image_template {
            image_template
        } else {
            self.generate_single_temp_image_template(temp_data_to_display, now)?
        };
        now = Instant::now();
        let image_template = Some(image.clone());

        let temp_whole_number = format!("{:.0}", temp_data_to_display.temp.trunc());
        let temp_decimal = format!("{:.0}", temp_data_to_display.temp.fract() * 10.);
        TextSegment::new(&self.font_mono, &temp_whole_number, Rgba::white())
            .with_size(100.0)
            .with_position(60, 91)
            .draw(&mut image);
        TextSegment::new(&self.font_mono, ".", Rgba::white())
            .with_size(100.0)
            .with_position(160, 91)
            .draw(&mut image);
        TextSegment::new(&self.font_mono, &temp_decimal, Rgba::white())
            .with_size(100.0)
            .with_position(200, 91)
            .draw(&mut image);
        TextSegment::new(&self.font_mono, "°", Rgba::white())
            .with_size(35.0)
            .with_position(254, 113)
            .draw(&mut image);

        trace!("Image text rasterized in: {:?}", now.elapsed());
        now = Instant::now();

        let image_path = Path::new(DEFAULT_CONFIG_DIR).join(IMAGE_FILENAME_SINGLE_TEMP);
        // std blocking save being used here:
        if let Err(e) = image.save(ImageFormat::Png, &image_path) {
            return Err(anyhow!("{}", e.to_string()));
        }
        trace!("Image saved in: {:?}", now.elapsed());
        Ok((
            image_path
                .to_str()
                .with_context(|| "Path to String conversion")?
                .to_string(),
            image_template,
        ))
    }

    #[allow(clippy::too_many_lines, clippy::cast_precision_loss)]
    fn generate_single_temp_image_template(
        &self,
        temp_data_to_display: &TempData,
        now: Instant,
    ) -> Result<Image<Rgba>> {
        let circle_center_x = 160.0_f32;
        let circle_center_y = 160.0_f32;
        let outer_circle_radius = 160.0_f32;
        let middle_of_boarder_radius = 145.0_f32;
        let boarder_thickness = 30.0;
        // degrees start left center = 0, bottom center = 90, right center = 180
        let left_stop_degree = 45.0_f32;
        let left_stop_cos = left_stop_degree.to_radians().cos();
        let left_stop_sin = left_stop_degree.to_radians().sin();
        let right_stop_degree = 135.0_f32;
        let right_stop_cos = right_stop_degree.to_radians().cos();
        let right_stop_sin = right_stop_degree.to_radians().sin();
        let stop_point_outer_circle_left_x = outer_circle_radius * left_stop_cos + circle_center_x;
        let stop_point_outer_circle_left_y = outer_circle_radius * left_stop_sin + circle_center_y;
        let stop_point_outer_circle_right_x =
            outer_circle_radius * right_stop_cos + circle_center_x;
        let stop_point_outer_circle_right_y =
            outer_circle_radius * right_stop_sin + circle_center_y;
        let center_point_end_cap_left_x =
            middle_of_boarder_radius * left_stop_cos + circle_center_x;
        let center_point_end_cap_left_y =
            middle_of_boarder_radius * left_stop_sin + circle_center_y;
        let center_point_end_cap_right_x =
            middle_of_boarder_radius * right_stop_cos + circle_center_x;
        let center_point_end_cap_right_y =
            middle_of_boarder_radius * right_stop_sin + circle_center_y;

        // create clip path for hollow circle with thick boarder
        let clip_path = {
            let mut pb = PathBuilder::new();
            pb.push_circle(circle_center_x, circle_center_y, outer_circle_radius); // outer circle
            pb.push_circle(
                circle_center_x,
                circle_center_y,
                outer_circle_radius - boarder_thickness,
            ); // inner circle
            pb.finish().with_context(|| "Path builder creation")?
        };

        let mut clip_mask =
            Mask::new(IMAGE_WIDTH, IMAGE_HEIGHT).with_context(|| "Image Mask creation")?;
        clip_mask.fill_path(&clip_path, FillRule::EvenOdd, true, Transform::identity());

        let mut paint = Paint {
            shader: tiny_skia::LinearGradient::new(
                Point::from_xy(0.0, 0.0),
                Point::from_xy(320.0, 0.0),
                vec![
                    // todo: Color selection for future feature
                    GradientStop::new(0.0, Color::from_rgba8(0, 0, 255, 255)),
                    GradientStop::new(0.2, Color::from_rgba8(0, 0, 255, 255)),
                    GradientStop::new(0.8, Color::from_rgba8(255, 0, 00, 255)),
                    GradientStop::new(1.0, Color::from_rgba8(255, 0, 00, 255)),
                ],
                SpreadMode::Pad,
                Transform::identity(),
            )
            .with_context(|| "Shader creation")?,
            ..Default::default()
        };

        let initial_paint = paint.clone();

        let mut pixmap_foreground =
            Pixmap::new(IMAGE_WIDTH, IMAGE_HEIGHT).with_context(|| "Pixmap creation")?;
        pixmap_foreground.fill_rect(
            Rect::from_xywh(0.0, 0.0, IMAGE_WIDTH as f32, IMAGE_HEIGHT as f32)
                .with_context(|| "Rect creation")?,
            &paint,
            Transform::identity(),
            Some(&clip_mask),
        );

        // Smooth out gradient for a semi-circle (emulates a sweep gradient)
        paint.shader = tiny_skia::LinearGradient::new(
            Point::from_xy(0.0, 0.0),
            Point::from_xy(0.0, 320.0),
            vec![
                GradientStop::new(0.0, Color::from_rgba8(255, 0, 255, 130)),
                GradientStop::new(0.5, Color::from_rgba8(255, 0, 255, 0)),
            ],
            SpreadMode::Pad,
            Transform::identity(),
        )
        .with_context(|| "Shader creation")?;
        pixmap_foreground.fill_rect(
            Rect::from_xywh(0.0, 0.0, IMAGE_WIDTH as f32, IMAGE_HEIGHT as f32)
                .with_context(|| "Rect creation")?,
            &paint,
            Transform::identity(),
            Some(&clip_mask),
        );

        // Cut out the lower part of the circle
        let cut_out_path = {
            let mut pb = PathBuilder::new();
            pb.move_to(circle_center_x, circle_center_y);
            pb.line_to(
                stop_point_outer_circle_left_x,
                stop_point_outer_circle_left_y,
            );
            pb.line_to(stop_point_outer_circle_left_x, 320.0);
            pb.line_to(stop_point_outer_circle_right_x, 320.0);
            pb.line_to(
                stop_point_outer_circle_right_x,
                stop_point_outer_circle_right_y,
            );
            pb.close();
            pb.finish().with_context(|| "PathBuilder creation")?
        };
        let mut cut_out_paint = Paint::default();
        cut_out_paint.set_color(Color::BLACK);
        pixmap_foreground.fill_path(
            &cut_out_path,
            &cut_out_paint,
            FillRule::Winding,
            Transform::identity(),
            None,
        );
        // transform the black 'cut out' to transparent
        pixmap_foreground
            .pixels_mut()
            .iter_mut()
            .filter(|p| p.red() == 0 && p.green() == 0 && p.blue() == 0)
            .for_each(|p| *p = PremultipliedColorU8::TRANSPARENT);

        // Create the rounded end caps for the circle
        let mut paint_caps = initial_paint;
        paint_caps.anti_alias = true;
        let left_cap_path = PathBuilder::from_circle(
            center_point_end_cap_left_x,
            center_point_end_cap_left_y,
            boarder_thickness / 2.0,
        )
        .with_context(|| "PathBuilder creation")?;
        pixmap_foreground.fill_path(
            &left_cap_path,
            &paint_caps,
            FillRule::Winding,
            Transform::identity(),
            None,
        );
        let right_cap_path = PathBuilder::from_circle(
            center_point_end_cap_right_x,
            center_point_end_cap_right_y,
            boarder_thickness / 2.0,
        )
        .with_context(|| "PathBuilder creation")?;
        pixmap_foreground.fill_path(
            &right_cap_path,
            &paint_caps,
            FillRule::Winding,
            Transform::identity(),
            None,
        );

        // draw the background and then place the foreground on top of it.
        let mut pixmap =
            Pixmap::new(IMAGE_WIDTH, IMAGE_HEIGHT).with_context(|| "Pixmap creation")?;
        pixmap.fill(Color::BLACK);
        let paint = Paint {
            shader: Pattern::new(
                pixmap_foreground.as_ref(),
                SpreadMode::Pad,
                FilterQuality::Bicubic,
                1.0,
                Transform::identity(),
            ),
            ..Default::default()
        };

        pixmap.fill_rect(
            Rect::from_xywh(0.0, 0.0, IMAGE_WIDTH as f32, IMAGE_HEIGHT as f32)
                .with_context(|| "Rect creation")?,
            &paint,
            Transform::identity(),
            None,
        );

        // Convert to ril Rgba model for font rasterization (faster than png encoding/decoding)
        let rgb_pixels = pixmap
            .pixels()
            .iter()
            .map(|p| Rgba::new(p.red(), p.green(), p.blue(), p.alpha()))
            .collect::<Vec<Rgba>>();
        let mut image = Image::from_pixels(IMAGE_WIDTH, rgb_pixels);

        // draw temp name
        let temp_label = if temp_data_to_display.label.len() < 8 {
            &temp_data_to_display.label
        } else if temp_data_to_display.label.starts_with("CPU") {
            "CPU"
        } else if temp_data_to_display.label.starts_with("GPU") {
            "GPU"
        } else if temp_data_to_display.label.starts_with('Δ') {
            "Δ"
        } else {
            temp_data_to_display.label.split_at(8).0
        };
        TextLayout::new()
            .with_align(TextAlign::Center)
            .centered()
            .with_position(160, 232)
            .with_segment(
                &TextSegment::new(&self.font_variable, temp_label, Rgba::white()).with_size(35.0),
            )
            .draw(&mut image);
        trace!("Single Temp Image Template created in: {:?}", now.elapsed());
        Ok(image)
    }

    /// Applies all Carousel scheduled settings
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    fn set_carousel_lcd_image<'s>(&'s self, scope: &'s Scope<'s, 's, ()>) {
        for (device_uid, channel_settings) in self.scheduled_settings.borrow().iter() {
            for (channel_name, lcd_settings) in channel_settings {
                if lcd_settings.mode != "carousel" {
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
                    mode: "image".to_string(),
                    brightness,
                    orientation,
                    image_file_processed: Some(image_path),
                    carousel: None,
                    colors: Vec::new(),
                    temp_source: None,
                };
                let device_type = self.all_devices[device_uid].borrow().d_type.clone();
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
    pub image_template: Option<Image<Rgba>>,

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
