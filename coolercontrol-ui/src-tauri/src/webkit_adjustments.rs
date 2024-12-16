/*
 * CoolerControl - monitor and control your cooling and other devices
 * Copyright (c) 2021-2024  Guy Boldon, Eren Simsek and contributors
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
 ******************************************************************************/

use std::env;
use std::process::Command;

/// Disable DMA Rendering by default for `WebKit`.
/// This causes an issue with NVIDIA GPUs where the webview is blank.
/// Many distros have patched the official package and disabled this by default for NVIDIA GPUs.
pub fn handle_dma_rendering_for_nvidia_gpus() {
    if env::var("WEBKIT_FORCE_DMABUF_RENDERER").is_err() && has_nvidia() {
        env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
    }
    if env::var("WEBKIT_FORCE_COMPOSITING_MODE").is_err() && is_app_image() {
        // Needed so that the app image works on most all systems (system library dependant)
        env::set_var("WEBKIT_DISABLE_COMPOSITING_MODE", "1");
    }
}

fn has_nvidia() -> bool {
    let Ok(output) = Command::new("lspci").env("LC_ALL", "C").output() else {
        return false;
    };
    let Ok(output_str) = std::str::from_utf8(&output.stdout) else {
        return false;
    };
    output_str.to_uppercase().contains("NVIDIA")
}

fn is_app_image() -> bool {
    env::var("APPDIR").is_ok()
}
