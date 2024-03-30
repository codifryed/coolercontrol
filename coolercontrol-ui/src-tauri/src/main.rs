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

use crate::port_finder::Port;
use serde_json::json;
use std::sync::{Mutex, MutexGuard};
use std::thread::sleep;
use std::time::Duration;
use tauri::utils::assets::EmbeddedAssets;
use tauri::utils::config::AppUrl;
use tauri::{AppHandle, Context, Manager, SystemTray, SystemTrayEvent, WindowUrl};
use tauri::{CustomMenuItem, SystemTrayMenu, SystemTrayMenuItem};
use tauri_plugin_store::StoreBuilder;

type UID = String;

// The store plugin places this in a data_dir, which is located at:
//  ~/.local/share/org.coolercontrol.coolercontrol/coolercontrol-ui.conf
const CONFIG_FILE: &str = "coolercontrol-ui.conf";
const CONFIG_START_IN_TRAY: &str = "start_in_tray";
const CONFIG_STARTUP_DELAY: &str = "startup_delay";

#[tauri::command]
async fn start_in_tray_enable(app_handle: AppHandle) {
    let mut store = StoreBuilder::new(app_handle, CONFIG_FILE.parse().unwrap()).build();
    let _ = store.load();
    let _ = store.insert(CONFIG_START_IN_TRAY.to_string(), json!(true));
    let _ = store.save();
}

#[tauri::command]
async fn start_in_tray_disable(app_handle: AppHandle) {
    let mut store = StoreBuilder::new(app_handle, CONFIG_FILE.parse().unwrap()).build();
    let _ = store.load();
    let _ = store.insert(CONFIG_START_IN_TRAY.to_string(), json!(false));
    let _ = store.save();
}

#[tauri::command]
async fn set_modes(
    modes: Vec<ModeTauri>,
    modes_state: tauri::State<'_, ModesState>,
    app_handle: AppHandle,
) -> Result<(), String> {
    let mut modes_state_lock = modes_state.modes.lock().expect("Modes State is poisoned");
    modes_state_lock.clear();
    modes_state_lock.extend(modes);
    let active_mode_lock = modes_state
        .active_mode
        .lock()
        .expect("Active Mode State is poisoned");
    recreate_mode_menu_items(app_handle, active_mode_lock, modes_state_lock);
    Ok(())
}

#[tauri::command]
async fn set_active_mode(
    active_mode_uid: Option<UID>,
    modes_state: tauri::State<'_, ModesState>,
    app_handle: AppHandle,
) -> Result<(), String> {
    let mut active_mode_lock = modes_state
        .active_mode
        .lock()
        .expect("Active Mode State is poisoned");
    *active_mode_lock = active_mode_uid;
    let modes_state_lock = modes_state.modes.lock().expect("Modes State is poisoned");
    recreate_mode_menu_items(app_handle, active_mode_lock, modes_state_lock);
    Ok(())
}

#[tauri::command]
async fn get_startup_delay(app_handle: AppHandle) -> Result<u64, String> {
    let mut store = StoreBuilder::new(app_handle, CONFIG_FILE.parse().unwrap()).build();
    let _ = store.load();
    store
        .get(CONFIG_STARTUP_DELAY)
        .unwrap_or(&json!(0))
        .as_u64()
        .ok_or_else(|| "Startup delay is not a number".to_string())
}

#[tauri::command]
async fn set_startup_delay(delay: u64, app_handle: AppHandle) {
    let mut store = StoreBuilder::new(app_handle, CONFIG_FILE.parse().unwrap()).build();
    let _ = store.load();
    let _ = store.insert(CONFIG_STARTUP_DELAY.to_string(), json!(delay));
    let _ = store.save();
}

fn recreate_mode_menu_items(
    app_handle: AppHandle,
    active_mode_lock: MutexGuard<Option<UID>>,
    modes_state_lock: MutexGuard<Vec<ModeTauri>>,
) {
    let modes_tray_menu = if modes_state_lock.len() > 0 {
        modes_state_lock
            .iter()
            .fold(create_starting_sys_tray_menu(), |menu, mode| {
                let mode_menu_item = if active_mode_lock
                    .as_ref()
                    .map(|uid| uid == &mode.uid)
                    .unwrap_or(false)
                {
                    CustomMenuItem::new(mode.uid.clone(), mode.name.clone()).selected()
                } else {
                    CustomMenuItem::new(mode.uid.clone(), mode.name.clone())
                };
                menu.add_item(mode_menu_item)
            })
            .add_native_item(SystemTrayMenuItem::Separator)
    } else {
        create_starting_sys_tray_menu()
    };
    app_handle
        .tray_handle()
        .set_menu(add_final_sys_tray_menu_items(modes_tray_menu))
        .expect("Failed to set new tray menu");
}

