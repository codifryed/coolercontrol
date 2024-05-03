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

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use log::{debug, error, trace};
use ril::{Draw, Font, Image, ImageFormat, Rgba, TextAlign, TextLayout, TextSegment};
use tiny_skia::{
    Color, FillRule, FilterQuality, GradientStop, Mask, Paint, PathBuilder, Pattern, Pixmap, Point,
    PremultipliedColorU8, Rect, SpreadMode, Transform,
};
use tokio::sync::RwLock;
use tokio::task;
use tokio::time::Instant;

use crate::config::DEFAULT_CONFIG_DIR;
use crate::device::{Temp, TempLabel, UID};
use crate::processing::settings::ReposByType;
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
    scheduled_settings: RwLock<HashMap<UID, HashMap<String, LcdSettings>>>,
    scheduled_settings_metadata: RwLock<HashMap<UID, HashMap<String, SettingMetadata>>>,
    font_mono: Font,
    font_variable: Font,
}

impl LcdCommander {
    pub fn new(all_devices: AllDevices, repos: ReposByType) -> Self {
        Self {
            all_devices,
            repos,
            scheduled_settings: RwLock::new(HashMap::new()),
            scheduled_settings_metadata: RwLock::new(HashMap::new()),
            font_mono: Font::from_bytes(FONT_MONO_BYTES, 100.0).expect("font to load"),
            font_variable: Font::from_bytes(FONT_VARIABLE_BYTES, 100.0).expect("font to load"),
        }
    }

    pub async fn schedule_setting(
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
            .write()
            .await
            .entry(device_uid.clone())
            .or_insert_with(HashMap::new)
            .insert(channel_name.to_string(), lcd_settings.clone());
        self.scheduled_settings_metadata
            .write()
            .await
            .entry(device_uid.clone())
            .or_insert_with(HashMap::new)
            .insert(channel_name.to_string(), SettingMetadata::new());
        Ok(())
    }

    pub async fn clear_channel_setting(&self, device_uid: &UID, channel_name: &str) {
        if let Some(device_channel_settings) =
            self.scheduled_settings.write().await.get_mut(device_uid)
        {
            device_channel_settings.remove(channel_name);
        }
        if let Some(device_channel_settings) = self
            .scheduled_settings_metadata
            .write()
            .await
            .get_mut(device_uid)
        {
            device_channel_settings.remove(channel_name);
        }
    }

    pub async fn update_lcd(self: Arc<Self>) {
        trace!("LCD Scheduler triggered");
        for (device_uid, channel_settings) in self.scheduled_settings.read().await.iter() {
            for (channel_name, lcd_settings) in channel_settings {
                if lcd_settings.mode != "temp" {
                    return;
                }
                if let Some(current_source_temp_data) =
                    self.get_source_temp_data(lcd_settings).await
                {
                    let last_temp_set = self
                        .scheduled_settings_metadata
                        .read()
                        .await
                        .get(device_uid)
                        .expect("lcd scheduler metadata for device should be present")
                        .get(channel_name)
                        .expect("lcd scheduler metadata by channel should be present")
                        .last_temp_set;
                    if last_temp_set.is_none()
                        || (last_temp_set.is_some()
                            && last_temp_set.unwrap() != current_source_temp_data.temp)
                    {
                        self.clone()
                            .set_lcd_image(
                                device_uid,
                                channel_name,
                                lcd_settings,
                                Arc::new(current_source_temp_data),
                            )
                            .await;
                    } else {
                        trace!("lcd scheduler skipping image update as there is no temperature change: {}", current_source_temp_data.temp);
                    }
                }
            }
        }
    }

    async fn get_source_temp_data(&self, lcd_settings: &LcdSettings) -> Option<TempData> {
        let setting_temp_source = lcd_settings.temp_source.as_ref().unwrap();
        if let Some(temp_source_device_lock) = self
            .all_devices
            .get(setting_temp_source.device_uid.as_str())
        {
            let device_read_lock = temp_source_device_lock.read().await;
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
                        .last()
                })
                .map(|temp_status|
                    // rounded to nearest 10th degree to avoid updating on really small degree changes
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

    /// The self: Arc<Self> is a 'trick' to be able to call methods that belong to self in another thread
    async fn set_lcd_image(
        self: Arc<Self>,
        device_uid: &UID,
        channel_name: &str,
        lcd_settings: &LcdSettings,
        temp_data_to_display: Arc<TempData>,
    ) {
        if lcd_settings.mode != "temp" {
            return;
        }
        // generating an image is a blocking operation, tokio spawn its own thread for this
        let self_clone = Arc::clone(&self);
        let temp_data = Arc::clone(&temp_data_to_display);
        let image_template = self
            .scheduled_settings_metadata
            .read()
            .await
            .get(device_uid)
            .unwrap()
            .get(channel_name)
            .unwrap()
            .image_template
            .clone();
        let generate_image = task::spawn_blocking(move || {
            self_clone.generate_single_temp_image(&temp_data, image_template)
        });
        let (image_path, image_template) = match generate_image.await {
            Ok(image_data_result) => match image_data_result {
                Ok(image_data) => image_data,
                Err(err) => {
                    error!("Error generating image for lcd scheduler: {}", err);
                    return;
                }
            },
            Err(err) => {
                error!("Error running image generation task: {}", err);
                return;
            }
        };

        let last_temp_set = self
            .scheduled_settings_metadata
            .read()
            .await
            .get(device_uid)
            .unwrap()
            .get(channel_name)
            .unwrap()
            .last_temp_set;
        let is_first_application = last_temp_set.is_none();
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
            colors: Vec::new(),
            temp_source: None,
        };
        {
            let mut metadata_lock = self.scheduled_settings_metadata.write().await;
            let metadata = metadata_lock
                .get_mut(device_uid)
                .unwrap()
                .get_mut(channel_name)
                .unwrap();
            metadata.last_temp_set = Some(temp_data_to_display.temp);
            metadata.image_template = image_template;
        }
        // this will block if reference is held, thus clone()
        let device_type = self.all_devices[device_uid].read().await.d_type.clone();
        debug!(
            "Applying scheduled LCD setting. Device: {}, Setting: {:?}",
            device_uid, lcd_settings
        );
        if let Some(repo) = self.repos.get(&device_type) {
            if let Err(err) = repo
                .apply_setting_lcd(device_uid, channel_name, &lcd_settings)
                .await
            {
                error!("Error applying scheduled lcd setting: {}", err);
            }
        }
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
            self.generate_single_temp_image_template(&temp_data_to_display, now)?
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

    fn generate_single_temp_image_template(
        &self,
        temp_data_to_display: &&TempData,
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
}

#[derive(Clone, Debug)]
struct TempData {
    pub temp: Temp,
    pub label: TempLabel,
}

#[derive(Clone)]
pub struct SettingMetadata {
    pub last_temp_set: Option<f64>,
    pub image_template: Option<Image<Rgba>>,
}

impl SettingMetadata {
    pub fn new() -> Self {
        Self {
            last_temp_set: None,
            image_template: None,
        }
    }
}
