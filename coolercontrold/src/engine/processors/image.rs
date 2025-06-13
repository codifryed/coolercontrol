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

use sha2::Digest;
use std::io::Cursor;
use std::ops::Not;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use crate::api::CCError;
use crate::cc_fs;
use crate::config::DEFAULT_CONFIG_DIR;
use crate::device::LcdInfo;
use anyhow::{bail, Result};
use image::codecs::gif::GifDecoder;
use image::imageops::FilterType;
use image::AnimationDecoder;
use imgref::ImgVec;
use log::{debug, info, warn};
use mime::Mime;
use sha2::Sha256;
use tokio::io::AsyncReadExt;
use tokio::task::JoinHandle;

const SUPPORTED_IMAGE_FORMATS: [&str; 6] = ["jpg", "jpeg", "png", "gif", "tiff", "bmp"];
// This limits files we try to process to sizes possible to be acceptable to the LCD screen.
const MAX_SCANNABLE_IMAGE_FILE_SIZE_BYTES: u64 = 50_000_000; // 50MB
const MAX_CAROUSEL_ITEMS: usize = 50;
const CAROUSEL_IMAGE_DIRECTORY: &str = "carousel";

pub fn supported_image_types() -> [Mime; 5] {
    // replace with lazy_cell once in Rust stable.
    let image_tiff: Mime = Mime::from_str("image/tiff").unwrap();
    [
        mime::IMAGE_PNG,
        mime::IMAGE_GIF,
        mime::IMAGE_JPEG,
        mime::IMAGE_BMP,
        image_tiff,
    ]
}

/// This method takes uploaded image data and processes it in accordance to the LCD/Screens specifications.
/// This make sure that images are properly sized, cropped and standardized for our use.
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
            let mut image_pixels = Vec::new();
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
        image::load_from_memory(&file_data)?
            .resize_to_fill(screen_width, screen_height, FilterType::Lanczos3)
            .write_to(&mut image_output, image::ImageFormat::Png)?;
        Ok(image_output)
    });
    Ok((mime::IMAGE_PNG, join_handle.await??.into_inner()))
}

/// Processes images from a specified directory for display on an LCD screen carousel.
///
/// This function takes a path to a directory containing image files and processes them
/// according to the provided LCD screen specifications. It verifies the image files,
/// processes them to ensure they fit the screen dimensions, and stores the processed
/// images in a designated location if they do not already exist. The function supports
/// both static images and animated GIFs, converting all static outputs to PNG format.
///
/// # Arguments
///
/// * `images_path_str` - A `String` representing the file path to the directory containing the images.
/// * `lcd_info` - An `LcdInfo` struct containing the screen specifications such as width and height.
///
/// # Returns
///
/// This function returns a `Result` containing a vector of `String` paths to the processed images,
/// or an error if the processing fails at any stage.
pub async fn process_carousel_images(
    images_path_str: &str,
    lcd_info: LcdInfo,
) -> Result<Vec<String>> {
    let images_path = verify_images_path(images_path_str)?;
    let verified_image_paths = verify_images(&images_path)?;
    create_carousel_processed_image_directory()?;
    let mut processed_image_paths = Vec::new();
    for path in verified_image_paths {
        let image_hash = image_digest(&path)
            .await
            .inspect_err(|err| warn!("Error getting image digest: {err}"))?;
        // The content type is just used to know if it's processing an animated gif or a
        // static file, all processed static file output is encoded to png regardless of input.
        let content_type = if path.extension() == Some("gif".as_ref()) {
            mime::IMAGE_GIF
        } else {
            mime::IMAGE_PNG
        };
        let processed_image_path = create_carousel_lcd_image_path(&content_type, image_hash);
        debug!("New Image Path created: {}", processed_image_path.display());
        if processed_image_path.exists().not() {
            let file_data = cc_fs::read_image(&path).await?;
            let Ok((_, processed_image_data)) = process_image(
                content_type,
                file_data,
                lcd_info.screen_width,
                lcd_info.screen_height,
            )
            .await
            else {
                debug!("Image processing failed for path: {}", path.display());
                continue;
            };
            // We cut the official number in half here as there are issues with images bigger than
            // this, particularly for the carousel as it applies new gif/images regularly.
            if processed_image_data.len() > lcd_info.max_image_size_bytes as usize / 2 {
                info!(
                    "Skipping Image file {}, as after processing still too large for LCD. \
                    Max Size: {}MBs",
                    path.display(),
                    lcd_info.max_image_size_bytes / 1_000_000 / 2
                );
                // This will cause files that are on the edge size-wise to be processed again
                // on settings-application. Minor inconvenience, but we might want to think of an
                // alternative at some point.
                continue;
            }
            cc_fs::write(&processed_image_path, processed_image_data).await?;
        }
        let processed_image_path_str = processed_image_path
            .to_str()
            .map(ToString::to_string)
            .ok_or_else(|| CCError::InternalError {
                msg: "Path to str conversion".to_string(),
            })?;
        processed_image_paths.push(processed_image_path_str);
    }
    Ok(processed_image_paths)
}

