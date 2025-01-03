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

use crate::tray::recreate_mode_menu_items;
use crate::UID;
use std::sync::{Arc, Mutex};
use tauri::{command, AppHandle};

#[command]
pub async fn set_modes(
    modes: Vec<ModeTauri>,
    modes_state: tauri::State<'_, Arc<ModesState>>,
    app_handle: AppHandle,
) -> Result<(), String> {
    let mut modes_state_lock = modes_state.modes.lock().expect("Modes State is poisoned");
    modes_state_lock.clear();
    modes_state_lock.extend(modes);
    let active_modes_lock = modes_state
        .active_modes
        .lock()
        .expect("Active Mode State is poisoned");
    recreate_mode_menu_items(&app_handle, &active_modes_lock, &modes_state_lock);
    Ok(())
}

#[command]
pub async fn set_active_modes(
    mut active_mode_uids: Vec<UID>,
    modes_state: tauri::State<'_, Arc<ModesState>>,
    app_handle: AppHandle,
) -> Result<(), String> {
    let mut active_modes_lock = modes_state
        .active_modes
        .lock()
        .expect("Active Mode State is poisoned");
    active_modes_lock.clear();
    active_modes_lock.append(&mut active_mode_uids);
    let modes_state_lock = modes_state.modes.lock().expect("Modes State is poisoned");
    recreate_mode_menu_items(&app_handle, &active_modes_lock, &modes_state_lock);
    Ok(())
}

#[derive(Clone, serde::Serialize)]
pub struct EventPayload {
    pub(crate) active_mode_uid: UID,
}

#[derive(Default)]
pub struct ModesState {
    pub(crate) active_modes: Mutex<Vec<UID>>,
    pub(crate) modes: Mutex<Vec<ModeTauri>>,
}

#[derive(Default, serde::Deserialize)]
pub struct ModeTauri {
    pub(crate) uid: UID,
    pub(crate) name: String,
}
