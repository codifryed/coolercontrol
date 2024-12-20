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

use notify_rust::{Hint, Notification};
use tauri::command;

const APP_ID: &str = "org.coolercontrol.CoolerControl";

#[command]
pub async fn send_notification(
    title: &str,
    message: &str,
    icon_name: Option<&str>,
) -> Result<(), String> {
    // default to our own icon (generally better that than no icon)
    // standard set: https://specifications.freedesktop.org/icon-naming-spec/0.8.90/
    let icon = icon_name.unwrap_or("coolercontrol");
    Notification::new()
        .appname("CoolerControl")
        .icon(icon)
        .hint(Hint::Resident(true))
        .hint(Hint::DesktopEntry(APP_ID.to_string()))
        .summary(title)
        .body(message)
        .show_async()
        .await
        .map(|_| ())
        .map_err(|err| err.to_string())
}
