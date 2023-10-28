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

use tauri::{AppHandle, GlobalWindowEvent, Manager, SystemTray, SystemTrayEvent};
use tauri::{CustomMenuItem, SystemTrayMenu, SystemTrayMenuItem};

fn main() {
    // here `"quit".to_string()` defines the menu item id, and the second parameter is the menu item label.
    let tray_menu_item_cc = CustomMenuItem::new("cc".to_string(), "CoolerControl").disabled();
    let tray_menu_item_hide = CustomMenuItem::new("hide".to_string(), "Hide/Show");
    let tray_menu_item_quit = CustomMenuItem::new("quit".to_string(), "Quit");
    let tray_menu = SystemTrayMenu::new()
        .add_item(tray_menu_item_cc)
        .add_native_item(SystemTrayMenuItem::Separator)
        .add_item(tray_menu_item_hide)
        .add_item(tray_menu_item_quit);
    let system_tray = SystemTray::new()
        .with_menu(tray_menu);
    tauri::Builder::default()
        .system_tray(system_tray)
        .on_system_tray_event(|app, event| handle_sys_tray_event(app, event))
        .on_window_event(|event| handle_window_event(event))
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn handle_sys_tray_event(app: &AppHandle, event: SystemTrayEvent) {
    match event {
        SystemTrayEvent::MenuItemClick { id, .. } => {
            match id.as_str() {
                "quit" => {
                    std::process::exit(0);
                }
                "hide" => {
                    let window = app.get_window("main").unwrap();
                    if window.is_visible().unwrap() {
                        window.hide().unwrap();
                    } else {
                        window.show().unwrap();
                    }
                }
                _ => {}
            }
        }
        _ => {}
    }
}

fn handle_window_event(event: GlobalWindowEvent) {
    match event.event() {
        tauri::WindowEvent::CloseRequested { api, .. } => {
            event.window().hide().unwrap();
            api.prevent_close();
        }
        _ => {}
    }
}
