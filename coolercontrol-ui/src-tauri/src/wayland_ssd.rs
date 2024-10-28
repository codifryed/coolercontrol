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

use gtk::prelude::{DisplayExtManual, GtkWindowExt};
use gtk::ApplicationWindow;
use std::env;
use tauri::plugin::TauriPlugin;
use tauri::{plugin, Runtime};

/// Initializes the Wayland SSD plugin.
///
/// This function sets up the plugin to remove client-side decorations for
/// non-GNOME desktops on Wayland, thereby forcing server-side decorations.
///
/// Returns a `TauriPlugin` instance.
pub fn init<R: Runtime>() -> TauriPlugin<R> {
    plugin::Builder::new("wayland-ssd")
        .on_window_ready(|window| {
            if !should_remove_csd() {
                return;
            }
            let Ok(gtk_window) = window.gtk_window() else {
                return;
            };
            remove_csd(&gtk_window);
        })
        .build()
}

fn should_remove_csd() -> bool {
    gdk::Display::default().is_some_and(|display| display.backend().is_wayland() && !is_gnome())
}

fn is_gnome() -> bool {
    env::var("XDG_CURRENT_DESKTOP")
        .map(|desktop| desktop == "GNOME")
        .unwrap_or(false)
}

/// Removes client-side decorations for the main window.
fn remove_csd(gtk_window: &ApplicationWindow) {
    gtk_window.set_titlebar(None::<&gtk::Widget>);
}
