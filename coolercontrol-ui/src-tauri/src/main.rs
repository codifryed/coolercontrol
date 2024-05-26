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

mod port_finder;

use crate::port_finder::Port;
use serde_json::json;
use std::env;
use std::process::Command;
use std::sync::{Mutex, MutexGuard};
use std::thread::sleep;
use std::time::Duration;
use tauri::utils::assets::EmbeddedAssets;
use tauri::utils::config::AppUrl;
use tauri::{command, AppHandle, Context, Manager, SystemTray, SystemTrayEvent, WindowUrl};
use tauri::{CustomMenuItem, SystemTrayMenu, SystemTrayMenuItem};
use tauri_plugin_store::StoreBuilder;
use tauri_plugin_window_state::{AppHandleExt, StateFlags};

type UID = String;

// The store plugin places this in a data_dir, which is located at:
//  ~/.local/share/org.coolercontrol.coolercontrol/coolercontrol-ui.conf
const CONFIG_FILE: &str = "coolercontrol-ui.conf";
const CONFIG_START_IN_TRAY: &str = "start_in_tray";
const CONFIG_STARTUP_DELAY: &str = "startup_delay";

#[command]
async fn start_in_tray_enable(app_handle: AppHandle) {
    let mut store = StoreBuilder::new(app_handle, CONFIG_FILE.parse().unwrap()).build();
    let _ = store.load();
    let _ = store.insert(CONFIG_START_IN_TRAY.to_string(), json!(true));
    let _ = store.save();
}

#[command]
async fn start_in_tray_disable(app_handle: AppHandle) {
    let mut store = StoreBuilder::new(app_handle, CONFIG_FILE.parse().unwrap()).build();
    let _ = store.load();
    let _ = store.insert(CONFIG_START_IN_TRAY.to_string(), json!(false));
    let _ = store.save();
}

#[command]
async fn save_window_state(app_handle: AppHandle) {
    app_handle
        .save_window_state(StateFlags::all())
        .unwrap_or_else(|e| {
            println!("Failed to save window state: {}", e);
        });
}

#[command]
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
    recreate_mode_menu_items(&app_handle, &active_mode_lock, &modes_state_lock);
    Ok(())
}

#[command]
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
    recreate_mode_menu_items(&app_handle, &active_mode_lock, &modes_state_lock);
    Ok(())
}

#[command]
async fn get_startup_delay(app_handle: AppHandle) -> Result<u64, String> {
    let mut store = StoreBuilder::new(app_handle, CONFIG_FILE.parse().unwrap()).build();
    let _ = store.load();
    store
        .get(CONFIG_STARTUP_DELAY)
        .unwrap_or(&json!(0))
        .as_u64()
        .ok_or_else(|| "Startup delay is not a number".to_string())
}

#[command]
async fn set_startup_delay(delay: u64, app_handle: AppHandle) {
    let mut store = StoreBuilder::new(app_handle, CONFIG_FILE.parse().unwrap()).build();
    let _ = store.load();
    let _ = store.insert(CONFIG_STARTUP_DELAY.to_string(), json!(delay));
    let _ = store.save();
}

fn recreate_mode_menu_items(
    app_handle: &AppHandle,
    active_mode_lock: &MutexGuard<Option<UID>>,
    modes_state_lock: &MutexGuard<Vec<ModeTauri>>,
) {
    let modes_tray_menu = if modes_state_lock.len() > 0 {
        modes_state_lock
            .iter()
            .fold(create_starting_sys_tray_menu(), |menu, mode| {
                let mode_menu_item = if active_mode_lock
                    .as_ref()
                    .map_or(false, |uid| uid == &mode.uid)
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
    // Disable DMA Rendering by default for webkit2gtk (See #229)
    // Many distros have patched the official package and disabled this by default for NVIDIA GPUs,
    if env::var("WEBKIT_FORCE_DMABUF_RENDERER").is_err() && has_nvidia() {
        env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
    }
    if env::var("WEBKIT_FORCE_COMPOSITING_MODE").is_err() && is_app_image() {
        // Needed so that the app image works on most all systems (system library dependant)
        env::set_var("WEBKIT_DISABLE_COMPOSITING_MODE", "1");
    }
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
            println!("A second instance was attempted to be started while this one is already running: {} {argv:?}, {cwd}", app.package_info().name);
            println!("Showing the window of this already running instance if hidden.");
            app.get_window("main").unwrap()
                .show().unwrap();
            // This doesn't appear to do anything in 1.x, perhaps it's meant for 2.x:
            app.emit_all("single-instance", Payload { args: argv, cwd })
                .unwrap();
        }))
        .invoke_handler(tauri::generate_handler![
            start_in_tray_enable,
            start_in_tray_disable,
            save_window_state,
            set_modes,
            set_active_mode,
            get_startup_delay,
            set_startup_delay,
        ])
        .setup(|app| {
            if env::args_os().count() > 1 {
                let Ok(matches) = app.get_cli_matches() else {
                    println!("ERROR: Unknown argument. Use the --help option to list the available arguments.");
                    std::process::exit(1);
                };
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
            let mut store = StoreBuilder::new(app.handle(), CONFIG_FILE.parse()?).build();
            let _ = store.load();
            let delay = store
                .get(CONFIG_STARTUP_DELAY)
                .unwrap_or(&json!(0))
                .as_u64()
                .unwrap_or(0);
            if delay > 0 {
                println!("Delaying startup by {delay} seconds");
                sleep(Duration::from_secs(delay));
            }
            let start_in_tray = store
                .get("start_in_tray")
                .unwrap_or(&json!(false))
                .as_bool()
                .unwrap_or(false);
            let window = app.get_window("main").unwrap();
            if start_in_tray {
                println!("Start in Tray setting is enabled, hiding window. Use the tray icon to show the window.");
                window.hide().unwrap();
            } else {
                window.show().unwrap();
            }
            Ok(())
        })
        .run(create_context(port))
        .expect("error while running tauri application");
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
    let url = format!("http://localhost:{port}").parse().unwrap();
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
                                &app.app_handle(),
                                &active_mode_lock,
                                &modes_state_lock,
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
