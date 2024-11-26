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
 */

use crate::api::{auth, base, custom_sensors, functions, modes, profiles, settings, status};
use crate::api::{devices, AppState};
use aide::axum::routing::{delete_with, get, get_with, patch_with, post_with, put_with};
use aide::axum::ApiRouter;

// Note: using `#[debug_handler]` on the handler functions themselves is sometimes very helpful.

#[allow(clippy::too_many_lines)]
pub fn init(app_state: AppState) -> ApiRouter {
    let router = ApiRouter::new()
        .api_route(
            "/handshake",
            get_with(base::handshake, |o| {
                o.summary("Handshake")
                    .description("A simple endpoint to verify the connection")
                    .tag("base")
            }),
        )
        .api_route(
            "/shutdown",
            post_with(base::shutdown, |o| {
                o.summary("Shutdown Daemon")
                    .description(
                        "Sends a cancellation signal to shut the daemon down. \
                        When the daemon is running as a systemd or initrc service, \
                        it is automatically restarted.",
                    )
                    .tag("base")
                    .security_requirement("CookieAuth")
            }),
        )
        .nest_service("/", base::web_app_service())
        ///////////////////////////////////////////////////////////////////////////////////////////
        .api_route(
            "/login",
            post_with(auth::login, |o| {
                o.summary("Login")
                    .description("The endpoint used to create a login session.")
                    .tag("auth")
                    .security_requirement("BasicAuth")
            }),
        )
        .api_route(
            "/verify-session",
            post_with(auth::verify_session, |o| {
                o.summary("Verify Session Auth")
                    .description("Verifies that the current session is still authenticated")
                    .tag("auth")
                    .security_requirement("CookieAuth")
            }),
        )
        .api_route(
            "/set-passwd",
            post_with(auth::set_passwd, |o| {
                o.summary("Set Admin Password")
                    .description("Stores a new Admin password.")
                    .tag("auth")
                    .security_requirement("CookieAuth")
                    .security_requirement("BasicAuth")
            }),
        )
        .api_route(
            "/logout",
            post_with(auth::logout, |o| {
                o.summary("Set Admin Password")
                    .description("Stores a new Admin password.")
                    .tag("auth")
            }),
        )
        ///////////////////////////////////////////////////////////////////////////////////////////
        .api_route(
            "/thinkpad-fan-control",
            put_with(devices::thinkpad_fan_control_modify, |o| {
                o.summary("ThinkPad Fan Control")
                    .description(
                        "Enables/Disabled Fan Control for ThinkPads, if acpi driver is present.",
                    )
                    .tag("device")
                    .security_requirement("CookieAuth")
            }),
        )
        .api_route(
            "/devices",
            get_with(devices::devices_get, |o| {
                o.summary("All Devices")
                    .description(
                        "Returns a list of all detected devices and their associated information.",
                    )
                    .tag("device")
            }),
        )
        .api_route(
            "/devices/:device_uid/settings",
            get_with(devices::device_settings_get, |o| {
                o.summary("All Device Settings")
                    .description(
                        "Returns all the currently applied settings for the given device. \
                        It returns the Config Settings model, which includes all possibilities \
                        for each channel.",
                    )
                    .tag("device")
            }),
        )
        .api_route(
            "/devices/:device_uid/settings/:channel_name/manual",
            put_with(devices::device_setting_manual_modify, |o| {
                o.summary("Device Channel Manual")
                    .description("Applies a fan duty to a specific device channel.")
                    .tag("device")
                    .security_requirement("CookieAuth")
            }),
        )
        .api_route(
            "/devices/:device_uid/settings/:channel_name/profile",
            put_with(devices::device_setting_profile_modify, |o| {
                o.summary("Device Channel Profile")
                    .description("Applies a Profile to a specific device channel.")
                    .tag("device")
                    .security_requirement("CookieAuth")
            }),
        )
        .api_route(
            "/devices/:device_uid/settings/:channel_name/lcd",
            put_with(devices::device_setting_lcd_modify, |o| {
                o.summary("Device Channel LCD")
                    .description("Applies LCD Settings to a specific device channel.")
                    .tag("device")
                    .security_requirement("CookieAuth")
            }),
        )
        .api_route(
            "/devices/:device_uid/settings/:channel_name/lcd/images",
            get_with(devices::get_device_lcd_image, |o| {
                o.summary("Retrieve Device Channel LCD")
                    .description("Retrieves the currently applied LCD Image file.")
                    .tag("device")
            })
            .post_with(devices::process_device_lcd_images, |o| {
                o.summary("Process Device Channel LCD Image")
                    .description("This takes and image file and processes it for optimal \
                    use by the specified device channel. This is useful for a UI Preview \
                    and is used internally before applying the image to the device.")
                    .tag("device")
                    .security_requirement("CookieAuth")
            })
            .put_with(devices::update_device_setting_lcd_image, |o| {
                o.summary("Update Device Channel LCD Settings")
                    .description("Used to apply LCD settings that contain images.")
                    .tag("device")
                    .security_requirement("CookieAuth")
            }),
        )
        .api_route(
            "/devices/:device_uid/settings/:channel_name/lighting",
            put_with(devices::device_setting_lighting_modify, |o| {
                o.summary("Device Channel Lighting")
                    .description("Applies Lighting Settings (RGB) to a specific device channel.")
                    .tag("device")
                    .security_requirement("CookieAuth")
            }),
        )
        .api_route(
            "/devices/:device_uid/settings/:channel_name/pwm",
            put_with(devices::device_setting_pwm_mode_modify, |o| {
                o.summary("Device Channel PWM Mode")
                    .description("Applies PWM Mode to a specific device channel.")
                    .tag("device")
                    .security_requirement("CookieAuth")
            }),
        )
        .api_route(
            "/devices/:device_uid/settings/:channel_name/reset",
            put_with(devices::device_setting_reset, |o| {
                o.summary("Device Channel Reset")
                    .description(
                        "Resents the specific device channel settings to not-set/device default.",
                    )
                    .tag("device")
                    .security_requirement("CookieAuth")
            }),
        )
        .api_route(
            "/devices/:device_id/asetek690",
            patch_with(devices::asetek_type_update, |o| {
                o.summary("Device AseTek690")
                    .description(
                        "Set the driver type for liquidctl AseTek cooler. This is needed \
                        to set Legacy690Lc or Modern690Lc device driver type.",
                    )
                    .tag("device")
            }),
        )
        ///////////////////////////////////////////////////////////////////////////////////////////
        .api_route(
            "/status",
            post_with(status::retrieve, |o| {
                o.summary("Retrieve Status")
                    .description(
                        "Returns the status of all devices with the selected \
                        filters from the request body",
                    )
                    .tag("status")
            }),
        )
        ///////////////////////////////////////////////////////////////////////////////////////////
        .api_route(
            "/profiles",
            get_with(profiles::get_all, |o| {
                o.summary("Retrieve Profile List")
                    .description("Returns a list of all the persisted Profiles.")
                    .tag("profile")
            })
            .post_with(profiles::create, |o| {
                o.summary("Create Profile")
                    .description("Creates the given Profile")
                    .tag("profile")
                    .security_requirement("CookieAuth")
            })
            .put_with(profiles::update, |o| {
                o.summary("Update Profile")
                    .description(
                        "Updates the Profile with the given properties. \
                        Dependent on the Profile UID.",
                    )
                    .tag("profile")
                    .security_requirement("CookieAuth")
            }),
        )
        .api_route(
            "/profiles/:profile_uid",
            delete_with(profiles::delete, |o| {
                o.summary("Delete Profile")
                    .description("Deletes the Profile with the given Profile UID")
                    .tag("profile")
                    .security_requirement("CookieAuth")
            }),
        )
        .api_route(
            "/profiles/order",
            post_with(profiles::save_order, |o| {
                o.summary("Save Profile Order")
                    .description("Saves the order of Profiles as given.")
                    .tag("profile")
            }),
        )
        ///////////////////////////////////////////////////////////////////////////////////////////
        .api_route(
            "/functions",
            get_with(functions::get_all, |o| {
                o.summary("Retrieve Function List")
                    .description("Returns a list of all the persisted Functions.")
                    .tag("function")
            })
            .post_with(functions::create, |o| {
                o.summary("Create Function")
                    .description("Creates the given Function")
                    .tag("function")
                    .security_requirement("CookieAuth")
            })
            .put_with(functions::update, |o| {
                o.summary("Update Function")
                    .description(
                        "Updates the Function with the given properties. \
                        Dependent on the Function UID.",
                    )
                    .tag("function")
                    .security_requirement("CookieAuth")
            }),
        )
        .api_route(
            "/functions/:function_uid",
            delete_with(functions::delete, |o| {
                o.summary("Delete Function")
                    .description("Deletes the Function with the given Function UID")
                    .tag("function")
                    .security_requirement("CookieAuth")
            }),
        )
        .api_route(
            "/functions/order",
            post_with(functions::save_order, |o| {
                o.summary("Save Function Order")
                    .description("Saves the order of the Functions as given.")
                    .tag("function")
            }),
        )
        ///////////////////////////////////////////////////////////////////////////////////////////
        .api_route(
            "/custom-sensors",
            get_with(custom_sensors::get_all, |o| {
                o.summary("Retrieve Custom Sensor List")
                    .description("Returns a list of all the persisted Custom Sensors.")
                    .tag("custom-sensor")
            })
            .post_with(custom_sensors::create, |o| {
                o.summary("Create Custom Sensor")
                    .description("Creates the given Custom Sensor")
                    .tag("custom-sensor")
                    .security_requirement("CookieAuth")
            })
            .put_with(custom_sensors::update, |o| {
                o.summary("Update Custom Sensor")
                    .description(
                        "Updates the Custom Sensor with the given properties. \
                        Dependent on the Custom Sensor ID.",
                    )
                    .tag("custom-sensor")
                    .security_requirement("CookieAuth")
            }),
        )
        .api_route(
            "/custom-sensors/:custom_sensor_id",
            get_with(custom_sensors::get, |o| {
                o.summary("Retrieve Custom Sensor")
                    .description("Retrieves the Custom Sensor with the given Custom Sensor ID")
                    .tag("custom-sensor")
            })
            .delete_with(custom_sensors::delete, |o| {
                o.summary("Delete Custom Sensor")
                    .description("Deletes the Custom Sensor with the given Custom Sensor UID")
                    .tag("custom-sensor")
                    .security_requirement("CookieAuth")
            }),
        )
        .api_route(
            "/custom-sensors/order",
            post_with(custom_sensors::save_order, |o| {
                o.summary("Save Custom Sensor Order")
                    .description("Saves the order of the Custom Sensors as given.")
                    .tag("custom-sensor")
            }),
        )
        ///////////////////////////////////////////////////////////////////////////////////////////
        .api_route(
            "/modes",
            get_with(modes::get_all, |o| {
                o.summary("Retrieve Mode List")
                    .description("Returns a list of all the persisted Modes.")
                    .tag("mode")
            })
                .post_with(modes::create, |o| {
                    o.summary("Create Mode")
                        .description("Creates a Mode with the given name, based on the currently applied settings.")
                        .tag("mode")
                        .security_requirement("CookieAuth")
                })
                .put_with(modes::update, |o| {
                    o.summary("Update Mode")
                        .description("Updates the Mode with the given properties.")
                        .tag("mode")
                        .security_requirement("CookieAuth")
                }),
        )
        .api_route(
            "/modes/:mode_uid",
            get_with(modes::get, |o| {
                o.summary("Retrieve Mode")
                    .description("Retrieves the Mode with the given Mode UID")
                    .tag("mode")
            })
                .delete_with(modes::delete, |o| {
                    o.summary("Delete Mode")
                        .description("Deletes the Mode with the given Mode UID")
                        .tag("mode")
                        .security_requirement("CookieAuth")
                }),
        )
        .api_route(
            "/modes/:mode_uid/duplicate",
            post_with(modes::duplicate, |o| {
                o.summary("Duplicate Mode")
                    .description(
                        "Duplicates the Mode and it's settings from the given \
                        Mode UID and returns the new Mode."
                    )
                    .tag("mode")
                    .security_requirement("CookieAuth")
            }),
        )
        .api_route(
            "/modes/:mode_uid/settings",
            put_with(modes::update_mode_settings, |o| {
                o.summary("Update Mode Device Settings")
                    .description(
                        "Updates the Mode with the given Mode UID device settings to \
                        what is currently applied, and returns the Mode with it's new settings."
                    )
                    .tag("mode")
                    .security_requirement("CookieAuth")
            }),
        )
        .api_route(
            "/modes-active",
            get_with(modes::get_all_active, |o| {
                o.summary("Retrieve Active Modes")
                    .description(
                        "Returns a list of all Mode UIDs that are currently considered Active."
                    )
                    .tag("mode")
            }),
        )
        .api_route(
            "/modes-active/:mode_uid",
            post_with(modes::activate, |o| {
                o.summary("Activate Mode")
                    .description(
                        "Activates the Mode with the given Mode UID. \
                        This applies all of this Mode's device settings."
                    )
                    .tag("mode")
                    .security_requirement("CookieAuth")
            }),
        )
        .api_route(
            "/modes/order",
            post_with(modes::save_order, |o| {
                o.summary("Save Mode Order")
                    .description("Saves the order of the Modes as given.")
                    .tag("mode")
                    .security_requirement("CookieAuth")
            }),
        )
        ///////////////////////////////////////////////////////////////////////////////////////////
        .api_route(
            "/settings",
            get_with(settings::get_cc, |o| {
                o.summary("CoolerControl Settings")
                    .description("Returns the current CoolerControl settings.")
                    .tag("setting")
            })
                .patch_with(settings::update_cc, |o| {
                o.summary("Update CoolerControl Settings")
                    .description("Applies only the given properties.")
                    .tag("setting")
                    .security_requirement("CookieAuth")
            })
        )
        .api_route(
            "/settings/devices",
            get_with(settings::get_all_cc_devices, |o| {
                o.summary("CoolerControl All Device Settings")
                    .description("Returns the current CoolerControl device settings for all devices.")
                    .tag("setting")
            })
        )
        .api_route(
            "/settings/devices/:device_uid",
            get_with(settings::get_cc_device, |o| {
                o.summary("CoolerControl Device Settings")
                    .description("Returns the current CoolerControl device settings for the given device UID.")
                    .tag("setting")
            })
                .put_with(settings::update_cc_device, |o| {
                o.summary("Update CoolerControl Device Settings")
                    .description("Updates the CoolerControl device settings for the given device UID.")
                    .tag("setting")
                    .security_requirement("CookieAuth")
            })
        )
        .api_route(
            "/settings/ui",
            get_with(settings::get_ui, |o| {
                o.summary("CoolerControl UI Settings")
                    .description("Returns the current CoolerControl UI Settings.")
                    .tag("setting")
            })
                .put_with(settings::update_ui, |o| {
                    o.summary("Update CoolerControl UI Settings")
                        .description("Updates and persists the CoolerControl UI settings.")
                        .tag("setting")
                })
        )
        ///////////////////////////////////////////////////////////////////////////////////////////
        .route("/api.json", get(base::serve_api_doc))
        .with_state(app_state);
    router
}
