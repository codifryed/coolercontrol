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
use std::error::Error;
use std::process::Command;
use std::sync::{Mutex, MutexGuard};
use std::thread::sleep;
use std::time::Duration;

use serde_json::json;
use tauri::menu::{
    AboutMetadata, AboutMetadataBuilder, CheckMenuItemBuilder, IconMenuItemBuilder, MenuBuilder,
    MenuEvent, MenuItemBuilder, SubmenuBuilder,
};
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconEvent};
use tauri::utils::config::FrontendDist;
use tauri::{command, App, AppHandle, Context, Emitter, Manager, Wry};
use tauri_plugin_cli::CliExt;
use tauri_plugin_store::StoreBuilder;
use tauri_plugin_window_state::{AppHandleExt, StateFlags};

use crate::port_finder::Port;

mod port_finder;

type UID = String;

// The store plugin places this in a data_dir, which is located at:
//  ~/.local/share/org.coolercontrol.CoolerControl/coolercontrol-ui.conf
const CONFIG_FILE: &str = "coolercontrol-ui.conf";
const CONFIG_START_IN_TRAY: &str = "start_in_tray";
const CONFIG_STARTUP_DELAY: &str = "startup_delay";
const SYSTEM_TRAY_ID: &str = "coolercontrol-system-tray";
const MAIN_WINDOW_ID: &str = "main";

#[command]
async fn start_in_tray_enable(app_handle: AppHandle) {
    let mut store = StoreBuilder::new(CONFIG_FILE).build(app_handle);
    store.load().expect("Failed to load store");
    store
        .insert(CONFIG_START_IN_TRAY.to_string(), json!(true))
        .expect("Failed to insert start_in_tray");
    store.save().expect("Failed to save store");
}

#[command]
async fn start_in_tray_disable(app_handle: AppHandle) {
    let mut store = StoreBuilder::new(CONFIG_FILE).build(app_handle);
    store.load().expect("Failed to load store");
    store
        .insert(CONFIG_START_IN_TRAY.to_string(), json!(false))
        .expect("Failed to insert start_in_tray");
    store.save().expect("Failed to save store");
}

#[command]
async fn get_start_in_tray(app_handle: AppHandle) -> Result<bool, String> {
    let mut store = StoreBuilder::new(CONFIG_FILE).build(app_handle);
    store.load().expect("Failed to load store");
    store
        .get(CONFIG_START_IN_TRAY)
        .unwrap_or(&json!(false))
        .as_bool()
        .ok_or_else(|| "Start in Tray is not a boolean".to_string())
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
    let mut store = StoreBuilder::new(CONFIG_FILE).build(app_handle);
    store.load().expect("Failed to load store");
    store
        .get(CONFIG_STARTUP_DELAY)
        .unwrap_or(&json!(0))
        .as_u64()
        .ok_or_else(|| "Startup delay is not a number".to_string())
}

#[command]
async fn set_startup_delay(delay: u64, app_handle: AppHandle) {
    let mut store = StoreBuilder::new(CONFIG_FILE).build(app_handle);
    store.load().expect("Failed to load store");
    store
        .insert(CONFIG_STARTUP_DELAY.to_string(), json!(delay))
        .expect("Failed to insert startup_delay");
    store.save().expect("Failed to save store");
}

fn main() {
    handle_dma_rendering_for_nvidia_gpus();
    let Some(port) = port_finder::find_free_port() else {
        println!("ERROR: No free port on localhost found, exiting.");
        std::process::exit(1);
    };
    tauri::Builder::default()
        .manage(ModesState::default())
        .plugin(tauri_plugin_cli::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_localhost::Builder::new(port).build())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .plugin(tauri_plugin_single_instance::init(|app, argv, cwd| {
            println!("A second instance was attempted to be started while this one is already running: {} {argv:?}, {cwd}", app.package_info().name);
            println!("Showing the window of this already running instance if hidden.");
            let webview = app.get_webview_window(MAIN_WINDOW_ID)
                .expect("Failed to get main window");
            webview.show().unwrap();
            webview.set_focus().unwrap();
            // This doesn't appear to do anything in 1.x, perhaps it's meant for 2.x:
            app.emit("single-instance", Payload { args: argv, cwd })
                .expect("Failed to emit single-instance event");
        }))
        .invoke_handler(tauri::generate_handler![
            start_in_tray_enable,
            start_in_tray_disable,
            get_start_in_tray,
            save_window_state,
            set_modes,
            set_active_mode,
            get_startup_delay,
            set_startup_delay,
        ])
        .setup(move |app: &mut App| {
            set_gtk_prgname(app);
            handle_cli_arguments(app);
            setup_system_tray(app)?;
            setup_config_store(app);
            Ok(())
        })
        .run(generate_localhost_context(port))
        .expect("error while running tauri application");
}

