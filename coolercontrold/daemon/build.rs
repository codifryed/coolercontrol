// SPDX-FileCopyrightText: 2022 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use flate2::write::GzEncoder;
use flate2::{Compression, Crc};
use std::fs;
use std::io::{self, Write};
use std::ops::Not;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

/// File types worth gzipping. Images and fonts are compressed formats already.
const GZIP_EXTENSIONS: [&str; 6] = ["js", "css", "html", "webmanifest", "svg", "json"];
/// Below this, the saving is not worth a second copy in the binary.
const GZIP_MIN_BYTES: usize = 1024;
/// Embedded by api/plugins.rs, outside the UI dir. Moves together with its `include_file!`.
const PLUGIN_LIB: &str = "resources/lib/cc-plugin-lib.js";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Rerun only when these change. Without any rerun directive cargo reruns this script on
    // every edit anywhere in the package, and it now compresses the whole UI.
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=resources/app");
    println!("cargo:rerun-if-changed={PLUGIN_LIB}");
    println!("cargo:rerun-if-changed=resources/proto");
    // The hwdata lookup below asks pkg-config, which reads this.
    println!("cargo:rerun-if-env-changed=PKG_CONFIG_PATH");
    println!("cargo:rerun-if-env-changed=SOURCE_DATE_EPOCH");

    // Guard: the web UI is embedded into the binary from resources/app via
    // include_dir! (see api/base.rs). Building the daemon without first building
    // the UI embeds an empty directory, yielding a binary that serves a blank
    // web UI with no other error. Fail packaging (release) builds early here;
    // warn for debug builds so daemon-only iteration still works.
    let app_index =
        PathBuf::from(std::env::var("CARGO_MANIFEST_DIR")?).join("resources/app/index.html");
    if !app_index.exists() {
        let msg = "UI assets missing: resources/app/index.html not found. The daemon embeds \
            the coolercontrol-ui build at compile time, so the UI must be built first. From \
            the repo root run `make` (builds everything in the correct order), or `make \
            build-ui` to build just the UI.";
        assert!(
            std::env::var("PROFILE").as_deref() != Ok("release"),
            "{msg}"
        );
        println!("cargo:warning={msg}");
    }

    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR")?);
    let out_dir = PathBuf::from(std::env::var("OUT_DIR")?);
    embed_app_assets(&manifest_dir, &out_dir)?;

    // Query pkg-config for hwdata's pkgdatadir at build time (e.g., NixOS).
    if let Ok(output) = std::process::Command::new("pkg-config")
        .args(["hwdata", "--variable", "pkgdatadir"])
        .output()
    {
        if output.status.success() {
            let dir = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !dir.is_empty() && std::path::Path::new(&dir).is_dir() {
                println!("cargo:rustc-env=HWDATA_PKGDATADIR={dir}");
            }
        }
    }

    // Compile the protos with the pure-Rust protox compiler, then hand tonic the resulting descriptor
    // set for code generation. This removes protoc as a system build dependency. protox supports
    // proto3 optional natively, so no experimental protoc arg is needed.
    let file_descriptor_set = protox::compile(
        [
            "resources/proto/coolercontrol/models/v1/device.proto",
            "resources/proto/coolercontrol/device_service/v1/device_service.proto",
        ],
        ["resources/proto"],
    )?;
    tonic_prost_build::configure()
        .build_server(true)
        .build_client(true)
        .compile_fds(file_descriptor_set)?;
    Ok(())
}

/// Gzips the UI for api/base.rs and records the embedded assets' validators.
fn embed_app_assets(manifest_dir: &Path, out_dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let app_dir = manifest_dir.join("resources/app");
    let app_files = list_files(&app_dir)?;
    precompress_app(&app_dir, &app_files, &out_dir.join("app-gzip"))?;
    // api/last_modified.rs serves this as every embedded file's Last-Modified. include_dir's
    // own metadata is all or nothing, and musl cannot read the creation time it also asks for.
    let plugin_lib = manifest_dir.join(PLUGIN_LIB);
    let embedded: Vec<&PathBuf> = app_files.iter().chain([&plugin_lib]).collect();
    let assets_modified_unix_secs = assets_modified_unix_secs(embedded.iter().copied())?;
    println!("cargo:rustc-env=CC_ASSETS_MODIFIED_UNIX_SECS={assets_modified_unix_secs}");
    let assets_hash = assets_hash(manifest_dir, &embedded)?;
    println!("cargo:rustc-env=CC_ASSETS_ETAG=\"{assets_hash}\"");
    // A strong ETag differs across content codings (RFC 9110 8.8.3), so the gzip copies get
    // their own.
    println!("cargo:rustc-env=CC_ASSETS_ETAG_GZIP=\"{assets_hash}-gzip\"");
    Ok(())
}

