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
use std::ops::Not;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

use crate::api::CCError;
use crate::cc_fs;
use crate::device::LcdInfo;
use crate::paths;
use anyhow::{bail, Result};
use log::{debug, info, warn};
use mime::Mime;
use sha2::Sha256;
use tokio::io::AsyncReadExt;

pub use cc_image::{process_image, supported_image_types};

const SUPPORTED_IMAGE_FORMATS: [&str; 6] = ["jpg", "jpeg", "png", "gif", "tiff", "bmp"];
// This limits files we try to process to sizes possible to be acceptable to the LCD screen.
const MAX_SCANNABLE_IMAGE_FILE_SIZE_BYTES: u64 = 50_000_000; // 50MB
const MAX_CAROUSEL_ITEMS: usize = 50;
const CAROUSEL_IMAGE_DIRECTORY: &str = "carousel";

/// Processes images from a specified directory for display on an LCD screen carousel.
///
/// This function takes a path to a directory containing image files and processes them
/// according to the provided LCD screen specifications. It verifies the image files,
/// processes them to ensure they fit the screen dimensions, and stores the processed
/// images in a designated location if they do not already exist. The function supports
/// both static images and animated GIFs, converting all static outputs to PNG format.
pub async fn process_carousel_images(
    images_path_str: &str,
    lcd_info: LcdInfo,
) -> Result<Vec<String>> {
    let images_path = verify_images_path(images_path_str)?;
    let verified_image_paths = verify_images(&images_path)?;
    create_carousel_processed_image_directory().await?;
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

async fn create_carousel_processed_image_directory() -> Result<()> {
    let carousel_processed_image_dir = paths::config_dir().join(CAROUSEL_IMAGE_DIRECTORY);
    if carousel_processed_image_dir.exists().not() {
        cc_fs::create_dir_all(&carousel_processed_image_dir)
            .await
            .inspect_err(|_| warn!("Error creating carousel processed image directory"))?;
    }
    Ok(())
}

fn create_carousel_lcd_image_path(content_type: &Mime, image_hash: String) -> PathBuf {
    if content_type == &mime::IMAGE_GIF {
        paths::config_dir()
            .join(CAROUSEL_IMAGE_DIRECTORY)
            .join(image_hash + ".gif")
    } else {
        paths::config_dir()
            .join(CAROUSEL_IMAGE_DIRECTORY)
            .join(image_hash + ".png")
    }
}

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
    Ok(crate::hashutil::to_lower_hex(&sha256_hash))
}