/// This function handles our special context localhost logic in the production build.
/// It not only allows us to use a localhost server for the Web UI assets, but also enables
/// http access to localhost in general, enabling us to access the locally running coolercontrold
/// daemon.
fn generate_localhost_context(port: Port) -> Context {
    let mut context = tauri::generate_context!();
    let url = format!("http://localhost:{port}").parse().unwrap();
    context.config_mut().build.frontend_dist = Some(FrontendDist::Url(url));
    context
}

fn handle_dma_rendering_for_nvidia_gpus() {
    // Disable DMA Rendering by default for webkit2gtk (See #229)
    // Many distros have patched the official package and disabled this by default for NVIDIA GPUs,
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

/// This is needed for the GTK3 application to be displayed with the correct top-level icon
/// under different Wayland compositors. (i.e. KDE Wayland)
/// https://sigxcpu.org/con/GTK__and_the_application_id.html
fn set_gtk_prgname(app: &mut App) {
    glib::set_prgname(Some(app.config().identifier.clone()));
}

fn handle_cli_arguments(app: &mut App) {
    if env::args_os().count() > 1 {
        let Ok(matches) = app.cli().matches() else {
            println!(
                "ERROR: Unknown argument. Use the --help option to list the available arguments."
            );
            std::process::exit(1);
        };
        if matches.args.contains_key("help") {
            println!(
                "
CoolerControl GUI Desktop Application v{}

OPTIONS:
-h, --help       Print help information (this)
-V, --version    Print version information",
                app.package_info().version
            );
            std::process::exit(0);
        } else if matches.args.contains_key("version")
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
}

fn setup_system_tray(app: &mut App) -> Result<(), Box<dyn Error>> {
    let tray_menu_builder = create_starting_tray_menu(app.handle());
    let tray_menu = add_final_tray_menu_items(app.handle(), tray_menu_builder).build()?;
    // The TrayIcon is created by tauri with the icon already:
    let tray_icon = app
        .handle()
        .tray_by_id(SYSTEM_TRAY_ID)
        .expect("Failed to get tray icon");
    tray_icon.set_menu(Some(tray_menu))?;
    tray_icon.on_tray_icon_event(handle_sys_tray_event);
    tray_icon.on_menu_event(handle_tray_menu_event);
    Ok(())
}

fn create_starting_tray_menu(app_handle: &AppHandle) -> MenuBuilder<Wry, AppHandle<Wry>> {
    let tray_menu_item_cc = IconMenuItemBuilder::with_id("cc", "CoolerControl")
        // Using an icon also creates an icon column in the tray menu for all menu items.
        // Perhaps that will change in the future and we can use both checked and Icons.
        // .icon(Image::from_path("icons/icon.png").unwrap()))
        .icon(
            app_handle
                .default_window_icon()
                .cloned()
                .expect("Failed to get default icon"),
        )
        .enabled(false)
        .build(app_handle)
        .expect("Failed to build menu item");
    MenuBuilder::new(app_handle)
        .item(&tray_menu_item_cc)
        .separator()
}

fn recreate_mode_menu_items(
    app_handle: &AppHandle,
    active_mode_lock: &MutexGuard<Option<UID>>,
    modes_state_lock: &MutexGuard<Vec<ModeTauri>>,
) {
    let modes_submenu_builder = SubmenuBuilder::with_id(app_handle, "modes", "Modes");
    let modes_submenu = if modes_state_lock.len() > 0 {
        modes_state_lock
            .iter()
            .fold(modes_submenu_builder, |menu, mode| {
                let mode_is_active = active_mode_lock
                    .as_ref()
                    .map_or(false, |uid| uid == &mode.uid);
                let mode_menu_item =
                    CheckMenuItemBuilder::with_id(mode.uid.clone(), mode.name.clone())
                        .checked(mode_is_active)
                        .build(app_handle)
                        .expect("Failed to build menu item");
                menu.item(&mode_menu_item)
            })
    } else {
        modes_submenu_builder.enabled(false)
    }
    .build()
    .expect("Failed to build submenu");
    let menu_with_modes = create_starting_tray_menu(app_handle).item(&modes_submenu);
    let tray_menu = add_final_tray_menu_items(app_handle, menu_with_modes)
        .build()
        .expect("Failed to build tray menu with modes");
    app_handle
        .tray_by_id(SYSTEM_TRAY_ID)
        .expect("Failed to get tray icon")
        .set_menu(Some(tray_menu))
        .expect("Failed to set new tray menu");
}

fn add_final_tray_menu_items<'m>(
    app_handle: &AppHandle,
    tray_menu: MenuBuilder<'m, Wry, AppHandle<Wry>>,
) -> MenuBuilder<'m, Wry, AppHandle<Wry>> {
    let tray_menu_item_show = MenuItemBuilder::with_id("show", "Show/Hide")
        .build(app_handle)
        .expect("Failed to build menu item");
    let tray_menu_item_quit = MenuItemBuilder::with_id("quit", "Quit")
        .build(app_handle)
        .expect("Failed to build menu item");
    tray_menu
        .separator()
        .about(create_metadata(app_handle))
        .item(&tray_menu_item_show)
        .item(&tray_menu_item_quit)
}

