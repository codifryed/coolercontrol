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

use std::io::Cursor;
use std::str::FromStr;

use anyhow::Result;
use image::codecs::gif::GifDecoder;
use image::imageops::FilterType;
use image::AnimationDecoder;
use imgref::ImgVec;
use mime::Mime;
use tokio::task::JoinHandle;

/// Returns the supported image MIME types for LCD screen uploads.
pub fn supported_image_types() -> [Mime; 5] {
    // Constant string literal, so parsing cannot fail.
    let image_tiff: Mime = Mime::from_str("image/tiff").unwrap();
    [
        mime::IMAGE_PNG,
        mime::IMAGE_GIF,
        mime::IMAGE_JPEG,
        mime::IMAGE_BMP,
        image_tiff,
    ]
}

/// Takes uploaded image data and processes it in accordance to the LCD/Screen specifications.
/// Makes sure that images are properly sized, cropped and standardized for our use.
pub async fn process_image(
    content_type: Mime,
    file_data: Vec<u8>,
    screen_width: u32,
    screen_height: u32,
) -> Result<(Mime, Vec<u8>)> {
    if content_type == mime::IMAGE_GIF {
        process_gif(file_data, screen_width, screen_height).await
    } else {
        process_static_image(file_data, screen_width, screen_height).await
    }
}

/// Our customized GIF processing implementation
async fn process_gif(
    file_data: Vec<u8>,
    screen_width: u32,
    screen_height: u32,
) -> Result<(Mime, Vec<u8>)> {
    // The collector and writer must be on separate threads:
    let (collector, writer) = gifski::new(gifski::Settings {
        width: None,
        height: None,
        quality: 100,
        fast: false,
        repeat: gifski::Repeat::Infinite,
    })?;
    // Tokio::task::spawn_blocking is preferred for these more expensive CPU operations,
    //  and these processing functions are rarely executed.
    let collector_handle: JoinHandle<Result<()>> = tokio::task::spawn_blocking(move || {
        let decoder = GifDecoder::new(Cursor::new(file_data))?;
        let frames = decoder.into_frames().collect_frames()?;
        let mut presentation_timestamp = 0.;
        for (index, frame) in frames.iter().enumerate() {
            let frame_image = image::DynamicImage::from(frame.buffer().clone())
                .resize_to_fill(
                    screen_width,
                    screen_height,
                    // Unfortunately the better filters have issues with the actual Kraken LCD:
                    FilterType::Nearest,
                )
                .to_rgba8();
            let pixel_count = (screen_width as usize) * (screen_height as usize);
            let mut image_pixels = Vec::with_capacity(pixel_count);
            for pixel in frame_image.pixels() {
                image_pixels.push(rgb::RGBA8::from(pixel.0));
            }
            let ms_ratio = frame.delay().numer_denom_ms();
            presentation_timestamp += (f64::from(ms_ratio.0) / f64::from(ms_ratio.1)) / 1_000.;
            collector.add_frame_rgba(
                index,
                ImgVec::new(image_pixels, screen_width as usize, screen_height as usize),
                presentation_timestamp,
            )?;
        }
        Ok(())
    });
    let writer_handle: JoinHandle<Result<Vec<u8>>> = tokio::task::spawn_blocking(move || {
        let mut gif_image_output = Cursor::new(Vec::new());
        writer.write(&mut gif_image_output, &mut gifski::progress::NoProgress {})?;
        Ok(gif_image_output.into_inner())
    });
    let data = writer_handle.await??;
    collector_handle.await??;
    Ok((mime::IMAGE_GIF, data))
}

async fn process_static_image(
    file_data: Vec<u8>,
    screen_width: u32,
    screen_height: u32,
) -> Result<(Mime, Vec<u8>)> {
    // Tokio::task::spawn_blocking is preferred for these more expensive CPU operations,
    //  and these processing functions are rarely executed.
    let join_handle: JoinHandle<Result<Cursor<Vec<u8>>>> = tokio::task::spawn_blocking(move || {
        let mut image_output = Cursor::new(Vec::new());
        let rgba = image::load_from_memory(&file_data)?
            .resize_to_fill(screen_width, screen_height, FilterType::Lanczos3)
            .to_rgba8();
        let rgb = flatten_alpha_to_black(&rgba);
        image::DynamicImage::ImageRgb8(rgb).write_to(&mut image_output, image::ImageFormat::Png)?;
        Ok(image_output)
    });
    Ok((mime::IMAGE_PNG, join_handle.await??.into_inner()))
}

/// Composites an RGBA image onto a black background, producing an RGB image.
/// Uses standard alpha blending: `channel_out = channel * alpha / 255`.
#[allow(clippy::cast_possible_truncation)]
fn flatten_alpha_to_black(rgba: &image::RgbaImage) -> image::RgbImage {
    let (width, height) = rgba.dimensions();
    let mut rgb = image::RgbImage::new(width, height);
    for (x, y, pixel) in rgba.enumerate_pixels() {
        let [r, g, b, a] = pixel.0;
        let alpha = u16::from(a);
        let out_r = (u16::from(r) * alpha / 255) as u8;
        let out_g = (u16::from(g) * alpha / 255) as u8;
        let out_b = (u16::from(b) * alpha / 255) as u8;
        rgb.put_pixel(x, y, image::Rgb([out_r, out_g, out_b]));
    }
    rgb
}
