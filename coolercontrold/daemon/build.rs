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

use protox::prost::Message;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
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
        if std::env::var("PROFILE").as_deref() == Ok("release") {
            panic!("{msg}");
        }
        println!("cargo:warning={msg}");
    }

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

    let out_dir = PathBuf::from(std::env::var("OUT_DIR")?);
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
    // compile_fds does not persist the descriptor set, so write it ourselves for the reflection
    // service (grpc_api loads it via include_file_descriptor_set!).
    std::fs::write(
        out_dir.join("device_service_descriptor.bin"),
        file_descriptor_set.encode_to_vec(),
    )?;
    tonic_prost_build::configure()
        .build_server(true)
        .build_client(true)
        .compile_fds(file_descriptor_set)?;
    Ok(())
}