/// Verifies that the given image path string is valid.
///
/// This function checks that the provided `images_path_str` is not empty,
/// exists as a path, and is a directory. If any of these conditions are not met,
/// it returns an error with a descriptive message. Otherwise, it returns the
/// valid `PathBuf`.
///
/// # Arguments
///
/// * `images_path_str` - A string representing the path to the images' directory.
///
/// # Returns
///
/// * `Result<PathBuf>` - A Result containing the valid `PathBuf` if the path is valid,
///   or an error if validation fails.
fn verify_images_path(images_path_str: &str) -> Result<PathBuf> {
    if images_path_str.is_empty() {
        bail!("Images Path String should not be empty for LCD Carousel Scheduling");
    }
    let images_path = Path::new(images_path_str).to_owned();
    if images_path.exists().not() {
        bail!("Images Path should exist for LCD Carousel Scheduling");
    }
    if images_path.is_dir().not() {
        bail!("Images Path should be a directory for LCD Carousel Scheduling");
    }
    Ok(images_path)
}

/// Verifies the images in a directory for LCD carousel scheduling.
///
/// This function reads the directory provided by `images_path` and filters out
/// any files that are not images of a supported format, or are too large
/// (>50MB). It stops after verifying `MAX_CAROUSEL_ITEMS` number of images.
/// If no valid images are found, it returns an error with a descriptive message.
///
/// # Arguments
///
/// * `images_path` - A `Path` to the directory containing the images.
///
/// # Returns
///
/// * `Result<Vec<PathBuf>>` - A Result containing a vector of valid image paths,
///   or an error if no valid images are found.
fn verify_images(images_path: &Path) -> Result<Vec<PathBuf>> {
    let verified_image_paths = cc_fs::read_dir(images_path)?
        .filter_map(|res_entry| {
            res_entry.map_or_else(
                |_| None,
                |entry| {
                    let path = entry.path();
                    let extension = path.extension()?.to_str()?;
                    if SUPPORTED_IMAGE_FORMATS.contains(&extension).not() {
                        debug!(
                            "Unsupported image format: {extension} at {}",
                            path.display()
                        );
                        return None;
                    }
                    let file_size_bytes = cc_fs::metadata(&path).map_or(0, |meta| meta.size());
                    if file_size_bytes > MAX_SCANNABLE_IMAGE_FILE_SIZE_BYTES {
                        info!("Image size too large for LCD (>50MB) at {}", path.display());
                        return None;
                    }
                    if file_size_bytes == 0 {
                        debug!("Image size is empty {}", path.display());
                        return None;
                    }
                    debug!("Image verification complete for: {}", path.display());
                    Some(path)
                },
            )
        })
        .take(MAX_CAROUSEL_ITEMS)
        .collect::<Vec<PathBuf>>();
    if verified_image_paths.is_empty() {
        bail!("Images Path should contain at least 1 valid image for LCD Carousel Scheduling");
    }
    debug!("Completed image verification for all contained images");
    Ok(verified_image_paths)
}

/// Creates the directory for storing processed carousel images if it doesn't already exist.
fn create_carousel_processed_image_directory() -> Result<()> {
    let carousel_processed_image_dir = Path::new(DEFAULT_CONFIG_DIR).join(CAROUSEL_IMAGE_DIRECTORY);
    if carousel_processed_image_dir.exists().not() {
        cc_fs::create_dir_all(&carousel_processed_image_dir)
            .inspect_err(|_| warn!("Error creating carousel processed image directory"))?;
    }
    Ok(())
}

/// Creates the path for the processed image to be stored.
///
/// Given a `content_type` and an `image_hash`, this function creates a path
/// under the `CAROUSEL_IMAGE_DIRECTORY` directory. The path is a combination of
/// the hash and the file extension, either ".gif" or ".png".
///
/// # Arguments
///
/// * `content_type` - The content type of the image, either `mime::IMAGE_GIF`
///   or something else.
/// * `image_hash` - The hash of the image, used to create a unique file name.
///
/// # Returns
///
/// A `PathBuf` representing the path to the processed image.
fn create_carousel_lcd_image_path(content_type: &Mime, image_hash: String) -> PathBuf {
    let image_path = if content_type == &mime::IMAGE_GIF {
        Path::new(DEFAULT_CONFIG_DIR)
            .join(CAROUSEL_IMAGE_DIRECTORY)
            .join(image_hash + ".gif")
    } else {
        Path::new(DEFAULT_CONFIG_DIR)
            .join(CAROUSEL_IMAGE_DIRECTORY)
            .join(image_hash + ".png")
    };
    image_path
}

/// Calculates the SHA256 digest of a file at the given path.
///
/// This function asynchronously reads the file and calculates its SHA256
/// digest. The digest is returned as a lowercase hexadecimal string.
///
/// # Arguments
///
/// * `path` - A `Path` pointing to the file to be hashed.
///
/// # Returns
///
/// A `Result` containing the SHA256 digest of the file as a string. If an
/// error occurs while reading the file, the error is propagated.
async fn image_digest(path: &Path) -> Result<String> {
    let file = tokio::fs::File::open(path).await?;
    let mut reader = tokio::io::BufReader::new(file);
    let sha256_hash = {
        let mut hasher = Sha256::new();
        let mut buffer = [0; 8192];
        loop {
            let count = reader.read(&mut buffer).await?;
            if count == 0 {
                break;
            }
            hasher.update(&buffer[..count]);
        }
        hasher.finalize()
    };
    Ok(format!("{sha256_hash:x}"))
}
