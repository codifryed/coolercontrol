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
            // Fonts are static byte slices compiled into the binary, so parsing cannot fail.
            font_mono: Font::from_bytes(FONT_MONO_BYTES, 100.0).expect("embedded font is valid"),
            font_variable: Font::from_bytes(FONT_VARIABLE_BYTES, 100.0)
                .expect("embedded font is valid"),
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

        let mut buf = Cursor::new(Vec::with_capacity(64 * 1024));
        if let Err(e) = image.encode(ImageFormat::Png, &mut buf) {
            return Err(anyhow!("{e}"));
        }
        trace!("Image encoded in: {:?}", now.elapsed());
        Ok((buf.into_inner(), template))
    }

    #[allow(clippy::cast_precision_loss)]
    fn generate_template(&self, label: &str, now: Instant) -> Result<Image<Rgba>> {
        let pixmap_foreground = Self::render_gauge_arc()?;
        let mut image = Self::composite_to_ril_image(pixmap_foreground)?;
        Self::draw_temp_label(&self.font_variable, label, &mut image);
        trace!("Single Temp Image Template created in: {:?}", now.elapsed());
        Ok(image)
    }

    /// Renders the colored gauge arc as a foreground pixmap with transparent background.
    #[allow(clippy::cast_precision_loss)]
    fn render_gauge_arc() -> Result<Pixmap> {
        let center_x = 160.0_f32;
        let center_y = 160.0_f32;
        let outer_radius = 160.0_f32;
        let mid_radius = 145.0_f32;
        let border_thickness = 30.0_f32;
        // Degrees: left center = 0, bottom center = 90, right center = 180.
        let (left_cos, left_sin) = (45.0_f32.to_radians().cos(), 45.0_f32.to_radians().sin());
        let (right_cos, right_sin) = (135.0_f32.to_radians().cos(), 135.0_f32.to_radians().sin());

        let (initial_paint, mut pixmap_fg) =
            Self::paint_gradient_ring(center_x, center_y, outer_radius, border_thickness)?;

        Self::cut_bottom_section(
            &mut pixmap_fg,
            center_x,
            center_y,
            outer_radius,
            (left_cos, left_sin),
            (right_cos, right_sin),
        )?;

        Self::add_end_caps(
            &mut pixmap_fg,
            initial_paint,
            border_thickness,
            (
                mid_radius * left_cos + center_x,
                mid_radius * left_sin + center_y,
            ),
            (
                mid_radius * right_cos + center_x,
                mid_radius * right_sin + center_y,
            ),
        )?;

        Ok(pixmap_fg)
    }

    /// Paints the blue-to-red gradient ring onto a new foreground pixmap.
    /// Returns the initial paint (for end caps) and the foreground pixmap.
    #[allow(clippy::cast_precision_loss)]
    fn paint_gradient_ring(
        center_x: f32,
        center_y: f32,
        outer_radius: f32,
        border_thickness: f32,
    ) -> Result<(Paint<'static>, Pixmap)> {
        let clip_path = {
            let mut pb = PathBuilder::new();
            pb.push_circle(center_x, center_y, outer_radius);
            pb.push_circle(center_x, center_y, outer_radius - border_thickness);
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
                    GradientStop::new(0.0, Color::from_rgba8(0, 0, 255, 255)),
                    GradientStop::new(0.2, Color::from_rgba8(0, 0, 255, 255)),
                    GradientStop::new(0.8, Color::from_rgba8(255, 0, 0, 255)),
                    GradientStop::new(1.0, Color::from_rgba8(255, 0, 0, 255)),
                ],
                SpreadMode::Pad,
                Transform::identity(),
            )
            .with_context(|| "Shader creation")?,
            ..Default::default()
        };
        let initial_paint = paint.clone();
        let full_rect = Rect::from_xywh(0.0, 0.0, IMAGE_WIDTH as f32, IMAGE_HEIGHT as f32)
            .with_context(|| "Rect creation")?;

        let mut pixmap =
            Pixmap::new(IMAGE_WIDTH, IMAGE_HEIGHT).with_context(|| "Pixmap creation")?;
        pixmap.fill_rect(full_rect, &paint, Transform::identity(), Some(&clip_mask));

        // Smooth out gradient for a semi-circle (emulates a sweep gradient).
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
        pixmap.fill_rect(full_rect, &paint, Transform::identity(), Some(&clip_mask));

        Ok((initial_paint, pixmap))
    }

    /// Cuts out the bottom section of the arc and replaces black pixels with
    /// transparency. This creates the open-ended gauge appearance.
    fn cut_bottom_section(
        pixmap: &mut Pixmap,
        center_x: f32,
        center_y: f32,
        outer_radius: f32,
        left_stop: (f32, f32),
        right_stop: (f32, f32),
    ) -> Result<()> {
        let outer_left_x = outer_radius * left_stop.0 + center_x;
        let outer_left_y = outer_radius * left_stop.1 + center_y;
        let outer_right_x = outer_radius * right_stop.0 + center_x;
        let outer_right_y = outer_radius * right_stop.1 + center_y;

        let cut_path = {
            let mut pb = PathBuilder::new();
            pb.move_to(center_x, center_y);
            pb.line_to(outer_left_x, outer_left_y);
            pb.line_to(outer_left_x, 320.0);
            pb.line_to(outer_right_x, 320.0);
            pb.line_to(outer_right_x, outer_right_y);
            pb.close();
            pb.finish().with_context(|| "PathBuilder creation")?
        };
        let mut cut_paint = Paint::default();
        cut_paint.set_color(Color::BLACK);
        pixmap.fill_path(
            &cut_path,
            &cut_paint,
            FillRule::Winding,
            Transform::identity(),
            None,
        );

        // Replace the black cutout pixels with transparency.
        pixmap
            .pixels_mut()
            .iter_mut()
            .filter(|p| p.red() == 0 && p.green() == 0 && p.blue() == 0)
            .for_each(|p| *p = PremultipliedColorU8::TRANSPARENT);
        Ok(())
    }

    /// Adds rounded end caps at the arc endpoints.
    fn add_end_caps(
        pixmap: &mut Pixmap,
        initial_paint: Paint<'_>,
        border_thickness: f32,
        left_center: (f32, f32),
        right_center: (f32, f32),
    ) -> Result<()> {
        let mut paint = initial_paint;
        paint.anti_alias = true;
        let cap_radius = border_thickness / 2.0;

        let left_path = PathBuilder::from_circle(left_center.0, left_center.1, cap_radius)
            .with_context(|| "PathBuilder creation")?;
        pixmap.fill_path(
            &left_path,
            &paint,
            FillRule::Winding,
            Transform::identity(),
            None,
        );

        let right_path = PathBuilder::from_circle(right_center.0, right_center.1, cap_radius)
            .with_context(|| "PathBuilder creation")?;
        pixmap.fill_path(
            &right_path,
            &paint,
            FillRule::Winding,
            Transform::identity(),
            None,
        );
        Ok(())
    }

    /// Composites the foreground arc onto a black background and converts to
    /// the ril Rgba image model for font rasterization.
    #[allow(clippy::cast_precision_loss)]
    fn composite_to_ril_image(pixmap_foreground: Pixmap) -> Result<Image<Rgba>> {
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

        let pixel_count = (IMAGE_WIDTH * IMAGE_HEIGHT) as usize;
        let rgb_pixels: Vec<Rgba> = pixmap
            .pixels()
            .iter()
            .map(|p| Rgba::new(p.red(), p.green(), p.blue(), p.alpha()))
            .collect::<Vec<Rgba>>();
        debug_assert_eq!(rgb_pixels.len(), pixel_count);
        Ok(Image::from_pixels(IMAGE_WIDTH, rgb_pixels))
    }

    /// Draws the temperature label text at the bottom of the gauge.
    fn draw_temp_label(font: &Font, label: &str, image: &mut Image<Rgba>) {
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
            .with_segment(&TextSegment::new(font, temp_label, Rgba::white()).with_size(35.0))
            .draw(image);
    }
}
