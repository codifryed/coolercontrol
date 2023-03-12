/*
 * CoolerControl - monitor and control your cooling and other devices
 * Copyright (c) 2023  Guy Boldon
 * |
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation, either version 3 of the License, or
 * (at your option) any later version.
 * |
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 * |
 * You should have received a copy of the GNU General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 ******************************************************************************/

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use log::{debug, error};
use ril::{Draw, Font, Image, ImageFormat, Rgba, TextLayout, TextSegment};
use tiny_skia::{ClipMask, Color, FillRule, FilterQuality, GradientStop, Paint, PathBuilder, Pattern, Pixmap, Point, PremultipliedColorU8, Rect, SpreadMode, Transform};
use tokio::sync::RwLock;
use tokio::task;
use tokio::time::Instant;

use crate::AllDevices;
use crate::config::DEFAULT_CONFIG_DIR;
use crate::device::{TempStatus, UID};
use crate::device_commander::ReposByType;
use crate::setting::{LcdSettings, Setting};

const IMAGE_WIDTH: u32 = 320;
const IMAGE_HEIGHT: u32 = 320;
const IMAGE_FILENAME_SINGLE_TEMP: &str = "single_temp.png";
const FONT_MONO_BYTES: &[u8] = include_bytes!("../resources/RobotoMono-Medium.ttf");
const FONT_VARIABLE_BYTES: &[u8] = include_bytes!("../resources/Roboto-Regular.ttf");

/// This enables regularly updated LCD screen changes
pub struct LcdScheduler {
    all_devices: AllDevices,
    repos: ReposByType,
    scheduled_settings: RwLock<HashMap<UID, HashMap<String, Setting>>>,
    scheduled_settings_metadata: RwLock<HashMap<UID, HashMap<String, SettingMetadata>>>,
    font_mono: Font,
    font_variable: Font,
}

impl LcdScheduler {
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

    pub async fn schedule_setting(&self, device_uid: &UID, setting: &Setting) -> Result<()> {
        if setting.temp_source.is_none() || setting.lcd.is_none() {
            return Err(anyhow!("Not enough info to schedule lcd updates"));
        }
        let temp_source = setting.temp_source.as_ref().unwrap();
        let _ = self.all_devices.get(temp_source.device_uid.as_str())
            .with_context(|| format!("temp_source Device must currently be present to schedule lcd update: {}", temp_source.device_uid))?;
        let _ = self.all_devices.get(device_uid)
            .with_context(|| format!("Target Device to schedule lcd update must be present: {}", device_uid))?;
        self.scheduled_settings.write().await
            .entry(device_uid.clone())
            .or_insert(HashMap::new())
            .insert(setting.channel_name.clone(), setting.clone());
        self.scheduled_settings_metadata.write().await
            .entry(device_uid.clone())
            .or_insert(HashMap::new())
            .insert(setting.channel_name.clone(), SettingMetadata::new());
        Ok(())
    }

    pub async fn clear_channel_setting(&self, device_uid: &UID, channel_name: &str) {
        if let Some(device_channel_settings) = self.scheduled_settings.write().await.get_mut(device_uid) {
            device_channel_settings.remove(channel_name);
        }
        if let Some(device_channel_settings) = self.scheduled_settings_metadata.write().await.get_mut(device_uid) {
            device_channel_settings.remove(channel_name);
        }
    }

    pub async fn update_lcd(self: Arc<Self>) {
        debug!("LCD Scheduler triggered");
        for (device_uid, channel_settings) in self.scheduled_settings.read().await.iter() {
            for (channel_name, scheduler_setting) in channel_settings {
                if scheduler_setting.lcd.as_ref().expect("lcd setting should be present").mode == "temp" {
                    if let Some(current_source_temp_status) = self.get_source_temp_status(scheduler_setting).await {
                        let last_temp_set = self.scheduled_settings_metadata.read().await
                            .get(device_uid).expect("lcd scheduler metadata for device should be present")
                            .get(channel_name).expect("lcd scheduler metadata by channel should be present")
                            .last_temp_set;
                        if last_temp_set.is_none()
                            || (last_temp_set.is_some() && last_temp_set.unwrap() != current_source_temp_status.temp) {
                            self.clone().set_lcd_image(device_uid, scheduler_setting, Arc::new(current_source_temp_status)).await
                        } else {
                            debug!("lcd scheduler skipping image update as there is no temperature change: {}", current_source_temp_status.temp)
                        }
                    }
                }
            }
        }
    }

    async fn get_source_temp_status(&self, setting: &Setting) -> Option<TempStatus> {
        if let Some(temp_source_device_lock) = self.all_devices
            .get(setting.temp_source.as_ref().unwrap().device_uid.as_str()) {
            temp_source_device_lock.read().await.status_history.iter()
                .last()
                .and_then(|status| status.temps.iter()
                    .filter(|temp_status| temp_status.name == setting.temp_source.as_ref().unwrap().temp_name)
                    .last()
                ).map(|temp_status|
                TempStatus {
                    // rounded to nearest 10th degree to avoid updating on really small degree changes
                    temp: (temp_status.temp * 10.).round() / 10.,
                    ..temp_status.clone()
                })
        } else {
            error!("Temperature Source Device for LCD Scheduler is currently not present: {}",
                setting.temp_source.as_ref().unwrap().device_uid);
            None
        }
    }