fn create_metadata(app_handle: &AppHandle) -> Option<AboutMetadata> {
    let metadata = AboutMetadataBuilder::new()
        .name(Some("CoolerControl".to_string()))
        .icon(Some(
            app_handle
                .default_window_icon()
                .cloned()
                .expect("Failed to get default icon"),
        ))
        .authors(Some(vec![
            "Guy Boldon and project contributors https://gitlab.com/coolercontrol/coolercontrol/-/graphs/main?ref_type=heads".to_string(),
        ]))
        .license(Some(
            "Copyright (c) 2021-2024  Guy Boldon, Eren Simsek and contributors

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, either version 3 of the License, or
any later version.

This program is distributed in the hope that it will be useful,
but WITHOUT ANY WARRANTY; without even the implied warranty of
MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
GNU General Public License for more details.

You should have received a copy of the GNU General Public License
along with this program.  If not, see <https://www.gnu.org/licenses/>."
                .to_string(),
        ))
        .version(app_handle.config().version.clone())
        .website(Some("https://gitlab.com/coolercontrol/coolercontrol"))
        .website_label(Some("GitLab Project Page"))
        .comments(Some(
            "Monitor and control your cooling devices".to_string(),
        ))
        .build();
    Some(metadata)
}

/// These events are not currently supported on Linux, but will leave for possible future support:
fn handle_sys_tray_event(tray_icon: &TrayIcon, tray_icon_event: TrayIconEvent) {
    if let TrayIconEvent::Click {
        button: MouseButton::Left,
        button_state: MouseButtonState::Up,
        ..
    } = tray_icon_event
    {
        let Some(window) = tray_icon.app_handle().get_webview_window(MAIN_WINDOW_ID) else {
            return;
        };
        window.show().expect("Failed to show window");
        window.set_focus().expect("Failed to set focus");
    }
}

fn handle_tray_menu_event(app: &AppHandle, event: MenuEvent) {
    match event.id().as_ref() {
        "quit" => {
            app.exit(0);
        }
        "show" => {
            let Some(window) = app.get_webview_window(MAIN_WINDOW_ID) else {
                return;
            };
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
        id => {
            if id.len() == 36 {
                // Mode UUID
                // println!("System Tray Menu Item Click with Mode ID: {}", id);
                let modes_state = app.state::<ModesState>();
                let active_mode_lock = modes_state
                    .active_mode
                    .lock()
                    .expect("Active Mode State is poisoned");
                if let Some(active_mode) = active_mode_lock.as_ref() {
                    if active_mode == id {
                        let modes_state_lock =
                            modes_state.modes.lock().expect("Modes State is poisoned");
                        // this sets the menu item back to selected
                        recreate_mode_menu_items(
                            app.app_handle(),
                            &active_mode_lock,
                            &modes_state_lock,
                        );
                        return;
                    }
                }
                app.emit(
                    "mode-activated",
                    EventPayload {
                        active_mode_uid: id.to_owned(),
                    },
                )
                .unwrap();
            }
        }
    }
}

fn setup_config_store(app: &mut App) {
    let mut store = StoreBuilder::new(CONFIG_FILE).build(app.handle().clone());
    if store.load().is_err() {
        println!("{CONFIG_FILE} not found, creating a new one.");
        store.save().expect("Failed to save store"); // writes an empty new store
        store.load().expect("Failed to load store");
    }
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
        .get(CONFIG_START_IN_TRAY)
        .unwrap_or(&json!(false))
        .as_bool()
        .unwrap_or(false);
    let window = app.get_webview_window(MAIN_WINDOW_ID).unwrap();
    if start_in_tray {
        println!("Start in Tray setting is enabled, hiding window. Use the tray icon to show the window.");
        window.hide().unwrap();
    } else {
        window.show().unwrap();
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
