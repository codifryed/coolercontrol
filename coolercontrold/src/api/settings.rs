/*
 * CoolerControl - monitor and control your cooling and other devices
 * Copyright (c) 2023  Guy Boldon
 * |
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation, either version 3 of the License, or
 * (at your option) any later version.
 * |
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 * |
 * You should have received a copy of the GNU General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

use std::sync::Arc;
use std::time::Duration;

use actix_web::{get, HttpResponse, patch, post, Responder};
use actix_web::web::{Data, Json};
use log::error;
use serde::{Deserialize, Serialize};

use crate::api::{ErrorResponse, handle_error, handle_simple_result};
use crate::config::Config;
use crate::setting::CoolerControlSettings;

/// Get CoolerControl settings
#[get("/settings")]
async fn get_cc_settings(
    config: Data<Arc<Config>>,
) -> impl Responder {
    match config.get_settings().await {
        Ok(settings) => HttpResponse::Ok()
            .json(Json(CoolerControlSettingsDto::from(&settings))),
        Err(err) => handle_error(err)
    }
}

/// Apply CoolerControl settings
#[patch("/settings")]
async fn apply_cc_settings(
    cc_settings_request: Json<CoolerControlSettingsDto>,
    config: Data<Arc<Config>>,
) -> impl Responder {
    let result = match config.get_settings().await {
        Ok(current_settings) => {
            let settings_to_set = cc_settings_request.merge(current_settings);
            config.set_settings(&settings_to_set).await;
            config.save_config_file().await
        }
        Err(err) => Err(err)
    };
    handle_simple_result(result)
}

/// Retrieves the persisted UI Settings, if found.
#[get("/settings/ui")]
async fn get_ui_settings(
    config: Data<Arc<Config>>,
) -> impl Responder {
    match config.load_ui_config_file().await {
        Ok(settings) => HttpResponse::Ok().body(settings),
        Err(err) => {
            error!("{:?}", err);
            let error = err.root_cause().to_string();
            if error.contains("No such file") {
                HttpResponse::NotFound()
                    .json(Json(ErrorResponse { error }))
            } else {
                HttpResponse::InternalServerError()
                    .json(Json(ErrorResponse { error }))
            }
        }
    }
}

/// Persists the UI Settings.
#[post("/settings/ui")]
async fn save_ui_settings(
    ui_settings_request: String,
    config: Data<Arc<Config>>,
) -> impl Responder {
    handle_simple_result(config.save_ui_config_file(&ui_settings_request).await)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CoolerControlSettingsDto {
    apply_on_boot: Option<bool>,
    handle_dynamic_temps: Option<bool>,
    startup_delay: Option<u8>,
    smoothing_level: Option<u8>,
    thinkpad_full_speed: Option<bool>,
}

impl CoolerControlSettingsDto {
    fn merge(&self, current_settings: CoolerControlSettings) -> CoolerControlSettings {
        let apply_on_boot = if let Some(apply) = self.apply_on_boot {
            apply
        } else {
            current_settings.apply_on_boot
        };
        let handle_dynamic_temps = if let Some(should_handle) = self.handle_dynamic_temps {
            should_handle
        } else {
            current_settings.handle_dynamic_temps
        };
        let startup_delay = if let Some(delay) = self.startup_delay {
            Duration::from_secs(delay.max(0).min(10) as u64)
        } else {
            current_settings.startup_delay
        };
        let smoothing_level = if let Some(level) = self.smoothing_level {
            level
        } else {
            current_settings.smoothing_level
        };
        let thinkpad_full_speed = if let Some(full_speed) = self.thinkpad_full_speed {
            full_speed
        } else {
            current_settings.thinkpad_full_speed
        };
        CoolerControlSettings {
            apply_on_boot,
            no_init: current_settings.no_init,
            handle_dynamic_temps,
            startup_delay,
            smoothing_level,
            thinkpad_full_speed,
        }
    }
}

impl From<&CoolerControlSettings> for CoolerControlSettingsDto {
    fn from(settings: &CoolerControlSettings) -> Self {
        Self {
            apply_on_boot: Some(settings.apply_on_boot),
            handle_dynamic_temps: Some(settings.handle_dynamic_temps),
            startup_delay: Some(settings.startup_delay.as_secs() as u8),
            smoothing_level: Some(settings.smoothing_level),
            thinkpad_full_speed: Some(settings.thinkpad_full_speed),
        }
    }
}