fn main() {
    let possible_port = port_finder::find_free_port();
    if possible_port.is_none() {
        println!("ERROR: No free port on localhost found, exiting.");
        std::process::exit(1);
    }
    let port: Port = possible_port.unwrap();
    tauri::Builder::default()
        .manage(ModesState::default())
        .system_tray(create_sys_tray())
        .on_system_tray_event(handle_sys_tray_event)
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_localhost::Builder::new(port).build())
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .plugin(tauri_plugin_single_instance::init(|app, argv, cwd| {
            println!("{}, {argv:?}, {cwd}", app.package_info().name);
            app.emit_all("single-instance", Payload { args: argv, cwd })
                .unwrap();
        }))
        .invoke_handler(tauri::generate_handler![
            start_in_tray_enable,
            start_in_tray_disable,
            set_modes,
            set_active_mode,
            get_startup_delay,
            set_startup_delay,
        ])
        .setup(|app| {
            match app.get_cli_matches() {
                Ok(matches) => {
                    if matches.args.get("help").is_some() {
                        println!(
                            "
CoolerControl GUI Desktop Application v{}

OPTIONS:
-h, --help       Print help information (this)
-v, --version    Print version information",
                            app.package_info().version
                        );
                        std::process::exit(0);
                    } else if matches.args.get("version").is_some()
                        && matches.args.get("version").unwrap().value.is_null()
                    {
                        // value is Bool(false) if no argument is given...
                        println!(
                            "CoolerControl GUI Desktop Application v{}",
                            app.package_info().version
                        );
                        std::process::exit(0);
                    }
                }
                Err(_) => {}
            }
            let mut store = StoreBuilder::new(app.handle(), CONFIG_FILE.parse()?).build();
            let _ = store.load();
            let delay = store
                .get(CONFIG_STARTUP_DELAY)
                .unwrap_or(&json!(0))
                .as_u64()
                .unwrap_or(0);
            if delay > 0 {
                println!("Delaying startup by {} seconds", delay);
                sleep(Duration::from_secs(delay));
            }
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
    let system_tray_menu = add_final_sys_tray_menu_items(create_starting_sys_tray_menu());
    SystemTray::new().with_menu(system_tray_menu)
}

fn create_starting_sys_tray_menu() -> SystemTrayMenu {
    let tray_menu_item_cc = CustomMenuItem::new("cc", "CoolerControl").disabled();
    
    SystemTrayMenu::new()
        .add_item(tray_menu_item_cc)
        .add_native_item(SystemTrayMenuItem::Separator)
}

fn add_final_sys_tray_menu_items(tray_menu: SystemTrayMenu) -> SystemTrayMenu {
    let tray_menu_item_show = CustomMenuItem::new("show", "Show/Hide");
    let tray_menu_item_quit = CustomMenuItem::new("quit", "Quit");
    tray_menu
        .add_item(tray_menu_item_show)
        .add_item(tray_menu_item_quit)
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
            _ => {
                if id.len() == 36 {
                    // Mode UUID
                    // println!("System Tray Menu Item Click with Mode ID: {}", id);
                    let modes_state = app.state::<ModesState>();
                    let active_mode_lock = modes_state
                        .active_mode
                        .lock()
                        .expect("Active Mode State is poisoned");
                    if let Some(active_mode) = active_mode_lock.as_ref() {
                        if active_mode == &id {
                            let modes_state_lock =
                                modes_state.modes.lock().expect("Modes State is poisoned");
                            // this sets the menu item back to selected
                            recreate_mode_menu_items(
                                app.app_handle(),
                                active_mode_lock,
                                modes_state_lock,
                            );
                            return;
                        }
                    }
                    app.emit_all(
                        "mode-activated",
                        EventPayload {
                            active_mode_uid: id,
                        },
                    )
                    .unwrap();
                }
            }
        },
        _ => {}
    }
}

#[derive(Clone, serde::Serialize)]
struct Payload {
    args: Vec<String>,
    cwd: String,
}

#[derive(Clone, serde::Serialize)]
struct EventPayload {
    active_mode_uid: UID,
}

#[derive(Default)]
struct ModesState {
    active_mode: Mutex<Option<UID>>,
    modes: Mutex<Vec<ModeTauri>>,
}

#[derive(Default, serde::Deserialize)]
struct ModeTauri {
    uid: UID,
    name: String,
}
