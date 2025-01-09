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

use crate::commands::daemon_state::DaemonState;
use crate::commands::USER_ID;
use crate::tray::recreate_mode_menu_items;
use crate::UID;
use serde::Deserialize;
use serde_json::json;
use std::ops::Not;
use std::sync::{Arc, Mutex};
use tauri::{command, AppHandle, Manager};

#[command]
pub async fn set_modes(
    modes: Vec<ModeTauri>,
    modes_state: tauri::State<'_, Arc<ModesState>>,
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
pub async fn set_active_mode(
    active_mode_uid: UID,
    modes_state: tauri::State<'_, Arc<ModesState>>,
    app_handle: AppHandle,
) -> Result<(), String> {
    let mut active_mode_lock = modes_state
        .active_mode
        .lock()
        .expect("Active Mode State is poisoned");
    active_mode_lock.replace(active_mode_uid);
    let modes_state_lock = modes_state.modes.lock().expect("Modes State is poisoned");
    recreate_mode_menu_items(&app_handle, &active_mode_lock, &modes_state_lock);
    Ok(())
}

pub async fn activate_mode(mode_uid: String, app: AppHandle) {
    let daemon_state = app.state::<Arc<DaemonState>>();
    let activate_mode_url = {
        let address = daemon_state.address.lock().unwrap();
        format!("{address}modes-active/{mode_uid}")
    };
    let login_url = {
        let address = daemon_state.address.lock().unwrap();
        format!("{address}login")
    };
    let client = daemon_state.client.read().await;
    match client
        .post(activate_mode_url.clone())
        .json(&json!({}))
        .send()
        .await
    {
        Ok(response) => {
            if response.status().is_success().not() {
                // if not successful, let's make sure our login cookie is valid:
                let passwd = daemon_state.passwd.read().await;
                let pass = String::from_utf8_lossy(passwd.as_slice());
                match client
                    .post(login_url)
                    .basic_auth(USER_ID, Some(pass))
                    .send()
                    .await
                {
                    Ok(response) => {
                        if response.status().is_success() {
                            // if logged in, retry activating mode
                            match client.post(activate_mode_url).json(&json!({})).send().await {
                                Ok(response) => {
                                    if response.status().is_success().not() {
                                        println!("Activate Mode Error: {response:?}");
                                        recreate_mode_menu(app.app_handle());
                                    }
                                }
                                Err(err) => {
                                    println!("Activate Mode Error: {err}");
                                    recreate_mode_menu(app.app_handle());
                                }
                            }
                        }
                    }
                    Err(err) => {
                        println!("Login Error: {err}");
                        recreate_mode_menu(app.app_handle());
                    }
                }
            }
        }
        Err(err) => {
            println!("Activate Mode Error: {err}");
            recreate_mode_menu(app.app_handle());
        }
    }
}

fn recreate_mode_menu(app_handle: &AppHandle) {
    let modes_state = app_handle.state::<Arc<ModesState>>();
    let active_mode_lock = modes_state
        .active_mode
        .lock()
        .expect("Active Mode State is poisoned");
    let modes_state_lock = modes_state.modes.lock().expect("Modes State is poisoned");
    recreate_mode_menu_items(app_handle, &active_mode_lock, &modes_state_lock);
}

#[derive(Default)]
pub struct ModesState {
    pub active_mode: Mutex<Option<UID>>,
    pub modes: Mutex<Vec<ModeTauri>>,
}

#[derive(Default, Deserialize)]
pub struct ModeTauri {
    pub uid: UID,
    pub name: String,
}
