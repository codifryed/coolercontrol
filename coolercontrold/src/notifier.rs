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

use anyhow::Result;
use image::ImageReader;
use std::collections::HashMap;
use std::io::Cursor;
use zbus::names::WellKnownName;
use zbus::zvariant::{self, Structure};
use zbus::Connection;

const NOTIFICATION_DEFAULT_BUS_NAME: &str = "org.freedesktop.Notifications";
const NOTIFICATION_OBJECTPATH: &str = "/org/freedesktop/Notifications";
const NOTIFICATION_INTERFACE: &str = "org.freedesktop.Notifications";
const NOTIFICATION_METHOD: &str = "Notify";
const APP_ID: &str = "org.coolercontrol.CoolerControl";
const APP_NAME: &str = "CoolerControl";
const IMAGE_PNG_ALERT_TRIGGERED: &[u8] = include_bytes!("../resources/alert-triggered.png");
const IMAGE_PNG_ALERT_RESOLVED: &[u8] = include_bytes!("../resources/alert-resolved.png");
const IMAGE_PNG_ALERT_ERROR: &[u8] = include_bytes!("../resources/alert-error.png");
const IMAGE_PNG_INFO: &[u8] = include_bytes!("../resources/information.png");
const IMAGE_PNG_SHUTDOWN: &[u8] = include_bytes!("../resources/shutdown.png");

/// Sends a desktop notification to the current user's dbus session.
/// This is used by the daemon itself to be able to send notifications to user sessions.
/// e.g. when an alert fires.
///
/// Based on the Freedesktop spec: <https://specifications.freedesktop.org/notification/latest-single>
pub async fn notify(
    summary: &str,
    body: &str,
    icon: u8,
    audio: bool,
    urgency: &str,
    debug: bool,
) -> Result<()> {
    let replace_id: u32 = 0; // 0 = new notification
    let actions: Vec<String> = vec![];
    let mut hints: HashMap<&str, zvariant::Value<'_>> = HashMap::new();
    // This seems to break Gnome DBus Notifications (Notifications won't show or close quickly)
    //hints.insert("desktop-entry", APP_ID.into());
    hints.insert("resident", true.into());
    if icon == 0 || icon > 5 {
        hints.insert("image-path", APP_ID.into());
    } else {
        hints.insert("image-data", image_data(icon).into());
    }
    if audio {
        hints.insert("sound-name", "alarm-clock-elapsed".into());
    }
    hints.insert("urgency", urgency.as_bytes().into()); // 0 = low, 1 = normal, 2 = critical
    let expire_timeout: i32 = -1; // -1 lets server decide, 0 never expires

    let conn = Connection::session().await?;
    let response = conn
        .call_method(
            Some(WellKnownName::from_static_str(NOTIFICATION_DEFAULT_BUS_NAME).unwrap()),
            NOTIFICATION_OBJECTPATH,
            Some(NOTIFICATION_INTERFACE),
            NOTIFICATION_METHOD,
            &(
                APP_NAME,
                replace_id,
                // Gnome uses this for the app icon, instead of desktop-entry help.
                APP_ID, // initial message icon.
                summary,
                body,
                &actions,
                hints,
                expire_timeout,
            ),
        )
        .await?;
    if debug {
        println!("DBus notification response: {response:?}");
    }
    Ok(())
}

#[allow(clippy::cast_possible_wrap)]
fn image_data(icon: u8) -> Structure<'static> {
    let image_binary_data = match icon {
        1 => IMAGE_PNG_ALERT_TRIGGERED,
        2 => IMAGE_PNG_ALERT_RESOLVED,
        3 => IMAGE_PNG_ALERT_ERROR,
        5 => IMAGE_PNG_SHUTDOWN,
        _ => IMAGE_PNG_INFO,
    };
    let image_data: (i32, i32, i32, bool, i32, i32, Vec<u8>) = {
        let img = ImageReader::new(Cursor::new(image_binary_data))
            .with_guessed_format()
            .expect("Failed to read embedded PNG")
            .decode()
            .expect("Failed to decode embedded PNG");
        let rgba = img.to_rgba8();
        let width = rgba.width() as i32;
        let height = rgba.height() as i32;
        let channels: i32 = 4; // RGBA
        let bits_per_sample: i32 = 8;
        let has_alpha = true;
        let rowstride = width * channels;
        let data = rgba.into_raw();
        (
            width,
            height,
            rowstride,
            has_alpha,
            bits_per_sample,
            channels,
            data,
        )
    };
    Structure::from((
        image_data.0,
        image_data.1,
        image_data.2,
        image_data.3,
        image_data.4,
        image_data.5,
        image_data.6.as_slice(),
    ))
}
