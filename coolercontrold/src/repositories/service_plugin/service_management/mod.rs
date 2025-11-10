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
 */

use std::env;
use std::path::PathBuf;

pub mod manager;
mod openrc;
mod systemd;

pub type ServiceId = String;

pub trait ServiceIdExt {
    fn to_service_name(&self) -> String;
    fn to_description(&self) -> String;
}

impl ServiceIdExt for ServiceId {
    fn to_service_name(&self) -> String {
        format!("cc-plugin-{self}")
    }

    fn to_description(&self) -> String {
        format!("CoolerControl Plugin {self}")
    }
}

fn find_on_path(executable: &str) -> Option<PathBuf> {
    env::var_os("PATH").and_then(|paths| {
        env::split_paths(&paths)
            .filter_map(|dir| {
                let full_path = dir.join(executable);
                if full_path.is_file() {
                    Some(full_path)
                } else {
                    None
                }
            })
            .next()
    })
}
