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

use anyhow::{anyhow, Context, Result};
use log::trace;
use ril::{Draw, Font, Image, ImageFormat, Rgba, TextAlign, TextLayout, TextSegment};
use std::io::Cursor;
use tiny_skia::{
    Color, FillRule, FilterQuality, GradientStop, Mask, Paint, PathBuilder, Pattern, Pixmap, Point,
    PremultipliedColorU8, Rect, SpreadMode, Transform,
};
use tokio::time::Instant;

const IMAGE_WIDTH: u32 = 320;
const IMAGE_HEIGHT: u32 = 320;
const FONT_MONO_BYTES: &[u8] = include_bytes!("../resources/RobotoMono-Medium.ttf");
const FONT_VARIABLE_BYTES: &[u8] = include_bytes!("../resources/Roboto-Regular.ttf");

pub const DEFAULT_LCD_SHUTDOWN_IMAGE: &[u8] = include_bytes!("../resources/lcd-shutdown.png");

/// Opaque wrapper around the internal ril image used as a reusable template
/// for LCD single-temp image generation.
#[derive(Clone)]
pub struct ImageTemplate(Image<Rgba>);

/// Generates LCD display images for single-temperature gauges.
///
/// Holds pre-loaded font resources and provides methods to render temperature
/// gauge images as PNG bytes.
pub struct LcdImageGenerator {
    font_mono: Font,
    font_variable: Font,
}

impl LcdImageGenerator {
    pub fn new() -> Self {
        Self {
            font_mono: Font::from_bytes(FONT_MONO_BYTES, 100.0).expect("font to load"),
            font_variable: Font::from_bytes(FONT_VARIABLE_BYTES, 100.0).expect("font to load"),
        }
    }

    /// Generates a single-temperature gauge image and returns the PNG bytes along with a
    /// reusable template. When an existing `template` is provided, the expensive gauge-circle
    /// generation is skipped and only the temperature text is re-rendered.
    pub fn generate_single_temp_image(
        &self,
        temp: f64,
        label: &str,
        template: Option<ImageTemplate>,
    ) -> Result<(Vec<u8>, ImageTemplate)> {
        let mut now = Instant::now();

        let mut image = if let Some(template) = template {
            template.0
        } else {
            self.generate_template(label, now)?
        };
        now = Instant::now();
        let template = ImageTemplate(image.clone());

        let temp_whole_number = format!("{:.0}", temp.trunc());
        let temp_decimal = format!("{:.0}", temp.fract() * 10.);
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

        let mut buf = Cursor::new(Vec::new());
        if let Err(e) = image.encode(ImageFormat::Png, &mut buf) {
            return Err(anyhow!("{e}"));
        }
        trace!("Image encoded in: {:?}", now.elapsed());
        Ok((buf.into_inner(), template))
    }

    #[allow(clippy::too_many_lines, clippy::cast_precision_loss)]
    fn generate_template(&self, label: &str, now: Instant) -> Result<Image<Rgba>> {
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
            pb.push_circle(circle_center_x, circle_center_y, outer_circle_radius);
            pb.push_circle(
                circle_center_x,
                circle_center_y,
                outer_circle_radius - boarder_thickness,
            );
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
        let temp_label = if label.len() < 8 {
            label
        } else if label.starts_with("CPU") {
            "CPU"
        } else if label.starts_with("GPU") {
            "GPU"
        } else if label.starts_with('Δ') {
            "Δ"
        } else {
            label.split_at(8).0
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
