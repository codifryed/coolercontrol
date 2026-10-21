// SPDX-FileCopyrightText: 2023 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use sha2::Digest;
use std::ops::Not;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

use crate::api::CCError;
use crate::cc_fs;
use crate::device::LcdInfo;
use crate::paths;
use anyhow::{anyhow, bail, Result};
use log::{debug, info, warn};
use mime::Mime;
use sha2::Sha256;

pub use cc_image::supported_image_types;

/// Processes an uploaded image for the LCD, offloaded to a blocking thread.
///
/// `cc_image::process_image` is synchronous and CPU-bound (decode, resize, re-encode, plus gifski's
/// worker threads for GIFs). Running it via `crate::rt::spawn_blocking` keeps it off the async
/// runtime and works on either backend, so `cc_image` itself stays runtime-agnostic.
pub async fn process_image(
    content_type: Mime,
    file_data: Vec<u8>,
    screen_width: u32,
    screen_height: u32,
) -> Result<(Mime, Vec<u8>)> {
    crate::rt::spawn_blocking(move || {
        cc_image::process_image(&content_type, file_data, screen_width, screen_height)
    })
    .await
    .map_err(|err| anyhow!("Image processing task failed: {err}"))?
}

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
///
/// The scan runs on the blocking pool: the directory is the user's, maybe on a network or
/// removable mount, and up to `MAX_CAROUSEL_ITEMS` images of up to 50 MB are each hashed with
/// SHA-256 and processed.
pub async fn process_carousel_images(
    images_path_str: &str,
    lcd_info: LcdInfo,
) -> Result<Vec<String>> {
    let images_path_str = images_path_str.to_owned();
    crate::rt::spawn_blocking(move || process_carousel_images_blocking(&images_path_str, &lcd_info))
        .await
        .map_err(|err| anyhow!("Carousel image processing failed: {err}"))?
}

/// `process_carousel_images` as blocking calls.
fn process_carousel_images_blocking(
    images_path_str: &str,
    lcd_info: &LcdInfo,
) -> Result<Vec<String>> {
    let images_path = verify_images_path(images_path_str)?;
    let verified_image_paths = verify_images(&images_path)?;
    debug_assert!(verified_image_paths.len() <= MAX_CAROUSEL_ITEMS);
    create_carousel_processed_image_directory()?;
    let mut processed_image_paths = Vec::with_capacity(verified_image_paths.len());
    for path in verified_image_paths {
        // Read once: the hash names the processed file, and the bytes are processed if it is new.
        let file_data = std::fs::read(&path)?;
        let image_hash = image_digest(&file_data);
        // The content type is just used to know if it's processing an animated gif or a
        // static file, all processed static file output is encoded to png regardless of input.
        // A screen that cannot animate still shows the gif's first frame, which is what
        // the static path encodes. Skipping the file would empty a carousel of gifs instead.
        let content_type = if path.extension() == Some("gif".as_ref()) && lcd_info.gif_supported {
            mime::IMAGE_GIF
        } else {
            mime::IMAGE_PNG
        };
        let processed_image_path = create_carousel_lcd_image_path(&content_type, image_hash);
        debug!("New Image Path created: {}", processed_image_path.display());
        if processed_image_path.exists().not() {
            let Some((_, processed_image_data)) = skip_on_panic(|| {
                cc_image::process_image(
                    &content_type,
                    file_data,
                    lcd_info.screen_width,
                    lcd_info.screen_height,
                )
            }) else {
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
            std::fs::write(&processed_image_path, processed_image_data)?;
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

/// One image's processing, with a failure or a decoder panic on a malformed file both a skip: the
/// whole scan runs in one blocking task, so a panic must not fail the rest of the Carousel.
fn skip_on_panic<T, E>(process: impl FnOnce() -> std::result::Result<T, E>) -> Option<T> {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(process)) {
        Ok(Ok(processed)) => Some(processed),
        Ok(Err(_)) | Err(_) => None,
    }
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

fn create_carousel_processed_image_directory() -> Result<()> {
    let carousel_processed_image_dir = paths::config_dir().join(CAROUSEL_IMAGE_DIRECTORY);
    if carousel_processed_image_dir.exists().not() {
        std::fs::create_dir_all(&carousel_processed_image_dir)
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

/// Names a processed image by its source's contents. Image files are bounded, so they are read
/// whole to hash.
fn image_digest(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    crate::hashutil::to_lower_hex(&hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_processed_image_is_named_by_the_sha256_of_its_source() {
        // Goal: hashing the bytes once read keeps the names earlier processed files were saved
        // under, so none is processed again. Method: the FIPS 180-2 test vector for "abc".
        assert_eq!(
            image_digest(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn a_panicking_or_failing_image_is_skipped() {
        // Goal: one malformed image that panics its decoder is skipped like one that fails,
        // leaving the rest of the Carousel to process. Method: run a processed, a failing and a
        // panicking image through the per-image guard.
        assert_eq!(skip_on_panic(|| Ok::<_, ()>(7)), Some(7));
        assert_eq!(skip_on_panic(|| Err::<u8, _>("unsupported format")), None);
        assert_eq!(
            skip_on_panic(|| -> std::result::Result<u8, ()> { panic!("malformed image") }),
            None
        );
    }
}
