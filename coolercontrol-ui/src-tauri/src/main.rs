/*
 * CoolerControl - monitor and control your cooling and other devices
 * Copyright (c) 2023  Guy Boldon
 * |
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation, either version 3 of the License, or
 * (at your option) any later version.
 * |
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 * |
 * You should have received a copy of the GNU General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 ******************************************************************************/

// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use tauri::{AppHandle, Context, Manager, SystemTray, SystemTrayEvent, WindowUrl};
use tauri::{CustomMenuItem, SystemTrayMenu, SystemTrayMenuItem};
use tauri::utils::assets::EmbeddedAssets;
use tauri::utils::config::AppUrl;
use tauri_plugin_autostart::MacosLauncher;

const DAEMON_ADDRESS: &str = "http://localhost";
const DAEMON_PORT: u16 = 11987;

fn main() {
    tauri::Builder::default()
        .system_tray(create_sys_tray())
        .on_system_tray_event(|app, event| handle_sys_tray_event(app, event))
        // .on_window_event(|event| handle_window_event(event))
        .plugin(tauri_plugin_localhost::Builder::new(DAEMON_PORT).build())
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .plugin(tauri_plugin_single_instance::init(|app, argv, cwd| {
            println!("{}, {argv:?}, {cwd}", app.package_info().name);
            app.emit_all("single-instance", Payload { args: argv, cwd }).unwrap();
        }))
        .plugin(tauri_plugin_autostart::init(MacosLauncher::LaunchAgent, Some(vec![])))
        .run(create_context())
        .expect("error while running tauri application");
}

fn create_sys_tray() -> SystemTray {
    let tray_menu_item_cc = CustomMenuItem::new("cc".to_string(), "CoolerControl").disabled();
    let tray_menu_item_hide = CustomMenuItem::new("hide".to_string(), "Hide");
    let tray_menu_item_quit = CustomMenuItem::new("quit".to_string(), "Quit");
    let tray_menu = SystemTrayMenu::new()
        .add_item(tray_menu_item_cc)
        .add_native_item(SystemTrayMenuItem::Separator)
        .add_item(tray_menu_item_hide)
        .add_item(tray_menu_item_quit);
    SystemTray::new().with_menu(tray_menu)
}

fn create_context() -> Context<EmbeddedAssets> {
    let mut context = tauri::generate_context!();
    let url = format!("{}:{}", DAEMON_ADDRESS, DAEMON_PORT).parse().unwrap();
    // rewrite the config so the IPC is enabled on this URL
    context.config_mut().build.dist_dir = AppUrl::Url(WindowUrl::External(url));
    context
}

fn handle_sys_tray_event(app: &AppHandle, event: SystemTrayEvent) {
    match event {
        SystemTrayEvent::MenuItemClick { id, .. } => {
            match id.as_str() {
                "quit" => {
                    std::process::exit(0);
                }
                "hide" => {
                    let item_handle = app.tray_handle().get_item(&id);
                    let window = app.get_window("main").unwrap();
                    if window.is_visible().unwrap() {
                        window.hide().unwrap();
                        item_handle.set_title("Show").unwrap();
                    } else {
                        window.show().unwrap();
                        item_handle.set_title("Hide").unwrap();
                    }
                }
                _ => {}
            }
        }
        _ => {}
    }
}

// todo: once we exchange settings from the UI and the Tauri backend
// fn handle_window_event(event: GlobalWindowEvent) {
//     match event.event() {
//         tauri::WindowEvent::CloseRequested { api, .. } => {
//              event.window().hide().unwrap();
//              api.prevent_close();
//         }
//         _ => {}
//     }
// }

#[derive(Clone, serde::Serialize)]
struct Payload {
    args: Vec<String>,
    cwd: String,
}