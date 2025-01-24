/*
 * CoolerControl - monitor and control your cooling and other devices
 * Copyright (c) 2021-2025  Guy Boldon, Eren Simsek and contributors
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

use std::sync::atomic::{AtomicBool, Ordering};
use tauri::command;

#[command]
pub fn is_first(first_run_state: tauri::State<'_, FirstRunState>) -> bool {
    let is_first = first_run_state.is_first.load(Ordering::Relaxed);
    if is_first {
        first_run_state.is_first.store(false, Ordering::Relaxed);
    }
    is_first
}

pub struct FirstRunState {
    is_first: AtomicBool,
}

impl Default for FirstRunState {
    fn default() -> Self {
        Self {
            is_first: AtomicBool::new(true),
        }
    }
}
