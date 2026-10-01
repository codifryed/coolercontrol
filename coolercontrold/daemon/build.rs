// SPDX-FileCopyrightText: 2022 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use flate2::write::GzEncoder;
use flate2::Compression;
use std::fs;
use std::io::{self, Write};
use std::ops::Not;
use std::path::{Path, PathBuf};

/// File types worth gzipping. Images and fonts are compressed formats already.
const GZIP_EXTENSIONS: [&str; 6] = ["js", "css", "html", "webmanifest", "svg", "json"];
/// Below this, the saving is not worth a second copy in the binary.
const GZIP_MIN_BYTES: usize = 1024;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Rerun only when these change. Without any rerun directive cargo reruns this script on
    // every edit anywhere in the package, and it now compresses the whole UI.
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=resources/app");
    println!("cargo:rerun-if-changed=resources/proto");
    // The hwdata lookup below asks pkg-config, which reads this.
    println!("cargo:rerun-if-env-changed=PKG_CONFIG_PATH");

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
    precompress_app(
        &manifest_dir.join("resources/app"),
        &out_dir.join("app-gzip"),
    )?;

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

/// Writes a gzip copy of each compressible UI file to `target`, mirroring its path with a
/// `.gz` suffix. `api/base.rs` embeds `target` and serves these to clients that accept gzip,
/// so the daemon never compresses a static file per request.
///
/// Built from the exact bytes being embedded, the copies cannot go stale, and vendored builds
/// that ship a prebuilt `resources/app` get them too. `target` is rebuilt from scratch, since
/// a leftover copy of a file the UI no longer has would still be served.
fn precompress_app(app_dir: &Path, target: &Path) -> io::Result<()> {
    if target.exists() {
        fs::remove_dir_all(target)?;
    }
    fs::create_dir_all(target)?;
    if app_dir.is_dir().not() {
        return Ok(());
    }
    // Iterative rather than recursive: a stack of directories still to visit.
    let mut pending = vec![app_dir.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for entry in fs::read_dir(&dir)? {
            let path = entry?.path();
            if path.is_dir() {
                pending.push(path);
                continue;
            }
            let Ok(relative) = path.strip_prefix(app_dir) else {
                continue;
            };
            if let Some(gzipped) = gzip_if_worthwhile(&path)? {
                let mut name = relative.as_os_str().to_owned();
                name.push(".gz");
                let destination = target.join(name);
                if let Some(parent) = destination.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::write(destination, gzipped)?;
            }
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
