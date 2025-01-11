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

use crate::MAIN_WINDOW_ID;
use tauri::plugin::TauriPlugin;
use tauri::{plugin, AppHandle, Manager, RunEvent, Runtime};
use zbus::blocking::connection::Builder;
use zbus::blocking::Connection;
use zbus::interface;

const DBUS_NAME: &str = "org.coolercontrol.SingleInstance";
const DBUS_PATH: &str = "/org/coolercontrol/SingleInstance";
const DBUS_INTERFACE: &str = "org.coolercontrol.SingleInstance";

struct ConnectionHandle(Connection);

struct SingleInstanceDBus<R: Runtime> {
    app_handle: AppHandle<R>,
}

#[interface(name = "org.coolercontrol.SingleInstance")]
impl<R: Runtime> SingleInstanceDBus<R> {
    fn focus(&mut self) {
        println!(
            "A second instance was attempted to be started while this one is already running: {}",
            self.app_handle.package_info().name
        );
        println!("Showing the window of this already running instance if hidden.");
        let webview = self
            .app_handle
            .get_webview_window(MAIN_WINDOW_ID)
            .expect("Failed to get main window");
        webview.show().unwrap();
        webview.set_focus().unwrap();
    }
}

/// To be able to use a proper Gtk Application ID,
/// we need to handle the single instance logic for Tauri ourselves.
/// (The standard Gtk single-instance logic doesn't seem to work with Tauri.)
/// This replaces the single-instance Tauri plugin (and improves upon it for our use case)
pub fn init<R: Runtime>() -> TauriPlugin<R> {
    plugin::Builder::new("cc-single-instance")
        .setup(|app, _api| {
            let single_instance_dbus = SingleInstanceDBus {
                app_handle: app.clone(),
            };
            if let Ok(connection) = Builder::session()
                .unwrap()
                .name(DBUS_NAME)
                .unwrap()
                .serve_at(DBUS_PATH, single_instance_dbus)
                .unwrap()
                .build()
            {
                app.manage(ConnectionHandle(connection));
            }
            Ok(())
        })
        .on_event(|app, event| {
            if let RunEvent::Exit = event {
                destroy(app);
            }
        })
        .build()
}

/// This function should be called before the Tauri application Builder
/// to handle the single instance logic.
///
/// In contrast to the official plugin, we need to check for the existence of an already
/// running instance BEFORE the application is initialized.
pub fn handle_startup() {
    match Builder::session()
        .expect("Failed to create session connection builder")
        .name(DBUS_NAME)
        .expect("Failed to set DBUS name")
        .build()
    {
        Ok(connection) => {
            // When this is the first instance of the application,
            // stop here and let Tauri & Gtk start normally.
            connection
                .release_name(DBUS_NAME)
                .expect("Failed to release name");
            connection.close().expect("Failed to close connection");
        }
        Err(zbus::Error::NameTaken) => {
            println!(
                "There appears to already be an instance of CoolerControl running. Please check your \
            system tray for the application icon or the task manager to find the running instance."
            );
            if let Ok(connection) = Connection::session() {
                let _ = connection.call_method(
                    Some(DBUS_NAME),
                    DBUS_PATH,
                    Some(DBUS_INTERFACE),
                    "Focus",
                    &(),
                );
            }
            // This is a second instance, so we exit.
            std::process::exit(0);
        }
        _ => {}
    }
}

pub fn destroy<R: Runtime, M: Manager<R>>(manager: &M) {
    if let Some(connection) = manager.try_state::<ConnectionHandle>() {
        let _ = connection.0.release_name(DBUS_NAME);
    }
}
