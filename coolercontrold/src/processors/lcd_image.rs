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
 */

use anyhow::Result;
use mime::Mime;
use ril::{Encoder, EncoderMetadata, Frame, Image, ImageFormat, ImageSequence, ResizeAlgorithm, Rgba};
use ril::encodings::gif;
use ril::encodings::gif::GifEncoder;

/// This method takes uploaded image data and processes it in accordance to the LCD/Screens specifications.
/// This make sure that images are properly sized, cropped and standardized for our use.
pub fn process_image(
    content_type: &Mime,
    file_data: Vec<u8>,
    screen_width: u32,
    screen_height: u32,
) -> Result<(Mime, Vec<u8>)> {
    if content_type == &mime::IMAGE_GIF {
        process_gif(&file_data, screen_width, screen_height)
    } else {
        process_static_image(content_type, &file_data, screen_width, screen_height)
    }
}

/// Our customized GIF processing implementation
fn process_gif(file_data: &Vec<u8>, screen_width: u32, screen_height: u32) -> Result<(Mime, Vec<u8>)> {
    let mut processed_gif_image = ImageSequence::<Rgba>::new();
    let gif_image = ImageSequence::from_bytes(ImageFormat::Gif, file_data.as_slice())?;
    for frame in gif_image {
        let mut frame = frame?;
        if frame.width() == screen_width && frame.height() == screen_height {
            // We can skip image processing if we are at the target size
            processed_gif_image.push_frame(frame);
        } else {
            let frame_duration = frame.delay();
            let processed_image = cover(
                frame.image_mut(),
                screen_width,
                screen_height,
                // Gifs are a bit more complex other resizing algorithms seem to have negative affects
                ResizeAlgorithm::Nearest,
            );
            let mut processed_frame = Frame::from_image(processed_image.clone());
            processed_frame.set_delay(frame_duration);
            processed_gif_image.push_frame(processed_frame);
        }
    }
    let mut gif_image_output = Vec::new();
    // instead of the following method, we set increased quality for the gif encoder manually:
    // processed_gif_image.encode(ImageFormat::Gif, &mut gif_image_output)?;
    let encoder_options = gif::GifEncoderOptions::new().with_speed(1);
    let encoder_metadata = EncoderMetadata::from(&processed_gif_image)
        .with_config(encoder_options);
    let mut encoder = GifEncoder::new(&mut gif_image_output, encoder_metadata)?;
    for frame in processed_gif_image.iter() {
        encoder.add_frame(frame)?;
    }
    encoder.finish()?;
    Ok((mime::IMAGE_GIF, gif_image_output))
}

fn process_static_image(
    content_type: &Mime,
    file_data: &Vec<u8>,
    screen_width: u32,
    screen_height: u32,
) -> Result<(Mime, Vec<u8>)> {
    let mut image_output = Vec::new();
    let mut image: Image<Rgba> = if content_type == &mime::IMAGE_JPEG {
        Image::from_bytes(ImageFormat::Jpeg, file_data)?
    } else {
        Image::from_bytes(ImageFormat::Png, file_data)? // default
    };
    if image.width() == screen_width && image.height() == screen_height {
        // just re-encode if the image is already at the target size
        image.encode(ImageFormat::Png, &mut image_output)?;
    } else {
        let processed_image = cover(
            &mut image,
            screen_width,
            screen_height,
            // Lanczos3 produces great single image results and is 'fast enough':
            ResizeAlgorithm::Lanczos3,
        );
        processed_image.encode(ImageFormat::Png, &mut image_output)?;
    }
    Ok((mime::IMAGE_PNG, image_output))
}

/// Resizes the image so it covers the given bounding box, cropping the overflowing edges.
fn cover(image: &mut Image<Rgba>, target_width: u32, target_height: u32, algorithm: ResizeAlgorithm) -> &Image<Rgba> {
    let scale_factor =
        if target_width as f64 / target_height as f64 > image.width() as f64 / image.height() as f64 {
            target_width as f64 / image.width() as f64
        } else {
            target_height as f64 / image.height() as f64
        };
    let scaled_image = scale(image, scale_factor, algorithm);
    let width_to_crop_per_side = (scaled_image.width() - target_width) / 2;
    let height_to_crop_per_side = (scaled_image.height() - target_height) / 2;
    let x2 = scaled_image.width() - width_to_crop_per_side;
    let y2 = scaled_image.height() - height_to_crop_per_side;
    scaled_image.crop(
        width_to_crop_per_side, height_to_crop_per_side, x2, y2,
    );
    scaled_image
}

/// This scales the image, keeping aspect ratio
fn scale(image: &mut Image<Rgba>, scale_factor: f64, algorithm: ResizeAlgorithm) -> &mut Image<Rgba> {
    if scale_factor == 1. {
        image
    } else {
        let new_width = (image.width() as f64 * scale_factor).floor() as u32;
        let new_height = (image.height() as f64 * scale_factor).floor() as u32;
        image.resize(new_width, new_height, algorithm);
        image
    }
}