/// The hash behind the strong `ETag`s, over every embedded file's path and bytes. Packagers
/// can pin `SOURCE_DATE_EPOCH` to one value for every release (nixpkgs does), so
/// Last-Modified alone would let a browser keep the old index.html after an upgrade.
fn assets_hash(
    manifest_dir: &Path,
    files: &[&PathBuf],
) -> Result<String, Box<dyn std::error::Error>> {
    // The plugin lib is embedded in every build, even a daemon-only one.
    assert!(files.is_empty().not(), "there is always an asset to hash");
    let mut relative = files
        .iter()
        .map(|path| path.strip_prefix(manifest_dir))
        .collect::<Result<Vec<_>, _>>()?;
    // Directory listing order is not stable across filesystems.
    relative.sort_unstable();
    // A file listed twice would hash differently from the same assets listed once.
    assert!(
        relative.windows(2).all(|pair| pair[0] != pair[1]),
        "each asset is hashed once"
    );
    let mut crc = Crc::new();
    for path in relative {
        let name = path.as_os_str().as_encoded_bytes();
        let bytes = fs::read(manifest_dir.join(path))?;
        // Length prefixes keep a byte moved between a name and its content from cancelling out.
        crc.update(&u64::try_from(name.len())?.to_le_bytes());
        crc.update(name);
        crc.update(&u64::try_from(bytes.len())?.to_le_bytes());
        crc.update(&bytes);
    }
    let hash = format!("{:08x}", crc.sum());
    // api/last_modified.rs quotes it into a strong ETag, which allows no quote inside.
    assert_eq!(hash.len(), 8, "a CRC32 is eight hex digits");
    assert!(
        hash.bytes().all(|byte| byte.is_ascii_hexdigit()),
        "{hash} is hex"
    );
    Ok(hash)
}

/// Every file under `dir`, at any depth. Empty when `dir` does not exist, as in a
/// daemon-only build.
fn list_files(dir: &Path) -> io::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    if dir.is_dir().not() {
        return Ok(files);
    }
    // Iterative rather than recursive: a stack of directories still to visit.
    let mut pending = vec![dir.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for entry in fs::read_dir(&dir)? {
            let path = entry?.path();
            if path.is_dir() {
                pending.push(path);
            } else {
                files.push(path);
            }
        }
    }
    Ok(files)
}

/// The embedded assets' modification time: `SOURCE_DATE_EPOCH` when set, so reproducible
/// builds stay reproducible, else the newest file's mtime.
fn assets_modified_unix_secs<'a>(
    files: impl IntoIterator<Item = &'a PathBuf>,
) -> Result<u64, Box<dyn std::error::Error>> {
    if let Ok(epoch) = std::env::var("SOURCE_DATE_EPOCH") {
        return epoch.trim().parse::<u64>().map_err(|err| {
            format!("SOURCE_DATE_EPOCH {epoch:?} is not a Unix time: {err}").into()
        });
    }
    let mut newest_unix_secs = 0;
    for path in files {
        let modified = fs::metadata(path)?.modified()?;
        let unix_secs = modified.duration_since(UNIX_EPOCH)?.as_secs();
        newest_unix_secs = newest_unix_secs.max(unix_secs);
    }
    Ok(newest_unix_secs)
}

/// Writes a gzip copy of each compressible UI file to `target`, mirroring its path with a
/// `.gz` suffix. `api/base.rs` embeds `target` and serves these to clients that accept gzip,
/// so the daemon never compresses a static file per request.
///
/// Built from the exact bytes being embedded, the copies cannot go stale, and vendored builds
/// that ship a prebuilt `resources/app` get them too. `target` is rebuilt from scratch, since
/// a leftover copy of a file the UI no longer has would still be served.
fn precompress_app(app_dir: &Path, app_files: &[PathBuf], target: &Path) -> io::Result<()> {
    if target.exists() {
        fs::remove_dir_all(target)?;
    }
    fs::create_dir_all(target)?;
    for path in app_files {
        let Ok(relative) = path.strip_prefix(app_dir) else {
            continue;
        };
        if let Some(gzipped) = gzip_if_worthwhile(path)? {
            let mut name = relative.as_os_str().to_owned();
            name.push(".gz");
            let destination = target.join(name);
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(destination, gzipped)?;
        }
    }
    Ok(())
}

/// The gzip of `path` when its type compresses and the result saves at least a tenth.
fn gzip_if_worthwhile(path: &Path) -> io::Result<Option<Vec<u8>>> {
    let compressible = path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| GZIP_EXTENSIONS.contains(&extension));
    if compressible.not() {
        return Ok(None);
    }
    let original = fs::read(path)?;
    if original.len() < GZIP_MIN_BYTES {
        return Ok(None);
    }
    // `GzEncoder` writes a zero timestamp and no file name, so the output is reproducible.
    let mut encoder = GzEncoder::new(Vec::with_capacity(original.len() / 2), Compression::best());
    encoder.write_all(&original)?;
    let gzipped = encoder.finish()?;
    if gzipped.len() * 10 > original.len() * 9 {
        return Ok(None);
    }
    Ok(Some(gzipped))
}
