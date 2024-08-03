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
use tauri::plugin::TauriPlugin;
use tauri::{plugin, Runtime};

fn is_app_image() -> bool {
    env::var("APPDIR").is_ok()
}

/// This is needed for the GTK3 application to be displayed with the correct top-level icon
/// under different Wayland compositors. (i.e. KDE Wayland)
/// https://sigxcpu.org/con/GTK__and_the_application_id.html
/// AppImage building uses its own script to set the identifier to the binary name.
pub fn init<R: Runtime>() -> TauriPlugin<R> {
    plugin::Builder::new("wayland-top-level-icon")
        .setup(|app, _api| {
            if !is_app_image() {
                glib::set_prgname(Some(app.config().identifier.clone()));
            }
            Ok(())
        })
        .build()
}
