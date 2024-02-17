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

mod port_finder;

use serde_json::json;
use tauri::utils::assets::EmbeddedAssets;
use tauri::utils::config::AppUrl;
use tauri::{AppHandle, Context, Manager, SystemTray, SystemTrayEvent, WindowUrl};
use tauri::{CustomMenuItem, SystemTrayMenu, SystemTrayMenuItem};
use tauri_plugin_autostart::MacosLauncher;
use tauri_plugin_store::StoreBuilder;
use crate::port_finder::Port;

// The store plugin places this in a data_dir, which is located at:
//  ~/.local/share/org.coolercontrol.coolercontrol/coolercontrol-ui.conf
const CONFIG_FILE: &str = "coolercontrol-ui.conf";

#[tauri::command]
async fn start_in_tray_enable(app_handle: tauri::AppHandle) {
    let mut store = StoreBuilder::new(app_handle, CONFIG_FILE.parse().unwrap()).build();
    let _ = store.load();
    let _ = store.insert("start_in_tray".to_string(), json!(true));
    let _ = store.save();
}

#[tauri::command]
async fn start_in_tray_disable(app_handle: tauri::AppHandle) {
    let mut store = StoreBuilder::new(app_handle, CONFIG_FILE.parse().unwrap()).build();
    let _ = store.load();
    let _ = store.insert("start_in_tray".to_string(), json!(false));
    let _ = store.save();
}

fn main() {
    let possible_port = port_finder::find_free_port();
    if possible_port.is_none() {
        println!("ERROR: No free port on localhost found, exiting.");
        std::process::exit(1);
    }
    let port: Port = possible_port.unwrap();
    tauri::Builder::default()
        .system_tray(create_sys_tray())
        .on_system_tray_event(|app, event| handle_sys_tray_event(app, event))
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_localhost::Builder::new(port).build())
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .plugin(tauri_plugin_single_instance::init(|app, argv, cwd| {
            println!("{}, {argv:?}, {cwd}", app.package_info().name);
            app.emit_all("single-instance", Payload { args: argv, cwd })
                .unwrap();
        }))
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec![]),
        ))
        .invoke_handler(tauri::generate_handler![
            start_in_tray_enable,
            start_in_tray_disable
        ])
        .setup(|app| {
            let mut store = StoreBuilder::new(app.handle(), CONFIG_FILE.parse()?).build();
            let _ = store.load();
            let start_in_tray = store
                .get("start_in_tray")
                .unwrap_or(&json!(false))
                .as_bool()
                .unwrap_or(false);
            if start_in_tray {
                let window = app.get_window("main").unwrap();
                window.hide().unwrap();
            }
            Ok(())
        })
        .run(create_context(port))
        .expect("error while running tauri application");
}

fn create_sys_tray() -> SystemTray {
    let tray_menu_item_cc = CustomMenuItem::new("cc".to_string(), "CoolerControl").disabled();
    let tray_menu_item_show = CustomMenuItem::new("show".to_string(), "Show/Hide");
    let tray_menu_item_quit = CustomMenuItem::new("quit".to_string(), "Quit");
    let tray_menu = SystemTrayMenu::new()
        .add_item(tray_menu_item_cc)
        .add_native_item(SystemTrayMenuItem::Separator)
        .add_item(tray_menu_item_show)
        .add_item(tray_menu_item_quit);
    SystemTray::new().with_menu(tray_menu)
}

fn create_context(port: Port) -> Context<EmbeddedAssets> {
    let mut context = tauri::generate_context!();
    // localhost plugin creates an asset http server at 'localhost:port' and this has to match
    let url = format!("http://localhost:{}", port).parse().unwrap();
    context.config_mut().build.dist_dir = AppUrl::Url(WindowUrl::External(url));
    context
}

fn handle_sys_tray_event(app: &AppHandle, event: SystemTrayEvent) {
    match event {
        // This isn't currently supported on Linux, but will leave for future use:
        SystemTrayEvent::DoubleClick { .. } => {
            let window = app.get_window("main").unwrap();
            window.show().unwrap();
            window.set_focus().unwrap();
        }
        SystemTrayEvent::MenuItemClick { id, .. } => match id.as_str() {
            "quit" => {
                app.exit(0);
            }
            "show" => {
                let window = app.get_window("main").unwrap();
                if window.is_visible().unwrap() {
                    // is_minimized() doesn't seem to work on Linux atm
                    if window.is_minimized().unwrap() {
                        window.unminimize().unwrap();
                        window.hide().unwrap();
                        window.show().unwrap();
                    } else {
                        window.hide().unwrap();
                    }
                } else {
                    window.show().unwrap();
                }
            }
            _ => {}
        },
        _ => {}
    }
}

#[derive(Clone, serde::Serialize)]
struct Payload {
    args: Vec<String>,
    cwd: String,
}
