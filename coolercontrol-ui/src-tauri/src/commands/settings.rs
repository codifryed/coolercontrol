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
use serde_json::json;
use std::error::Error;
use std::thread::sleep;
use std::time::Duration;
use tauri::{command, App, AppHandle, Manager};
use tauri_plugin_store::StoreBuilder;
use tauri_plugin_window_state::{AppHandleExt, StateFlags};

// The store plugin places this in a data_dir, which is located at:
//  ~/.local/share/org.coolercontrol.CoolerControl/coolercontrol-ui.conf
const CONFIG_FILE: &str = "coolercontrol-ui.conf";
const CONFIG_START_IN_TRAY: &str = "start_in_tray";
const CONFIG_STARTUP_DELAY: &str = "startup_delay";

#[command]
pub async fn start_in_tray_enable(app_handle: AppHandle) {
    let Ok(store) = StoreBuilder::new(&app_handle, CONFIG_FILE).build() else {
        return;
    };
    store.set(CONFIG_START_IN_TRAY.to_string(), json!(true));
    store.save().expect("Failed to save store");
}

#[command]
pub async fn start_in_tray_disable(app_handle: AppHandle) {
    let Ok(store) = StoreBuilder::new(&app_handle, CONFIG_FILE).build() else {
        return;
    };
    store.set(CONFIG_START_IN_TRAY.to_string(), json!(false));
    store.save().expect("Failed to save store");
}

#[command]
pub async fn get_start_in_tray(app_handle: AppHandle) -> Result<bool, String> {
    let Ok(store) = StoreBuilder::new(&app_handle, CONFIG_FILE).build() else {
        return Err("Store not found.".to_string());
    };
    store
        .get(CONFIG_START_IN_TRAY)
        .unwrap_or(json!(false))
        .as_bool()
        .ok_or_else(|| "Start in Tray is not a boolean".to_string())
}

#[command]
pub async fn save_window_state(app_handle: AppHandle) {
    app_handle
        .save_window_state(StateFlags::all())
        .unwrap_or_else(|e| {
            println!("Failed to save window state: {e}");
        });
}

#[command]
pub async fn get_startup_delay(app_handle: AppHandle) -> Result<u64, String> {
    let Ok(store) = StoreBuilder::new(&app_handle, CONFIG_FILE).build() else {
        return Err("Store not found".to_string());
    };
    store
        .get(CONFIG_STARTUP_DELAY)
        .unwrap_or(json!(0))
        .as_u64()
        .ok_or_else(|| "Startup delay is not a number".to_string())
}

#[command]
pub async fn set_startup_delay(delay: u64, app_handle: AppHandle) {
    let Ok(store) = StoreBuilder::new(&app_handle, CONFIG_FILE).build() else {
        return;
    };
    store.set(CONFIG_STARTUP_DELAY.to_string(), json!(delay));
    store.save().expect("Failed to save store");
}

pub fn setup_config_store(app: &mut App) -> Result<(), Box<dyn Error>> {
    let store = StoreBuilder::new(app.handle(), CONFIG_FILE).build()?;
    let delay = store
        .get(CONFIG_STARTUP_DELAY)
        .unwrap_or(json!(0))
        .as_u64()
        .unwrap_or(0);
    if delay > 0 {
        println!("Delaying startup by {delay} seconds");
        sleep(Duration::from_secs(delay));
    }
    let start_in_tray = store
        .get(CONFIG_START_IN_TRAY)
        .unwrap_or(json!(false))
        .as_bool()
        .unwrap_or(false);
    let window = app.get_webview_window(MAIN_WINDOW_ID).unwrap();
    if start_in_tray {
        println!("Start in Tray setting is enabled, hiding window. Use the tray icon to show the window.");
        window.hide().unwrap();
    } else {
        window.show().unwrap();
    }
    Ok(())
}