    /// The self: Arc<Self> is a 'trick' to be able to call methods that belong to self in another thread
    async fn set_lcd_image(self: Arc<Self>, device_uid: &UID, scheduler_setting: &Setting, temp_status_to_display: Arc<TempStatus>) {
        if scheduler_setting.lcd.as_ref().expect("lcd setting should be present").mode != "temp" {
            return;
        }

        // generating an image is a blocking operation, tokio spawn it's own thread for this
        let self_clone = Arc::clone(&self);
        let temp_status = Arc::clone(&temp_status_to_display);
        let generate_image = task::spawn_blocking(move || {
            self_clone.generate_single_temp_image(&temp_status)
        });
        let image_path = match generate_image.await {
            Ok(image_path_result) => match image_path_result {
                Ok(image_path) => image_path,
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

        let last_temp_set = self.scheduled_settings_metadata.read().await
            .get(device_uid).unwrap()
            .get(&scheduler_setting.channel_name).unwrap()
            .last_temp_set;
        let is_first_application = last_temp_set.is_none();
        let brightness = if is_first_application { scheduler_setting.lcd.as_ref().unwrap().brightness } else { None };
        let orientation = if is_first_application { scheduler_setting.lcd.as_ref().unwrap().orientation } else { None };
        let lcd_settings = LcdSettings {
            mode: "image".to_string(),
            brightness,
            orientation,
            image_file_src: None,
            image_file_processed: Some(image_path),
            colors: Vec::new(),
        };
        let generated_image_setting = Setting {
            channel_name: scheduler_setting.channel_name.clone(),
            speed_fixed: None,
            speed_profile: None,
            temp_source: None,
            lighting: None,
            lcd: Some(lcd_settings),
            pwm_mode: None,
            reset_to_default: None,
        };
        {
            let mut metadata_lock = self.scheduled_settings_metadata.write().await;
            let mut metadata = metadata_lock.get_mut(device_uid).unwrap()
                .get_mut(&scheduler_setting.channel_name).unwrap();
            metadata.last_temp_set = Some(temp_status_to_display.temp.clone());
            metadata.image_template = None;
        }
        // this will block if reference is held, thus clone()
        let device_type = self.all_devices[device_uid].read().await.d_type.clone();
        debug!("Applying scheduled lcd setting: {:?}", generated_image_setting);
        if let Some(repo) = self.repos.get(&device_type) {
            if let Err(err) = repo.apply_setting(device_uid, &generated_image_setting).await {
                error!("Error applying scheduled lcd setting: {}", err);
            }
        }
    }

    /// Generates and saves an appropriate image and returns the path location for liquidctl
    /// INFO: this is a blocking call, takes CPU resources, and writes to the file system.
    fn generate_single_temp_image(&self, temp_status_to_display: &TempStatus) -> Result<String> {
        let mut now = Instant::now();

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
        let stop_point_outer_circle_right_x = outer_circle_radius * right_stop_cos + circle_center_x;
        let stop_point_outer_circle_right_y = outer_circle_radius * right_stop_sin + circle_center_y;
        let center_point_end_cap_left_x = middle_of_boarder_radius * left_stop_cos + circle_center_x;
        let center_point_end_cap_left_y = middle_of_boarder_radius * left_stop_sin + circle_center_y;
        let center_point_end_cap_right_x = middle_of_boarder_radius * right_stop_cos + circle_center_x;
        let center_point_end_cap_right_y = middle_of_boarder_radius * right_stop_sin + circle_center_y;

        // create clip path for hollow circle with thick boarder
        let clip_path = {
            let mut pb = PathBuilder::new();
            pb.push_circle(circle_center_x, circle_center_y, outer_circle_radius); // outer circle
            pb.push_circle(circle_center_x, circle_center_y, outer_circle_radius - boarder_thickness); // inner circle
            pb.finish().with_context(|| "Path builder creation")?
        };

        let mut clip_mask = ClipMask::new();
        clip_mask.set_path(IMAGE_WIDTH, IMAGE_HEIGHT, &clip_path, FillRule::EvenOdd, true);

        let mut paint = Paint::default();
        paint.shader = tiny_skia::LinearGradient::new(
            Point::from_xy(0.0, 0.0),
            Point::from_xy(320.0, 0.0),
            vec![
                // todo: Color selection for future feature
                GradientStop::new(0.0, Color::from_rgba8(0, 0, 255, 255)),
                GradientStop::new(1.0, Color::from_rgba8(255, 0, 00, 255)),
            ],
            SpreadMode::Pad,
            Transform::identity(),
        ).with_context(|| "Shader creation")?;
        let initial_paint = paint.clone();

        let mut pixmap_foreground = Pixmap::new(IMAGE_WIDTH, IMAGE_HEIGHT)
            .with_context(|| "Pixmap creation")?;
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
                GradientStop::new(0.0, Color::from_rgba8(255, 0, 255, 150)),
                GradientStop::new(0.1, Color::from_rgba8(255, 0, 255, 120)),
                GradientStop::new(0.2, Color::from_rgba8(255, 0, 255, 100)),
                GradientStop::new(0.7, Color::from_rgba8(255, 0, 255, 0)),
            ],
            SpreadMode::Pad,
            Transform::identity(),
        ).with_context(|| "Shader creation")?;
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
            pb.line_to(stop_point_outer_circle_left_x, stop_point_outer_circle_left_y);
            pb.line_to(stop_point_outer_circle_left_x, 320.0);
            pb.line_to(stop_point_outer_circle_right_x, 320.0);
            pb.line_to(stop_point_outer_circle_right_x, stop_point_outer_circle_right_y);
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
        pixmap_foreground.pixels_mut().iter_mut()
            .filter(|p| p.red() == 0 && p.green() == 0 && p.blue() == 0)
            .for_each(|p| *p = PremultipliedColorU8::TRANSPARENT);


        // Create the rounded end caps for the circle
        let mut paint_caps = initial_paint;
        paint_caps.anti_alias = true;
        let left_cap_path = PathBuilder::from_circle(center_point_end_cap_left_x, center_point_end_cap_left_y, boarder_thickness / 2.0)
            .with_context(|| "PathBuilder creation")?;
        pixmap_foreground.fill_path(
            &left_cap_path,
            &paint_caps,
            FillRule::Winding,
            Transform::identity(),
            None,
        );
        let right_cap_path = PathBuilder::from_circle(center_point_end_cap_right_x, center_point_end_cap_right_y, boarder_thickness / 2.0)
            .with_context(|| "PathBuilder creation")?;
        pixmap_foreground.fill_path(
            &right_cap_path,
            &paint_caps,
            FillRule::Winding,
            Transform::identity(),
            None,
        );

        // draw the background and then place the foreground on top of it.
        let mut pixmap = Pixmap::new(IMAGE_WIDTH, IMAGE_HEIGHT)
            .with_context(|| "Pixmap creation")?;
        pixmap.fill(Color::BLACK);
        let mut paint = Paint::default();
        paint.shader = Pattern::new(pixmap_foreground.as_ref(), SpreadMode::Pad, FilterQuality::Bicubic, 1.0, Transform::identity());
        pixmap.fill_rect(
            Rect::from_xywh(0.0, 0.0, IMAGE_WIDTH as f32, IMAGE_HEIGHT as f32)
                .with_context(|| "Rect creation")?,
            &paint,
            Transform::identity(),
            None,
        );

        debug!("Base Image created in: {:?}", now.elapsed());
        now = Instant::now();

        // Convert to ril Rgba model for font rasterization (faster than png encoding/decoding)
        let rgb_pixels = pixmap.pixels().iter()
            .map(|p| Rgba::new(p.red(), p.green(), p.blue(), p.alpha()))
            .collect::<Vec<Rgba>>();
        let mut image = Image::from_pixels(IMAGE_WIDTH, &rgb_pixels);

        // draw temp name
        let temp_name = if temp_status_to_display.frontend_name.len() < 8 {
            &temp_status_to_display.frontend_name
        } else if temp_status_to_display.frontend_name.starts_with("CPU") {
            "CPU"
        } else if temp_status_to_display.frontend_name.starts_with("GPU") {
            "GPU"
        } else if temp_status_to_display.frontend_name.starts_with("Δ") {
            "Δ"
        } else {
            &temp_status_to_display.frontend_name.split_at(8).0
        };
        TextLayout::new()
            .centered()
            .with_position(160, 232)
            .with_segment(
                &TextSegment::new(&self.font_variable, temp_name, Rgba::white())
                    .with_size(35.0)
            ).draw(&mut image);

        // possible image template stop position, after this it's just the actual temp

        let temp_whole_number = format!("{:.0}", temp_status_to_display.temp.trunc());
        let temp_decimal = format!("{:.0}", temp_status_to_display.temp.fract() * 10.);
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

        debug!("Image text rasterized in: {:?}", now.elapsed());
        now = Instant::now();

        let image_path = Path::new(DEFAULT_CONFIG_DIR)
            .join(IMAGE_FILENAME_SINGLE_TEMP);
        if let Err(e) = image.save(ImageFormat::Png, &image_path) {
            return Err(anyhow!("{}", e.to_string()));
        }
        debug!("Image saved in: {:?}", now.elapsed());
        Ok(image_path.to_str().with_context(|| "Path to String conversion")?.to_string())
    }
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
