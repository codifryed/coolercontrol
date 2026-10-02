// SPDX-FileCopyrightText: 2024 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use crate::api::{
    alerts, auth, auth_throttle, base, calibration, connection, custom_sensors, detect,
    device_health, functions, hardware_report, metrics, modes, plugins, power_profiles,
    profile_generation, profiles, settings, sse, stats, status, stress_test, tokens,
};
use crate::api::{devices, AppState};
use crate::grpc_api;
#[cfg(debug_assertions)]
use aide::axum::routing::get;
use aide::axum::routing::{delete_with, get_with, patch_with, post_with, put_with};
use aide::axum::ApiRouter;
use axum::extract::Request;
use axum::http::{header, HeaderMap, StatusCode};
use axum::middleware::Next;
use axum::response::Response;
use axum::Extension;
use std::ops::Not;

// Note: using `#[debug_handler]` on the handler functions themselves is sometimes very helpful.

pub async fn init(app_state: AppState) -> ApiRouter {
    let token_handle = app_state.token_handle.clone();
    let document_csp = app_state.frame_policy.document_csp.clone();
    let router = documented_routes().merge(grpc_routes(&app_state).await);
    // Only add API doc route for debug builds (safer for production)
    #[cfg(debug_assertions)]
    let router = router.route("/api.json", get(base::serve_api_doc));

    router
        .fallback_service(base::web_app_service(document_csp))
        .with_state(app_state)
        // need an extension here for middleware::from_fn to work and not pass app_state everywhere.
        .layer(Extension(token_handle))
        // Outermost here, so a throttled peer is turned away before any token validation
        // runs. `grpc_error_middleware` wraps this whole router from `create_api_server`.
        .layer(axum::middleware::from_fn(
            auth_throttle::token_throttle_middleware,
        ))
}

/// Renders error responses that gRPC clients can actually read.
///
/// The auth middleware, the throttle, and the timeout layer all answer with ordinary
/// HTTP status codes and a JSON body. A gRPC client cannot parse that: it reports
/// `Internal` with a content-type complaint, so an expired token would reach a user as
/// an unexplained protocol error rather than "your credentials were refused". This
/// translates those into properly framed `grpc-status` responses.
///
/// Applied by `create_api_server` outside every other layer, which is the only position
/// that catches the timeout: `TimeoutLayer` wraps this router, so a 408 is produced above
/// anything `init` could install and would otherwise never reach this translation.
/// Requests that are not gRPC, and responses already framed as gRPC, pass through
/// untouched.
pub async fn grpc_error_middleware(request: Request, next: Next) -> Response {
    let is_grpc = is_grpc_content_type(request.headers());
    let response = next.run(request).await;
    if is_grpc.not() {
        return response;
    }
    if is_grpc_content_type(response.headers()) {
        return response;
    }
    let Some((code, message)) = grpc_code_for(response.status()) else {
        return response;
    };
    tonic::Status::new(code, message).into_http()
}

fn is_grpc_content_type(headers: &HeaderMap) -> bool {
    headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.starts_with("application/grpc"))
}

/// Maps the HTTP statuses our own layers produce onto gRPC codes. Anything else is left
/// alone: a status we did not generate is not ours to reinterpret.
fn grpc_code_for(status: StatusCode) -> Option<(tonic::Code, &'static str)> {
    match status {
        StatusCode::UNAUTHORIZED => Some((
            tonic::Code::Unauthenticated,
            "Invalid or missing access token.",
        )),
        StatusCode::FORBIDDEN => Some((
            tonic::Code::PermissionDenied,
            "This token does not have the required access.",
        )),
        StatusCode::TOO_MANY_REQUESTS => Some((
            tonic::Code::ResourceExhausted,
            "Too many failed authentication attempts.",
        )),
        StatusCode::REQUEST_TIMEOUT => {
            Some((tonic::Code::DeadlineExceeded, "The request timed out."))
        }
        StatusCode::NOT_FOUND => Some((tonic::Code::Unimplemented, "Unknown gRPC method.")),
        _ => None,
    }
}

/// The gRPC services, mounted on the REST listener behind the same read-tier auth.
///
/// gRPC used to run its own `tonic::transport::Server` on `port + 1`, which meant it had
/// no TLS and no authentication at all: a second, weaker front door onto the same data.
/// Sharing the REST listener is what closes that, and it costs nothing, because that
/// listener already speaks HTTP/2.
///
/// These carry no `OpenAPI` metadata on purpose. They are not REST operations and have no
/// place in the spec, so they are merged in outside `documented_routes`.
async fn grpc_routes(app_state: &AppState) -> ApiRouter<AppState> {
    let device_service = grpc_api::device_service(
        app_state.device_handle.clone(),
        app_state.status_handle.clone(),
        app_state.calibration_handle.clone(),
    );
    ApiRouter::new()
        .route_service(&grpc_api::device_service_route(), device_service)
        .route_service(
            &grpc_api::health_service_route(),
            grpc_api::health_service().await,
        )
        // The whole served surface is read-only: every mutating RPC answers
        // `Unimplemented`. So it takes the same read-tier credentials as `GET /devices`,
        // rather than a third auth semantic invented for gRPC.
        .layer(axum::middleware::from_fn(auth::auth_middleware))
}

/// Every route carrying `OpenAPI` metadata, before state, the fallback and the doc route are
/// attached. None of those three contribute operations, so generating the spec from this alone
/// yields exactly what `/api.json` serves, with no `AppState` and no running server.
pub fn documented_routes() -> ApiRouter<AppState> {
    base_routes()
        .merge(auth_routes())
        .merge(token_routes())
        .merge(device_routes())
        .merge(device_health_routes())
        .merge(status_routes())
        .merge(stats_routes())
        .merge(profile_routes())
        .merge(function_routes())
        .merge(custom_sensor_routes())
        .merge(mode_routes())
        .merge(power_profile_routes())
        .merge(settings_routes())
        .merge(plugins_routes())
        .merge(alert_routes())
        .merge(calibration_routes())
        .merge(calibration_batch_routes())
        .merge(calibration_map_routes())
        .merge(detect_routes())
        .merge(hardware_report_routes())
        .merge(stress_test_routes())
        .merge(metrics_routes())
        .merge(sse_routes())
}

fn base_routes() -> ApiRouter<AppState> {
    ApiRouter::new()
        .api_route(
            "/handshake",
            get_with(base::handshake, |o| {
                o.summary("Handshake")
                    .description("A simple endpoint to verify the connection")
                    .tag("base")
            }),
        )
        .api_route(
            "/health",
            get_with(base::health, |o| {
                o.summary("Health Check")
                    .description("Returns a Health Check Status.")
                    .tag("base")
                    .security_requirement("CookieAuth")
                    .security_requirement("BearerAuth")
            })
            .layer(axum::middleware::from_fn(auth::auth_middleware)),
        )
        .api_route(
            "/logs",
            get_with(base::logs, |o| {
                o.summary("Daemon Logs")
                    .description("This returns all recent main daemon logs as raw text")
                    .tag("base")
                    .security_requirement("CookieAuth")
                    .security_requirement("BearerAuth")
            })
            .layer(axum::middleware::from_fn(auth::auth_middleware)),
        )
        .api_route(
            "/acknowledge",
            post_with(base::acknowledge_issues, |o| {
                o.summary("Acknowledge Log Issues")
                    .description("This acknowledges existing log warnings and errors, and sets a timestamp of when this occurred")
                    .tag("base")
                .security_requirement("CookieAuth")
                    .security_requirement("BearerAuth")
            }).layer(axum::middleware::from_fn(auth::auth_write_middleware)),
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
            }).layer(axum::middleware::from_fn(auth::session_auth_middleware)),
        )
}

fn auth_routes() -> ApiRouter<AppState> {
    ApiRouter::new()
        .api_route(
            "/login",
            post_with(auth::login, |o| {
                o.summary("Login")
                    .description("The endpoint used to create a login session.")
                    .tag("auth")
                    .security_requirement("BasicAuth")
            })
            // The handler checks the password itself, so its status is the verdict.
            .layer(axum::middleware::from_fn(
                connection::promote_on_success_middleware,
            ))
            .layer(axum::middleware::from_fn(
                auth_throttle::password_throttle_middleware,
            )),
        )
        .api_route(
            "/verify-session",
            post_with(auth::verify_session, |o| {
                o.summary("Verify Session Auth")
                    .description("Verifies that the current session is still authenticated")
                    .tag("auth")
                    .security_requirement("CookieAuth")
            })
            .layer(axum::middleware::from_fn(auth::session_auth_middleware)),
        )
        .api_route(
            "/set-passwd",
            post_with(auth::set_passwd, |o| {
                o.summary("Set Admin Password")
                    .description("Stores a new Admin password.")
                    .tag("auth")
                    // both are required:
                    .security_requirement_multi(["CookieAuth", "BasicAuth"])
            })
            // Inside the session check: a cookie-less 401 is not a password guess. Without
            // this, a stolen session could brute-force `current_password` unthrottled.
            .layer(axum::middleware::from_fn(
                auth_throttle::password_throttle_middleware,
            ))
            .layer(axum::middleware::from_fn(auth::session_auth_middleware)),
        )
        .api_route(
            "/logout",
            post_with(auth::logout, |o| {
                o.summary("Logout")
                    .description("Logout and invalidate the current session.")
                    .tag("auth")
                    .security_requirement("CookieAuth")
            })
            .layer(axum::middleware::from_fn(auth::session_auth_middleware)),
        )
}

fn token_routes() -> ApiRouter<AppState> {
    ApiRouter::new()
        .api_route(
            "/tokens",
            post_with(tokens::create, |o| {
                o.summary("Create Access Token")
                    .description(
                        "Creates a new access token for external service authentication. \
                        The raw token is only returned once.",
                    )
                    .tag("auth")
                    .security_requirement("CookieAuth")
            })
            .get_with(tokens::list, |o| {
                o.summary("List Access Tokens")
                    .description("Returns a list of all access tokens (without hashes).")
                    .tag("auth")
                    .security_requirement("CookieAuth")
            })
            .layer(axum::middleware::from_fn(auth::session_auth_middleware)),
        )
        .api_route(
            "/tokens/{token_id}",
            delete_with(tokens::delete, |o| {
                o.summary("Delete Access Token")
                    .description("Deletes the access token with the given ID.")
                    .tag("auth")
                    .security_requirement("CookieAuth")
            })
            .layer(axum::middleware::from_fn(auth::session_auth_middleware)),
        )
}

#[allow(clippy::too_many_lines)]
fn device_routes() -> ApiRouter<AppState> {
    ApiRouter::new()
        .api_route(
            "/thinkpad-fan-control",
            put_with(devices::thinkpad_fan_control_modify, |o| {
                o.summary("ThinkPad Fan Control")
                    .description(
                        "Enables/Disabled Fan Control for ThinkPads, if acpi driver is present.",
                    )
                    .tag("device")
                    .security_requirement("CookieAuth")
                    .security_requirement("BearerAuth")
            })
            .layer(axum::middleware::from_fn(auth::auth_write_middleware)),
        )
        .api_route(
            "/amd-gpu-overdrive",
            post_with(devices::amd_gpu_overdrive_enable, |o| {
                o.summary("AMD GPU Overdrive Enable")
                    .description(
                        "Enables AMD GPU overdrive by configuring the ppfeaturemask \
                        kernel parameter. Requires a reboot to take effect.",
                    )
                    .tag("device")
                    .security_requirement("CookieAuth")
                    .security_requirement("BearerAuth")
            })
            .layer(axum::middleware::from_fn(auth::auth_write_middleware)),
        )
        .api_route(
            "/devices",
            get_with(devices::get, |o| {
                o.summary("All Devices")
                    .description(
                        "Returns a list of all detected devices and their associated information.",
                    )
                    .tag("device")
                    .security_requirement("CookieAuth")
                    .security_requirement("BearerAuth")
            })
            .layer(axum::middleware::from_fn(auth::auth_middleware)),
        )
        .api_route(
            "/devices/{device_uid}/channels/{channel_name}/attributes",
            get_with(devices::device_channel_attributes_get, |o| {
                o.summary("Device Channel Attributes")
                    .description(
                        "Returns the extra attributes the driver reports for a temperature, fan \
                        or power channel, such as its max, critical and emergency limits, fan \
                        target or power cap, read from the hardware when requested. Each is \
                        named by its sysfs file, because the meaning varies by driver: on some \
                        chips min and max are the lowest and highest readings so far. Unset and \
                        unreadable values are left out. Channels without attribute files return \
                        an empty list.",
                    )
                    .tag("device")
                    .security_requirement("CookieAuth")
                    .security_requirement("BearerAuth")
            })
            .layer(axum::middleware::from_fn(auth::auth_middleware)),
        )
        .api_route(
            "/devices/{device_uid}/settings",
            get_with(devices::device_settings_get, |o| {
                o.summary("All Device Settings")
                    .description(
                        "Returns all the currently applied settings for the given device. \
                    It returns the Config Settings model, which includes all possibilities \
                    for each channel.",
                    )
                    .tag("device")
                    .security_requirement("CookieAuth")
                    .security_requirement("BearerAuth")
            })
            .layer(axum::middleware::from_fn(auth::auth_middleware)),
        )
        .api_route(
            "/devices/{device_uid}/settings/{channel_name}/manual",
            put_with(devices::device_setting_manual_modify, |o| {
                o.summary("Device Channel Manual")
                    .description("Applies a fan duty to a specific device channel.")
                    .tag("device")
                    .security_requirement("CookieAuth")
                    .security_requirement("BearerAuth")
            })
            .layer(axum::middleware::from_fn(auth::auth_write_middleware)),
        )
        .api_route(
            "/devices/{device_uid}/settings/{channel_name}/profile",
            put_with(devices::device_setting_profile_modify, |o| {
                o.summary("Device Channel Profile")
                    .description("Applies a Profile to a specific device channel.")
                    .tag("device")
                    .security_requirement("CookieAuth")
                    .security_requirement("BearerAuth")
            })
            .layer(axum::middleware::from_fn(auth::auth_write_middleware)),
        )
        .api_route(
            "/devices/{device_uid}/settings/{channel_name}/lcd",
            put_with(devices::device_setting_lcd_modify, |o| {
                o.summary("Device Channel LCD")
                    .description("Applies LCD Settings to a specific device channel.")
                    .tag("device")
                    .security_requirement("CookieAuth")
                    .security_requirement("BearerAuth")
            })
            .layer(axum::middleware::from_fn(auth::auth_write_middleware)),
        )
        .api_route(
            "/devices/{device_uid}/settings/{channel_name}/lcd/images",
            get_with(devices::get_device_lcd_image, |o| {
                o.summary("Retrieve Device Channel LCD")
                    .description("Retrieves the currently applied LCD Image file.")
                    .tag("device")
                    .security_requirement("CookieAuth")
                    .security_requirement("BearerAuth")
            })
            .layer(axum::middleware::from_fn(auth::auth_middleware)),
        )
        .api_route(
            "/devices/{device_uid}/settings/{channel_name}/lcd/images",
            post_with(devices::process_device_lcd_images, |o| {
                o.summary("Process Device Channel LCD Image")
                    .description(
                        "This takes and image file and processes it for optimal \
                use by the specified device channel. This is useful for a UI Preview \
                and is used internally before applying the image to the device.",
                    )
                    .tag("device")
                    .security_requirement("CookieAuth")
                    .security_requirement("BearerAuth")
            })
            .put_with(devices::update_device_setting_lcd_image, |o| {
                o.summary("Update Device Channel LCD Settings")
                    .description("Used to apply LCD settings that contain images.")
                    .tag("device")
                    .security_requirement("CookieAuth")
                    .security_requirement("BearerAuth")
            })
            .layer(axum::middleware::from_fn(auth::auth_write_middleware)),
        )
        .api_route(
            "/devices/{device_uid}/settings/{channel_name}/lcd/shutdown-image",
            put_with(devices::set_device_lcd_shutdown_image, |o| {
                o.summary("Set LCD Shutdown Image")
                    .description(
                        "Upload and save an LCD image that will be applied to the \
                        device when the daemon shuts down.",
                    )
                    .tag("device")
                    .security_requirement("CookieAuth")
                    .security_requirement("BearerAuth")
            })
            .delete_with(devices::delete_device_lcd_shutdown_image, |o| {
                o.summary("Clear LCD Shutdown Image")
                    .description(
                        "Remove the saved LCD shutdown image for the given device channel.",
                    )
                    .tag("device")
                    .security_requirement("CookieAuth")
                    .security_requirement("BearerAuth")
            })
            .layer(axum::middleware::from_fn(auth::auth_write_middleware)),
        )
        .api_route(
            "/devices/{device_uid}/settings/{channel_name}/lighting",
            put_with(devices::device_setting_lighting_modify, |o| {
                o.summary("Device Channel Lighting")
                    .description("Applies Lighting Settings (RGB) to a specific device channel.")
                    .tag("device")
                    .security_requirement("CookieAuth")
                    .security_requirement("BearerAuth")
            })
            .layer(axum::middleware::from_fn(auth::auth_write_middleware)),
        )
        .api_route(
            "/devices/{device_uid}/settings/{channel_name}/pwm",
            put_with(devices::device_setting_pwm_mode_modify, |o| {
                o.summary("DEPRECATED: Device Channel PWM Mode")
                    .description("DEPRECATED: Applies PWM Mode to a specific device channel.")
                    .tag("device")
                    .security_requirement("CookieAuth")
                    .security_requirement("BearerAuth")
            })
            .layer(axum::middleware::from_fn(auth::auth_write_middleware)),
        )
        .api_route(
            "/devices/{device_uid}/settings/{channel_name}/reset",
            put_with(devices::device_setting_reset, |o| {
                o.summary("Device Channel Reset")
                    .description(
                        "Resents the specific device channel settings to not-set/device default.",
                    )
                    .tag("device")
                    .security_requirement("CookieAuth")
                    .security_requirement("BearerAuth")
            })
            .layer(axum::middleware::from_fn(auth::auth_write_middleware)),
        )
        .api_route(
            "/devices/{device_uid}/asetek690",
            patch_with(devices::asetek_type_update, |o| {
                o.summary("Device AseTek690")
                    .description(
                        "Set the driver type for liquidctl AseTek cooler. This is needed \
                    to set Legacy690Lc or Modern690Lc device driver type.",
                    )
                    .tag("device")
                    .security_requirement("CookieAuth")
                    .security_requirement("BearerAuth")
            })
            .layer(axum::middleware::from_fn(auth::auth_write_middleware)),
        )
}

fn status_routes() -> ApiRouter<AppState> {
    ApiRouter::new()
        .api_route(
            "/status",
            post_with(status::retrieve, |o| {
                o.summary("Retrieve Status")
                    .description(
                        "Returns the status of all devices and their channels,with the \
                        selected filters from the request body. This endpoint has the most \
                        options available for retrieving all statuses.",
                    )
                    .tag("status")
                    .security_requirement("CookieAuth")
                    .security_requirement("BearerAuth")
            })
            .get_with(status::get_all, |o| {
                o.summary("Retrieve Status")
                    .description(
                        "Returns the status of all devices and their channels, returning \
                        only the most recent status by default.",
                    )
                    .tag("status")
                    .security_requirement("CookieAuth")
                    .security_requirement("BearerAuth")
            })
            .layer(axum::middleware::from_fn(auth::auth_middleware)),
        )
        .api_route(
            "/status/{device_uid}",
            get_with(status::get_device, |o| {
                o.summary("Retrieve Device Status")
                    .description(
                        "Returns the status of all channels for a specific device, \
                        returning only the most recent status by default.",
                    )
                    .tag("status")
                    .security_requirement("CookieAuth")
                    .security_requirement("BearerAuth")
            })
            .layer(axum::middleware::from_fn(auth::auth_middleware)),
        )
        .api_route(
            "/status/{device_uid}/channels/{channel_name}",
            get_with(status::get_device_channel, |o| {
                o.summary("Retrieve Device Channel Status")
                    .description(
                        "Returns the status of a specific channel for a specific device, \
                        returning only the most recent status by default.",
                    )
                    .tag("status")
                    .security_requirement("CookieAuth")
                    .security_requirement("BearerAuth")
            })
            .layer(axum::middleware::from_fn(auth::auth_middleware)),
        )
}

fn stats_routes() -> ApiRouter<AppState> {
    ApiRouter::new().api_route(
        "/stats",
        get_with(stats::get_all, |o| {
            o.summary("Retrieve Channel Stats")
                .description(
                    "Returns running min/max/avg/count per channel and temp since \
                         daemon start. Entries are present once observed at least once.",
                )
                .tag("stats")
                .security_requirement("CookieAuth")
                .security_requirement("BearerAuth")
        })
        .delete_with(stats::delete_all, |o| {
            o.summary("Reset Channel Stats")
                .description(
                    "Clears all running stats and reseeds each from the most recent \
                         status with count=1. Channels absent from the most recent status \
                         are dropped and will reseed naturally on their next observation.",
                )
                .tag("stats")
                .security_requirement("CookieAuth")
                .security_requirement("BearerAuth")
        })
        .layer(axum::middleware::from_fn(auth::auth_middleware)),
    )
}

fn profile_routes() -> ApiRouter<AppState> {
    ApiRouter::new()
        .api_route(
            "/profiles",
            get_with(profiles::get_all, |o| {
                o.summary("Retrieve Profile List")
                    .description("Returns a list of all the persisted Profiles.")
                    .tag("profile")
                    .security_requirement("CookieAuth")
                    .security_requirement("BearerAuth")
            })
            .layer(axum::middleware::from_fn(auth::auth_middleware)),
        )
        .api_route(
            "/profiles",
            post_with(profiles::create, |o| {
                o.summary("Create Profile")
                    .description("Creates the given Profile")
                    .tag("profile")
                    .security_requirement("CookieAuth")
                    .security_requirement("BearerAuth")
            })
            .put_with(profiles::update, |o| {
                o.summary("Update Profile")
                    .description(
                        "Updates the Profile with the given properties. \
                    Dependent on the Profile UID.",
                    )
                    .tag("profile")
                    .security_requirement("CookieAuth")
                    .security_requirement("BearerAuth")
            })
            .layer(axum::middleware::from_fn(auth::auth_write_middleware)),
        )
        .api_route(
            "/profiles/{profile_uid}",
            delete_with(profiles::delete, |o| {
                o.summary("Delete Profile")
                    .description("Deletes the Profile with the given Profile UID")
                    .tag("profile")
                    .security_requirement("CookieAuth")
                    .security_requirement("BearerAuth")
            })
            .layer(axum::middleware::from_fn(auth::auth_write_middleware)),
        )
        .api_route(
            "/profiles/order",
            post_with(profiles::save_order, |o| {
                o.summary("Save Profile Order")
                    .description("Saves the order of Profiles as given.")
                    .tag("profile")
                    .security_requirement("CookieAuth")
                    .security_requirement("BearerAuth")
            })
            .layer(axum::middleware::from_fn(auth::auth_write_middleware)),
        )
        .api_route(
            "/profiles/generate",
            post_with(profile_generation::generate, |o| {
                o.summary("Generate Profiles")
                    .description(
                        "Proposes profiles, functions, and custom sensors for the given fan \
                    assignments and presets, without persisting them.",
                    )
                    .tag("profile")
                    .security_requirement("CookieAuth")
                    .security_requirement("BearerAuth")
            })
            .layer(axum::middleware::from_fn(auth::auth_middleware)),
        )
}

fn function_routes() -> ApiRouter<AppState> {
    ApiRouter::new()
        .api_route(
            "/functions",
            get_with(functions::get_all, |o| {
                o.summary("Retrieve Function List")
                    .description("Returns a list of all the persisted Functions.")
                    .tag("function")
                    .security_requirement("CookieAuth")
                    .security_requirement("BearerAuth")
            })
            .layer(axum::middleware::from_fn(auth::auth_middleware)),
        )
        .api_route(
            "/functions",
            post_with(functions::create, |o| {
                o.summary("Create Function")
                    .description("Creates the given Function")
                    .tag("function")
                    .security_requirement("CookieAuth")
                    .security_requirement("BearerAuth")
            })
            .put_with(functions::update, |o| {
                o.summary("Update Function")
                    .description(
                        "Updates the Function with the given properties. \
                    Dependent on the Function UID.",
                    )
                    .tag("function")
                    .security_requirement("CookieAuth")
                    .security_requirement("BearerAuth")
            })
            .layer(axum::middleware::from_fn(auth::auth_write_middleware)),
        )
        .api_route(
            "/functions/{function_uid}",
            delete_with(functions::delete, |o| {
                o.summary("Delete Function")
                    .description("Deletes the Function with the given Function UID")
                    .tag("function")
                    .security_requirement("CookieAuth")
                    .security_requirement("BearerAuth")
            })
            .layer(axum::middleware::from_fn(auth::auth_write_middleware)),
        )
        .api_route(
            "/functions/order",
            post_with(functions::save_order, |o| {
                o.summary("Save Function Order")
                    .description("Saves the order of the Functions as given.")
                    .tag("function")
                    .security_requirement("CookieAuth")
                    .security_requirement("BearerAuth")
            })
            .layer(axum::middleware::from_fn(auth::auth_write_middleware)),
        )
}

fn custom_sensor_routes() -> ApiRouter<AppState> {
    ApiRouter::new()
        .api_route(
            "/custom-sensors",
            get_with(custom_sensors::get_all, |o| {
                o.summary("Retrieve Custom Sensor List")
                    .description("Returns a list of all the persisted Custom Sensors.")
                    .tag("custom-sensor")
                    .security_requirement("CookieAuth")
                    .security_requirement("BearerAuth")
            })
            .layer(axum::middleware::from_fn(auth::auth_middleware)),
        )
        .api_route(
            "/custom-sensors",
            post_with(custom_sensors::create, |o| {
                o.summary("Create Custom Sensor")
                    .description("Creates the given Custom Sensor")
                    .tag("custom-sensor")
                    .security_requirement("CookieAuth")
                    .security_requirement("BearerAuth")
            })
            .put_with(custom_sensors::update, |o| {
                o.summary("Update Custom Sensor")
                    .description(
                        "Updates the Custom Sensor with the given properties. \
                    Dependent on the Custom Sensor ID.",
                    )
                    .tag("custom-sensor")
                    .security_requirement("CookieAuth")
                    .security_requirement("BearerAuth")
            })
            .layer(axum::middleware::from_fn(auth::auth_write_middleware)),
        )
        .api_route(
            "/custom-sensors/{custom_sensor_id}",
            get_with(custom_sensors::get, |o| {
                o.summary("Retrieve Custom Sensor")
                    .description("Retrieves the Custom Sensor with the given Custom Sensor ID")
                    .tag("custom-sensor")
                    .security_requirement("CookieAuth")
                    .security_requirement("BearerAuth")
            })
            .layer(axum::middleware::from_fn(auth::auth_middleware)),
        )
        .api_route(
            "/custom-sensors/{custom_sensor_id}",
            delete_with(custom_sensors::delete, |o| {
                o.summary("Delete Custom Sensor")
                    .description("Deletes the Custom Sensor with the given Custom Sensor UID")
                    .tag("custom-sensor")
                    .security_requirement("CookieAuth")
                    .security_requirement("BearerAuth")
            })
            .layer(axum::middleware::from_fn(auth::auth_write_middleware)),
        )
        .api_route(
            "/custom-sensors/order",
            post_with(custom_sensors::save_order, |o| {
                o.summary("Save Custom Sensor Order")
                    .description("Saves the order of the Custom Sensors as given.")
                    .tag("custom-sensor")
                    .security_requirement("CookieAuth")
                    .security_requirement("BearerAuth")
            })
            .layer(axum::middleware::from_fn(auth::auth_write_middleware)),
        )
}

/// The system power profile integration: what the system offers, and which Mode each profile
/// activates. Reads need only read access; changing the mapping is a write.
fn power_profile_routes() -> ApiRouter<AppState> {
    ApiRouter::new()
        .api_route(
            "/power-profiles",
            get_with(power_profiles::get, |o| {
                o.summary("Retrieve Power Profile State")
                    .description(
                        "Returns the power profiles the system offers, the active one, and the \
                         profile to Mode mapping. An empty `available` list means no power \
                         profile daemon is reachable over D-Bus.",
                    )
                    .tag("power-profile")
                    .security_requirement("CookieAuth")
                    .security_requirement("BearerAuth")
            })
            .layer(axum::middleware::from_fn(auth::auth_middleware)),
        )
        .api_route(
            "/power-profiles/modes",
            put_with(power_profiles::update_modes, |o| {
                o.summary("Set Power Profile Mode Mapping")
                    .description(
                        "Replaces the mapping of system power profile to Mode. Every referenced \
                         Mode must exist. Profiles left out are unmapped.",
                    )
                    .tag("power-profile")
                    .security_requirement("CookieAuth")
                    .security_requirement("BearerAuth")
            })
            .layer(axum::middleware::from_fn(auth::auth_write_middleware)),
        )
}

#[allow(clippy::too_many_lines)]
fn mode_routes() -> ApiRouter<AppState> {
    ApiRouter::new()
        .api_route(
            "/modes",
            get_with(modes::get_all, |o| {
                o.summary("Retrieve Mode List")
                    .description("Returns a list of all the persisted Modes.")
                    .tag("mode")
                    .security_requirement("CookieAuth")
                    .security_requirement("BearerAuth")
            })
            .layer(axum::middleware::from_fn(auth::auth_middleware)),
        )
        .api_route(
            "/modes",
            post_with(modes::create, |o| {
                o.summary("Create Mode")
                    .description("Creates a Mode with the given name, based on the currently applied settings.")
                    .tag("mode")
                    .security_requirement("CookieAuth")
                    .security_requirement("BearerAuth")
            })
            .put_with(modes::update, |o| {
                o.summary("Update Mode")
                    .description("Updates the Mode with the given properties.")
                    .tag("mode")
                    .security_requirement("CookieAuth")
                    .security_requirement("BearerAuth")
            })
            .layer(axum::middleware::from_fn(auth::auth_write_middleware)),
        )
        .api_route(
            "/modes/{mode_uid}",
            get_with(modes::get, |o| {
                o.summary("Retrieve Mode")
                    .description("Retrieves the Mode with the given Mode UID")
                    .tag("mode")
                    .security_requirement("CookieAuth")
                    .security_requirement("BearerAuth")
            })
            .layer(axum::middleware::from_fn(auth::auth_middleware)),
        )
        .api_route(
            "/modes/{mode_uid}",
            delete_with(modes::delete, |o| {
                o.summary("Delete Mode")
                    .description("Deletes the Mode with the given Mode UID")
                    .tag("mode")
                    .security_requirement("CookieAuth")
                    .security_requirement("BearerAuth")
            })
            .layer(axum::middleware::from_fn(auth::auth_write_middleware)),
        )
        .api_route(
            "/modes/{mode_uid}/duplicate",
            post_with(modes::duplicate, |o| {
                o.summary("Duplicate Mode")
                    .description(
                        "Duplicates the Mode and it's settings from the given \
                    Mode UID and returns the new Mode."
                    )
                    .tag("mode")
                    .security_requirement("CookieAuth")
                    .security_requirement("BearerAuth")
            })
            .layer(axum::middleware::from_fn(auth::auth_write_middleware)),
        )
        .api_route(
            "/modes/{mode_uid}/settings",
            put_with(modes::update_mode_settings, |o| {
                o.summary("Update Mode Device Settings")
                    .description(
                        "Updates the Mode with the given Mode UID device settings to \
                    what is currently applied, and returns the Mode with it's new settings."
                    )
                    .tag("mode")
                    .security_requirement("CookieAuth")
                    .security_requirement("BearerAuth")
            })
            .layer(axum::middleware::from_fn(auth::auth_write_middleware)),
        )
        .api_route(
            "/modes-active",
            get_with(modes::get_active, |o| {
                o.summary("Retrieve Active Modes")
                    .description(
                        "Returns the active and previously active Mode UIDs."
                    )
                    .tag("mode")
                    .security_requirement("CookieAuth")
                    .security_requirement("BearerAuth")
            })
            .layer(axum::middleware::from_fn(auth::auth_middleware)),
        )
        .api_route(
            "/modes-active/{mode_uid}",
            post_with(modes::activate, |o| {
                o.summary("Activate Mode")
                    .description(
                        "Activates the Mode with the given Mode UID. \
                    This applies all of this Mode's device settings."
                    )
                    .tag("mode")
                    .security_requirement("CookieAuth")
                    .security_requirement("BearerAuth")
            })
            .layer(axum::middleware::from_fn(auth::auth_write_middleware)),
        )
        .api_route(
            "/modes/order",
            post_with(modes::save_order, |o| {
                o.summary("Save Mode Order")
                    .description("Saves the order of the Modes as given.")
                    .tag("mode")
                    .security_requirement("CookieAuth")
                    .security_requirement("BearerAuth")
            })
            .layer(axum::middleware::from_fn(auth::auth_write_middleware)),
        )
}

// Declarative route registration; splitting hurts readability.
#[allow(clippy::too_many_lines)]
fn settings_routes() -> ApiRouter<AppState> {
    ApiRouter::new()
        .api_route(
            "/settings",
            get_with(settings::get_cc, |o| {
                o.summary("CoolerControl Settings")
                    .description("Returns the current CoolerControl settings.")
                    .tag("setting")
                    .security_requirement("CookieAuth")
                    .security_requirement("BearerAuth")
            })
            .layer(axum::middleware::from_fn(auth::auth_middleware)),
        )
        .api_route(
            "/settings",
            patch_with(settings::update_cc, |o| {
                o.summary("Update CoolerControl Settings")
                    .description("Applies only the given properties.")
                    .tag("setting")
                    .security_requirement("CookieAuth")
                    .security_requirement("BearerAuth")
            })
            .layer(axum::middleware::from_fn(auth::auth_write_middleware)),
        )
        .api_route(
            "/settings/devices",
            get_with(settings::get_all_cc_devices, |o| {
                o.summary("CoolerControl All Device Settings")
                    .description("Returns the current CoolerControl device settings for all devices.")
                    .tag("setting")
                    .security_requirement("CookieAuth")
                    .security_requirement("BearerAuth")
            })
            .layer(axum::middleware::from_fn(auth::auth_middleware)),
        )
        .api_route(
            "/settings/devices/{device_uid}",
            get_with(settings::get_cc_device, |o| {
                o.summary("CoolerControl Device Settings")
                    .description("Returns the current CoolerControl device settings for the given device UID.")
                    .tag("setting")
                    .security_requirement("CookieAuth")
                    .security_requirement("BearerAuth")
            })
            .layer(axum::middleware::from_fn(auth::auth_middleware)),
        )
        .api_route(
            "/settings/devices/{device_uid}",
            put_with(settings::update_cc_device, |o| {
                o.summary("Update CoolerControl Device Settings")
                    .description("Updates the CoolerControl device settings for the given device UID.")
                    .tag("setting")
                    .security_requirement("CookieAuth")
                    .security_requirement("BearerAuth")
            })
            .layer(axum::middleware::from_fn(auth::auth_write_middleware)),
        )
        .api_route(
            "/settings/overrides",
            // Read-only clients may read user-defined names with a Bearer
            // access token, like /settings/ui.
            get_with(settings::get_overrides, |o| {
                o.summary("Name Overrides")
                    .description(
                        "Returns the raw, sparse user-defined name overrides document \
                        (overrides.toml). Devices and channels without an override are absent.",
                    )
                    .tag("setting")
                    .security_requirement("CookieAuth")
                    .security_requirement("BearerAuth")
            })
            .layer(axum::middleware::from_fn(auth::auth_middleware)),
        )
        .api_route(
            "/settings/devices/{device_uid}/overrides",
            put_with(settings::update_device_overrides, |o| {
                o.summary("Update Device Name Override")
                    .description(
                        "Sets the user-defined display name for the device. \
                        A null or absent name removes the override.",
                    )
                    .tag("setting")
                    .security_requirement("CookieAuth")
                    .security_requirement("BearerAuth")
            })
            .layer(axum::middleware::from_fn(auth::auth_write_middleware)),
        )
        .api_route(
            "/settings/devices/{device_uid}/channels/{channel_name}/overrides",
            put_with(settings::update_channel_overrides, |o| {
                o.summary("Update Channel Label Override")
                    .description(
                        "Sets the user-defined display label for the channel. \
                        A null or absent label removes the override. The channel does not \
                        have to be present; only the device must be known.",
                    )
                    .tag("setting")
                    .security_requirement("CookieAuth")
                    .security_requirement("BearerAuth")
            })
            .layer(axum::middleware::from_fn(auth::auth_write_middleware)),
        )
        .api_route(
            "/settings/ui",
            // Read-only: a Bearer access token (or a session) may read UI
            // settings. This is so read-only clients can use user-defined channel colors / labels
            // without an admin session.
            get_with(settings::get_ui, |o| {
                o.summary("CoolerControl UI Settings")
                    .description("Returns the current CoolerControl UI Settings.")
                    .tag("setting")
                    .security_requirement("CookieAuth")
                    .security_requirement("BearerAuth")
            })
            .layer(axum::middleware::from_fn(auth::auth_middleware)),
        )
        .api_route(
            "/settings/ui",
            // Writes remain UI session-only.
            put_with(settings::update_ui, |o| {
                o.summary("Update CoolerControl UI Settings")
                    .description("Updates and persists the CoolerControl UI settings.")
                    .tag("setting")
                    .security_requirement("CookieAuth")
            })
            .layer(axum::middleware::from_fn(auth::session_auth_middleware)),
        )
}

#[allow(clippy::too_many_lines)]
fn plugins_routes() -> ApiRouter<AppState> {
    ApiRouter::new()
        .api_route(
            "/plugins",
            get_with(plugins::get_plugins, |o| {
                o.summary("CoolerControl Plugins")
                    .description("Returns the current list of active CoolerControl plugins.")
                    .tag("plugins")
                    .security_requirement("CookieAuth")
            })
            .layer(axum::middleware::from_fn(auth::session_auth_middleware)),
        )
        .api_route(
            "/plugins/lib/cc-plugin-lib.js",
            get_with(plugins::get_cc_plugin_lib, |o| {
                o.summary("CoolerControl Plugin UI Library")
                    .description("Returns the CoolerControl plugin UI library for plugins to use in their UI code.")
                    .tag("plugins")
                    // Due to the request coming from inside an iframe, this needs to be public
            })
        )
        .api_route(
            "/plugins/{plugin_id}/config",
            get_with(plugins::get_config, |o| {
                o.summary("CoolerControl Plugin Config")
                    .description(
                        "Returns the current CoolerControl plugin config for the given plugin ID.",
                    )
                    .tag("plugins")
                    .security_requirement("CookieAuth")
            })
            .layer(axum::middleware::from_fn(auth::session_auth_middleware)),
        )
        .api_route(
            "/plugins/{plugin_id}/config",
            put_with(plugins::update_config, |o| {
                o.summary("Update CoolerControl Plugin Config")
                    .description("Updates the CoolerControl plugin config for the given plugin ID.")
                    .tag("plugins")
                    .security_requirement("CookieAuth")
            })
            .layer(axum::middleware::from_fn(auth::session_auth_middleware)),
        )
        .api_route(
            "/plugins/{plugin_id}/start",
            post_with(plugins::start_plugin, |o| {
                o.summary("Start CoolerControl Plugin")
                    .description("Starts a managed integration plugin's service.")
                    .tag("plugins")
                    .security_requirement("CookieAuth")
            })
            .layer(axum::middleware::from_fn(auth::session_auth_middleware)),
        )
        .api_route(
            "/plugins/{plugin_id}/stop",
            post_with(plugins::stop_plugin, |o| {
                o.summary("Stop CoolerControl Plugin")
                    .description("Stops a managed integration plugin's service.")
                    .tag("plugins")
                    .security_requirement("CookieAuth")
            })
            .layer(axum::middleware::from_fn(auth::session_auth_middleware)),
        )
        .api_route(
            "/plugins/{plugin_id}/restart",
            post_with(plugins::restart_plugin, |o| {
                o.summary("Restart CoolerControl Plugin")
                    .description("Restarts a managed integration plugin's service.")
                    .tag("plugins")
                    .security_requirement("CookieAuth")
            })
            .layer(axum::middleware::from_fn(auth::session_auth_middleware)),
        )
        .api_route(
            "/plugins/{plugin_id}/reload",
            post_with(plugins::reload_plugin, |o| {
                o.summary("Reload CoolerControl Plugin Manifest")
                    .description(
                        "Re-reads the manifest of a plugin that has no service to restart. \
                        A device plugin's manifest is only checked: applying it takes a daemon \
                        restart. A managed integration plugin is refused, since restarting it \
                        re-reads its manifest.",
                    )
                    .tag("plugins")
                    .security_requirement("CookieAuth")
            })
            .layer(axum::middleware::from_fn(auth::session_auth_middleware)),
        )
        .api_route(
            "/plugins/{plugin_id}/status",
            get_with(plugins::get_plugin_status, |o| {
                o.summary("CoolerControl Plugin Status")
                    .description("Returns the status of a plugin's service.")
                    .tag("plugins")
                    .security_requirement("CookieAuth")
            })
            .layer(axum::middleware::from_fn(auth::session_auth_middleware)),
        )
        .api_route(
            "/plugins/{plugin_id}/disable",
            post_with(plugins::disable_plugin, |o| {
                o.summary("Disable CoolerControl Plugin")
                    .description("Disables a plugin persistently. Stops integration services immediately.")
                    .tag("plugins")
                    .security_requirement("CookieAuth")
            })
            .layer(axum::middleware::from_fn(auth::session_auth_middleware)),
        )
        .api_route(
            "/plugins/{plugin_id}/enable",
            post_with(plugins::enable_plugin, |o| {
                o.summary("Enable CoolerControl Plugin")
                    .description("Re-enables a disabled plugin. Starts integration services immediately.")
                    .tag("plugins")
                    .security_requirement("CookieAuth")
            })
            .layer(axum::middleware::from_fn(auth::session_auth_middleware)),
        )
        .api_route(
            "/plugins/{plugin_id}/ui",
            get_with(plugins::has_ui, |o| {
                o.summary("CoolerControl Plugin UI Check")
                    .description("Returns if the CoolerControl plugin has a UI or not.")
                    .tag("plugins")
                    .security_requirement("CookieAuth")
            })
            .layer(axum::middleware::from_fn(auth::session_auth_middleware)),
        )
        .api_route(
            "/plugins/{plugin_id}/ui/{*file_path}",
            get_with(plugins::get_ui_files, |o| {
                o.summary("CoolerControl Plugin UI")
                    .description(
                        "Returns the CoolerControl plugin UI file for the given plugin ID. Supports nested paths.",
                    )
                    .tag("plugins")
                    .security_requirement("CookieAuth")
            })
            .layer(axum::middleware::from_fn(auth::session_auth_middleware)),
        )
        .api_route(
            "/plugins/{plugin_id}/data/{*data_path}",
            get_with(plugins::proxy_plugin_data, |o| {
                o.summary("CoolerControl Plugin Data Proxy")
                    .description(
                        "Reverse-proxies a request to the plugin's local HTTP server. \
                         The plugin must declare [proxy] in its manifest.",
                    )
                    .tag("plugins")
                    .security_requirement("CookieAuth")
            })
            .post_with(plugins::proxy_plugin_data, |o| {
                o.summary("CoolerControl Plugin Data Proxy (POST)")
                    .description(
                        "Reverse-proxies a POST request to the plugin's local HTTP server.",
                    )
                    .tag("plugins")
                    .security_requirement("CookieAuth")
            })
            .layer(axum::middleware::from_fn(auth::session_auth_middleware)),
        )
}

fn device_health_routes() -> ApiRouter<AppState> {
    ApiRouter::new().api_route(
        "/devices/health",
        get_with(device_health::get_all, |o| {
            o.summary("Retrieve Device Health")
                .description(
                    "Returns the failsafe channels and missing temp-source references \
                     currently tracked by the daemon.",
                )
                .tag("device")
                .security_requirement("CookieAuth")
                .security_requirement("BearerAuth")
        })
        .layer(axum::middleware::from_fn(auth::auth_middleware)),
    )
}

fn alert_routes() -> ApiRouter<AppState> {
    ApiRouter::new()
        .api_route(
            "/alerts",
            get_with(alerts::get_all, |o| {
                o.summary("Retrieve Alert List")
                    .description("Returns a list of all the persisted Alerts.")
                    .tag("alert")
                    .security_requirement("CookieAuth")
                    .security_requirement("BearerAuth")
            })
            .layer(axum::middleware::from_fn(auth::auth_middleware)),
        )
        .api_route(
            "/alerts",
            post_with(alerts::create, |o| {
                o.summary("Create Alert")
                    .description("Creates the given Alert")
                    .tag("alert")
                    .security_requirement("CookieAuth")
                    .security_requirement("BearerAuth")
            })
            .put_with(alerts::update, |o| {
                o.summary("Update Alert")
                    .description(
                        "Updates the Alert with the given properties. \
                    Dependent on the Alert UID.",
                    )
                    .tag("alert")
                    .security_requirement("CookieAuth")
                    .security_requirement("BearerAuth")
            })
            .layer(axum::middleware::from_fn(auth::auth_write_middleware)),
        )
        .api_route(
            "/alerts/{alert_uid}",
            delete_with(alerts::delete, |o| {
                o.summary("Delete Alert")
                    .description("Deletes the Alert with the given Alert UID")
                    .tag("alert")
                    .security_requirement("CookieAuth")
                    .security_requirement("BearerAuth")
            })
            .layer(axum::middleware::from_fn(auth::auth_write_middleware)),
        )
}

/// Batch-level calibration routes. Split from `calibration_routes` so
/// neither registration function exceeds the line budget. The daemon
/// owns the queue, so these are how the UI submits, polls, and cancels
/// a multi-fan calibration that survives a reload.
fn calibration_batch_routes() -> ApiRouter<AppState> {
    ApiRouter::new()
        .api_route(
            "/calibrations/batch",
            get_with(calibration::batch_status, |o| {
                o.summary("Get the calibration batch status")
                    .description(
                        "Returns the active or most recent calibration batch, or null when \
                         no batch has run this session. The daemon owns the queue, so the UI \
                         polls this and re-attaches after a reload.",
                    )
                    .tag("calibration")
                    .security_requirement("CookieAuth")
                    .security_requirement("BearerAuth")
            })
            .layer(axum::middleware::from_fn(auth::auth_middleware)),
        )
        .api_route(
            "/calibrations/batch/start",
            post_with(calibration::batch_start, |o| {
                o.summary("Start a calibration batch")
                    .description(
                        "Queues calibration of the given channels and returns 202. \
                         Returns 409 when a batch is already active or the request is invalid.",
                    )
                    .tag("calibration")
                    .security_requirement("CookieAuth")
                    .security_requirement("BearerAuth")
            })
            .layer(axum::middleware::from_fn(auth::auth_write_middleware)),
        )
        .api_route(
            "/calibrations/batch/cancel",
            post_with(calibration::batch_cancel, |o| {
                o.summary("Cancel the calibration batch")
                    .description(
                        "Cancels the active batch and stops its queue. Returns 404 when no \
                         batch is active.",
                    )
                    .tag("calibration")
                    .security_requirement("CookieAuth")
                    .security_requirement("BearerAuth")
            })
            .layer(axum::middleware::from_fn(auth::auth_write_middleware)),
        )
}

fn calibration_routes() -> ApiRouter<AppState> {
    ApiRouter::new()
        .api_route(
            "/calibrations",
            get_with(calibration::list, |o| {
                o.summary("List every persisted calibration")
                    .description(
                        "Returns one entry per channel that has a stored calibration. \
                         Used by the UI at app load to mark calibrated channels in the \
                         tree menu without one request per channel. Always 200; an empty \
                         list signals that no channel has been calibrated yet.",
                    )
                    .tag("calibration")
                    .security_requirement("CookieAuth")
                    .security_requirement("BearerAuth")
            })
            .layer(axum::middleware::from_fn(auth::auth_middleware)),
        )
        .api_route(
            "/calibrations/{device_uid}/channels/{channel_name}/start",
            post_with(calibration::start, |o| {
                o.summary("Start a calibration diagnosis")
                    .description(
                        "Queues a calibration diagnosis on the channel and returns 202. \
                         Poll GET .../status to track progress. \
                         Returns 409 when a diagnosis is already in flight for the channel.",
                    )
                    .tag("calibration")
                    .security_requirement("CookieAuth")
                    .security_requirement("BearerAuth")
            })
            .layer(axum::middleware::from_fn(auth::auth_write_middleware)),
        )
        .api_route(
            "/calibrations/{device_uid}/channels/{channel_name}/cancel",
            post_with(calibration::cancel, |o| {
                o.summary("Cancel an in-flight calibration")
                    .description(
                        "Triggers cancellation of the in-flight calibration for the \
                         channel. Returns 404 when nothing is running.",
                    )
                    .tag("calibration")
                    .security_requirement("CookieAuth")
                    .security_requirement("BearerAuth")
            })
            .layer(axum::middleware::from_fn(auth::auth_write_middleware)),
        )
        .api_route(
            "/calibrations/{device_uid}/channels/{channel_name}/status",
            get_with(calibration::status, |o| {
                o.summary("Get the calibration status")
                    .description(
                        "Returns the most recent calibration status for the channel \
                         (in_progress / completed / failed). 404 if no diagnosis has \
                         ever been observed. Intended for UI polling at ~1 Hz while \
                         a diagnosis is in flight.",
                    )
                    .tag("calibration")
                    .security_requirement("CookieAuth")
                    .security_requirement("BearerAuth")
            })
            .layer(axum::middleware::from_fn(auth::auth_middleware)),
        )
        .api_route(
            "/calibrations/{device_uid}/channels/{channel_name}",
            get_with(calibration::get, |o| {
                o.summary("Get the stored calibration")
                    .description("Returns the persisted calibration JSON, or 404 if none.")
                    .tag("calibration")
                    .security_requirement("CookieAuth")
                    .security_requirement("BearerAuth")
            })
            .delete_with(calibration::delete, |o| {
                o.summary("Delete the stored calibration")
                    .description(
                        "Removes the persisted calibration for the channel. \
                         Returns 404 when none was stored.",
                    )
                    .tag("calibration")
                    .security_requirement("CookieAuth")
                    .security_requirement("BearerAuth")
            })
            .layer(axum::middleware::from_fn(auth::auth_middleware)),
        )
        .api_route(
            "/calibrations/{device_uid}/channels/{channel_name}/overrides",
            patch_with(calibration::set_overrides, |o| {
                o.summary("Set per-fan calibration overrides")
                    .description(
                        "Replaces the kick-boost and kick-duration override fields on \
                         the stored calibration. Both fields in the body are applied \
                         unconditionally; `null` clears the override. Returns 404 when \
                         no calibration is stored for the channel.",
                    )
                    .tag("calibration")
                    .security_requirement("CookieAuth")
                    .security_requirement("BearerAuth")
            })
            .layer(axum::middleware::from_fn(auth::auth_write_middleware)),
        )
}

/// The calibration duty map. Split from `calibration_routes` so neither
/// registration function exceeds the line budget.
fn calibration_map_routes() -> ApiRouter<AppState> {
    ApiRouter::new().api_route(
        "/calibrations/{device_uid}/channels/{channel_name}/map",
        // Read auth: this computes and stores nothing, and exposes no
        // more than GET on the same calibration already does.
        post_with(calibration::map_duties, |o| {
            o.summary("Map device duties to true duties")
                .description(
                    "Runs each device duty through the calibration's stable inverse and \
                     returns the true duties that reproduce it, in the same order. A \
                     calibrated channel reinterprets stored duties as true-duty, so a \
                     curve authored before calibration behaves differently; feeding its \
                     points through here yields values that restore the old behavior. \
                     Computes nothing else and stores nothing. Returns 404 when the \
                     channel has no stored calibration, and 409 for a stepped \
                     calibration, which is written through unmapped and so has nothing \
                     to convert. Rejects an empty list, more than 256 entries, or any \
                     duty over 100.",
                )
                .tag("calibration")
                .security_requirement("CookieAuth")
                .security_requirement("BearerAuth")
        })
        .layer(axum::middleware::from_fn(auth::auth_middleware)),
    )
}

fn detect_routes() -> ApiRouter<AppState> {
    ApiRouter::new()
        .api_route(
            "/detect",
            get_with(detect::get_detect, |o| {
                o.summary("Detect Hardware")
                    .description(
                        "Return what the startup Super-I/O detection found, retained rather than \
                         re-probed. `probed` is false when no detection ran at all, so an empty \
                         chip list means \"not looked for\" rather than \"none present\".",
                    )
                    .tag("detect")
                    .security_requirement("CookieAuth")
                    .security_requirement("BearerAuth")
            })
            .layer(axum::middleware::from_fn(auth::auth_middleware)),
        )
        .api_route(
            "/detect",
            post_with(detect::post_detect, |o| {
                o.summary("Detect Hardware and Load Modules")
                    .description(
                        "Run Super-I/O hardware detection and optionally load kernel modules.",
                    )
                    .tag("detect")
                    .security_requirement("CookieAuth")
                    .security_requirement("BearerAuth")
            })
            .layer(axum::middleware::from_fn(auth::auth_write_middleware)),
        )
}

fn hardware_report_routes() -> ApiRouter<AppState> {
    ApiRouter::new().api_route(
        "/hardware-report",
        get_with(hardware_report::get_hardware_report, |o| {
            o.summary("Hardware Support Report")
                .description(
                    "Returns a paste-ready hardware support report as plain text. `full` adds \
                     the whole hwmon tree; the compact default is trimmed to stay pasteable. \
                     Includes liquidctl devices, which only the running daemon can enumerate \
                     without touching hardware.",
                )
                .tag("devices")
                .security_requirement("CookieAuth")
                .security_requirement("BearerAuth")
        })
        .layer(axum::middleware::from_fn(auth::auth_middleware)),
    )
}

#[allow(clippy::too_many_lines)] // Declarative route registration; splitting hurts readability.
fn stress_test_routes() -> ApiRouter<AppState> {
    ApiRouter::new()
        .api_route(
            "/stress-test/cpu",
            post_with(stress_test::start_cpu, |o| {
                o.summary("Start CPU Stress Test")
                    .description(
                        "Spawns a CPU stress subprocess with tight FMA loops \
                         on the requested number of threads.",
                    )
                    .tag("stress-test")
                    .security_requirement("CookieAuth")
            })
            .delete_with(stress_test::stop_cpu, |o| {
                o.summary("Stop CPU Stress Test")
                    .description("Stops the running CPU stress test.")
                    .tag("stress-test")
                    .security_requirement("CookieAuth")
            })
            .layer(axum::middleware::from_fn(auth::session_auth_middleware)),
        )
        .api_route(
            "/stress-test/gpu",
            post_with(stress_test::start_gpu, |o| {
                o.summary("Start GPU Stress Test")
                    .description(
                        "Spawns a GPU stress subprocess using wgpu compute \
                         shaders (Vulkan or OpenGL ES via ANGLE).",
                    )
                    .tag("stress-test")
                    .security_requirement("CookieAuth")
            })
            .delete_with(stress_test::stop_gpu, |o| {
                o.summary("Stop GPU Stress Test")
                    .description("Stops the running GPU stress test.")
                    .tag("stress-test")
                    .security_requirement("CookieAuth")
            })
            .layer(axum::middleware::from_fn(auth::session_auth_middleware)),
        )
        .api_route(
            "/stress-test/gpus",
            get_with(stress_test::list_gpus, |o| {
                o.summary("List Available GPUs")
                    .description(
                        "Returns the GPUs available for stress testing, discrete first. \
                         Enumerated out of process on first use, then cached.",
                    )
                    .tag("stress-test")
                    .security_requirement("CookieAuth")
            })
            .layer(axum::middleware::from_fn(auth::session_auth_middleware)),
        )
        .api_route(
            "/stress-test/ram",
            post_with(stress_test::start_ram, |o| {
                o.summary("Start RAM Stress Test")
                    .description(
                        "Spawns a RAM stress subprocess that streams data \
                         through system memory to maximize bandwidth.",
                    )
                    .tag("stress-test")
                    .security_requirement("CookieAuth")
            })
            .delete_with(stress_test::stop_ram, |o| {
                o.summary("Stop RAM Stress Test")
                    .description("Stops the running RAM stress test.")
                    .tag("stress-test")
                    .security_requirement("CookieAuth")
            })
            .layer(axum::middleware::from_fn(auth::session_auth_middleware)),
        )
        .api_route(
            "/stress-test/drive",
            post_with(stress_test::start_drive, |o| {
                o.summary("Start Drive Stress Test")
                    .description(
                        "Spawns a drive stress subprocess that performs random \
                         read-only I/O on the specified block device using O_DIRECT.",
                    )
                    .tag("stress-test")
                    .security_requirement("CookieAuth")
            })
            .delete_with(stress_test::stop_drive, |o| {
                o.summary("Stop Drive Stress Test")
                    .description("Stops the running drive stress test.")
                    .tag("stress-test")
                    .security_requirement("CookieAuth")
            })
            .layer(axum::middleware::from_fn(auth::session_auth_middleware)),
        )
        .api_route(
            "/stress-test/drives",
            get_with(stress_test::list_drives, |o| {
                o.summary("List Available Drives")
                    .description("Returns block devices available for drive stress testing.")
                    .tag("stress-test")
                    .security_requirement("CookieAuth")
            })
            .layer(axum::middleware::from_fn(auth::session_auth_middleware)),
        )
        .api_route(
            "/stress-test",
            get_with(stress_test::status, |o| {
                o.summary("Stress Test Status")
                    .description(
                        "Returns the current status of CPU, GPU, RAM, and Drive stress tests.",
                    )
                    .tag("stress-test")
                    .security_requirement("CookieAuth")
            })
            .delete_with(stress_test::stop_all, |o| {
                o.summary("Stop All Stress Tests")
                    .description("Stops all running stress tests (CPU, GPU, RAM, and Drive).")
                    .tag("stress-test")
                    .security_requirement("CookieAuth")
            })
            .layer(axum::middleware::from_fn(auth::session_auth_middleware)),
        )
}

fn metrics_routes() -> ApiRouter<AppState> {
    ApiRouter::new().api_route(
        "/metrics",
        get_with(metrics::get_metrics, |o| {
            o.summary("Prometheus Metrics")
                .description("Returns device sensor data in Prometheus text exposition format.")
                .tag("metrics")
                .security_requirement("CookieAuth")
                .security_requirement("BearerAuth")
        })
        .layer(axum::middleware::from_fn(auth::auth_middleware)),
    )
}

fn sse_routes() -> ApiRouter<AppState> {
    ApiRouter::new()
        .api_route(
            "/sse",
            get_with(sse::combined, |o| {
                o.summary("Server Sent Events")
                    .description(
                        "Subscribes to every event stream on one connection. Optional \
                         `events` query parameter narrows the subscription to a \
                         comma-separated subset of: status, health, logs, modes, alerts, \
                         notifications, system. Prefer this over the per-stream endpoints: \
                         browsers cap concurrent connections per origin over HTTP/1.1.",
                    )
                    .tag("sse")
                    .response::<200, sse::SseStream>()
                    .security_requirement("CookieAuth")
                    .security_requirement("BearerAuth")
            })
            .layer(axum::middleware::from_fn(auth::auth_middleware)),
        )
        .merge(legacy_sse_routes())
}

/// The per-stream endpoints `/sse` replaced. Split out so neither registration function
/// exceeds the line budget, and so the deprecated set is removable as one unit.
/// DOWNGRADE-COMPAT(added 5.0.0, remove 5.2.0): see DEPRECATIONS.md.
fn legacy_sse_routes() -> ApiRouter<AppState> {
    ApiRouter::new()
        .api_route(
            "/sse/logs",
            get_with(sse::logs, |o| {
                o.summary("Log Server Sent Events")
                    .description(
                        "Deprecated: use GET /sse?events=logs. Subscribes and returns the \
                         Server Sent Events for a Log stream",
                    )
                    .tag("sse")
                    .security_requirement("CookieAuth")
                    .security_requirement("BearerAuth")
            })
            .layer(axum::middleware::from_fn(auth::auth_middleware)),
        )
        .api_route(
            "/sse/status",
            get_with(sse::status, |o| {
                o.summary("Recent Status Server Sent Events")
                    .description(
                        "Deprecated: use GET /sse?events=status,health. Subscribes and returns \
                         the Server Sent Events for a Status stream",
                    )
                    .tag("sse")
                    .security_requirement("CookieAuth")
                    .security_requirement("BearerAuth")
            })
            .layer(axum::middleware::from_fn(auth::auth_middleware)),
        )
        .api_route(
            "/sse/modes",
            get_with(sse::modes, |o| {
                o.summary("Activated Mode Events")
                    .description(
                        "Deprecated: use GET /sse?events=modes. Subscribes and returns the \
                         Server Sent Events for a ModeActivated stream",
                    )
                    .tag("sse")
                    .security_requirement("CookieAuth")
                    .security_requirement("BearerAuth")
            })
            .layer(axum::middleware::from_fn(auth::auth_middleware)),
        )
        .merge(legacy_sse_event_routes())
}

/// The remaining deprecated per-stream endpoints, split from `legacy_sse_routes` purely
/// to stay inside the line budget. Removed with it.
/// DOWNGRADE-COMPAT(added 5.0.0, remove 5.2.0): see DEPRECATIONS.md.
fn legacy_sse_event_routes() -> ApiRouter<AppState> {
    ApiRouter::new()
        .api_route(
            "/sse/alerts",
            get_with(sse::alerts, |o| {
                o.summary("Alert Events")
                    .description(
                        "Deprecated: use GET /sse?events=alerts. Subscribes and returns Events \
                         for when an Alert State has changed",
                    )
                    .tag("sse")
                    .security_requirement("CookieAuth")
                    .security_requirement("BearerAuth")
            })
            .layer(axum::middleware::from_fn(auth::auth_middleware)),
        )
        .api_route(
            "/sse/notifications",
            get_with(sse::notifications, |o| {
                o.summary("Desktop Notification Events")
                    .description(
                        "Deprecated: use GET /sse?events=notifications. Subscribes and returns \
                         Events for desktop notifications that should be displayed to the user",
                    )
                    .tag("sse")
                    .security_requirement("CookieAuth")
                    .security_requirement("BearerAuth")
            })
            .layer(axum::middleware::from_fn(auth::auth_middleware)),
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::actor::TokenHandle;
    use crate::api::auth;
    use crate::api::session_store::MemorySessionStore;
    use crate::token::{self, StoredToken};
    use axum::Extension;
    use chrono::Local;
    use tokio::net::TcpListener;
    use tonic::Code;
    use tonic_health::pb::health_client::HealthClient;
    use tonic_health::pb::HealthCheckRequest;
    use tower::ServiceExt;
    use tower_http::timeout::TimeoutLayer;
    use tower_sessions::SessionManagerLayer;

    fn expired_token(raw: &str) -> StoredToken {
        StoredToken {
            expires_at: Some(Local::now() - chrono::Duration::hours(1)),
            ..stored_token(raw)
        }
    }

    fn stored_token(raw: &str) -> StoredToken {
        StoredToken {
            id: "grpc-test".to_string(),
            label: "gRPC Test".to_string(),
            hash: token::hash_token(raw).unwrap(),
            digest: Some(token::digest_token(raw)),
            created_at: Local::now(),
            expires_at: None,
            last_used: None,
            write_access: false,
        }
    }

    /// Mounts the health service the way `grpc_routes` and `init` mount the real ones:
    /// same route pattern, same read-tier auth middleware, same session layer, and the
    /// same outermost gRPC error translation. The device service needs live actor
    /// handles, so health stands in for it; the mounting and transport path under test
    /// is identical.
    ///
    /// The layer order here mirrors `init` by hand. If a layer is added there, it must
    /// be added here too, or these tests stop covering the real stack.
    async fn serve(token_handle: TokenHandle) -> String {
        let router = axum::Router::new()
            .route_service(
                &grpc_api::health_service_route(),
                grpc_api::health_service().await,
            )
            .layer(axum::middleware::from_fn(auth::auth_middleware))
            .layer(Extension(token_handle))
            .layer(SessionManagerLayer::new(MemorySessionStore::new(4)))
            .layer(axum::middleware::from_fn(grpc_error_middleware));
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        format!("http://{address}")
    }

    /// Goal: `/set-passwd` counts a wrong current password against the peer's password
    /// budget, but not a request refused for lacking a session, which guesses nothing.
    /// Method: the route's two layers in the order `auth_routes` applies them, mirrored by
    /// hand around a handler that always refuses the password, like the gRPC `serve` above.
    #[tokio::test]
    async fn set_passwd_throttle_sits_inside_the_session_check() {
        use axum::extract::ConnectInfo;
        use axum::routing::{get, post};
        use std::net::SocketAddr;
        use tower_sessions::Session;

        let app = axum::Router::new()
            .route(
                "/grant",
                get(|session: Session| async move { auth::grant_admin_session(&session).await }),
            )
            .route(
                "/set-passwd",
                post(|| async { StatusCode::UNAUTHORIZED })
                    .layer(axum::middleware::from_fn(
                        auth_throttle::password_throttle_middleware,
                    ))
                    .layer(axum::middleware::from_fn(auth::session_auth_middleware)),
            )
            .layer(SessionManagerLayer::new(MemorySessionStore::new(4)));
        // A TEST-NET peer, allotted beside the throttle statics in `auth_throttle`.
        let peer = SocketAddr::from(([198, 51, 100, 20], 40000));
        let set_passwd = |cookie: Option<&str>| {
            let mut builder = Request::builder()
                .method("POST")
                .uri("/set-passwd")
                .header(header::AUTHORIZATION, "Basic Q0NBZG1pbjpuZXc=");
            if let Some(value) = cookie {
                builder = builder.header(header::COOKIE, value);
            }
            let mut request = builder.body(axum::body::Body::empty()).unwrap();
            request.extensions_mut().insert(ConnectInfo(peer));
            request
        };

        for _ in 0..10 {
            let response = app.clone().oneshot(set_passwd(None)).await.unwrap();
            assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        }
        assert_eq!(auth_throttle::password_failures(peer.ip()), None);

        let granted = app
            .clone()
            .oneshot(
                Request::get("/grant")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let set_cookie = granted.headers()[header::SET_COOKIE].to_str().unwrap();
        let cookie = set_cookie.split(';').next().unwrap();
        let response = app.oneshot(set_passwd(Some(cookie))).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(auth_throttle::password_failures(peer.ip()), Some(1));
    }

    /// Sends a Basic `/login` on a new pre-auth remote connection, and returns its status
    /// and whether it promoted the connection.
    async fn login(app: &axum::Router, credentials: &str) -> (StatusCode, bool) {
        use axum::extract::ConnectInfo;
        // A TEST-NET peer, allotted beside the throttle statics in `auth_throttle`.
        let peer = std::net::SocketAddr::from(([198, 51, 100, 21], 40000));
        let connection = connection::AdmittedConnection::remote_for_test();
        let mut request = Request::post("/login")
            .header(header::AUTHORIZATION, format!("Basic {credentials}"))
            .body(axum::body::Body::empty())
            .unwrap();
        request.extensions_mut().insert(ConnectInfo(peer));
        request.extensions_mut().insert(connection.clone());
        let response = app.clone().oneshot(request).await.unwrap();
        (response.status(), connection.is_authenticated())
    }

    /// Goal: the real `/login` promotes its connection out of the pre-auth pool, and only
    /// when the password is right. Method: `auth_routes` over an empty app state, sent a
    /// wrong password and then the default one.
    #[test]
    #[serial_test::serial(modes_file)]
    fn real_login_promotes_only_on_success() {
        crate::rt::test_runtime(async {
            let cancel_token = tokio_util::sync::CancellationToken::new();
            moro_local::async_scope!(|main_scope| -> anyhow::Result<()> {
                let state = crate::api::empty_app_state(&cancel_token, main_scope).await;
                let routes = axum::Router::from(auth_routes().with_state(state))
                    .layer(SessionManagerLayer::new(MemorySessionStore::new(4)));
                let app = crate::api::with_client_addr(routes, std::sync::Arc::default());
                // "CCAdmin:x", then "CCAdmin:coolAdmin".
                let (status, promoted) = login(&app, "Q0NBZG1pbjp4").await;
                assert_eq!(status, StatusCode::UNAUTHORIZED);
                assert!(promoted.not());
                let (status, promoted) = login(&app, "Q0NBZG1pbjpjb29sQWRtaW4=").await;
                assert_eq!(status, StatusCode::OK);
                assert!(promoted);
                // Stops the actors so the scope can finish.
                cancel_token.cancel();
                Ok(())
            })
            .await
            .unwrap();
        });
    }

    /// Goal: the load-bearing claim of the whole design. gRPC is served from the REST
    /// listener over plaintext h2c prior-knowledge, with no `tonic::transport::Server`
    /// and no second port. If hyper's auto builder ever stopped sniffing the HTTP/2
    /// preface, every gRPC client would break and only this test would notice.
    #[tokio::test]
    async fn grpc_is_served_over_h2c_from_the_rest_listener() {
        let raw = token::generate_token();
        let address = serve(TokenHandle::with_tokens(vec![stored_token(&raw)])).await;

        let channel = tonic::transport::Endpoint::new(address)
            .unwrap()
            .connect()
            .await
            .expect("h2c prior-knowledge connect must succeed");
        let mut client = HealthClient::new(channel);
        let mut request = tonic::Request::new(HealthCheckRequest {
            service: String::new(),
        });
        request
            .metadata_mut()
            .insert("authorization", format!("Bearer {raw}").parse().unwrap());

        let response = client.check(request).await.expect("authenticated call");
        assert_eq!(response.into_inner().status, 1); // SERVING
    }

    /// Goal: the gap this branch exists to close. An unauthenticated peer must not reach
    /// the gRPC surface, on loopback included.
    #[tokio::test]
    async fn grpc_without_credentials_is_rejected() {
        let raw = token::generate_token();
        let address = serve(TokenHandle::with_tokens(vec![stored_token(&raw)])).await;

        let channel = tonic::transport::Endpoint::new(address)
            .unwrap()
            .connect()
            .await
            .unwrap();
        let mut client = HealthClient::new(channel);
        let status = client
            .check(HealthCheckRequest {
                service: String::new(),
            })
            .await
            .expect_err("an unauthenticated call must be refused");
        assert_eq!(status.code(), Code::Unauthenticated);
    }

    /// Goal: a token that was never minted here is refused just like no token, so the
    /// gRPC surface cannot be reached by guessing. Expiry is covered separately, since a
    /// token that exists but has lapsed takes a different path through the store.
    #[tokio::test]
    async fn grpc_with_an_unknown_token_is_rejected() {
        let raw = token::generate_token();
        let address = serve(TokenHandle::with_tokens(vec![stored_token(&raw)])).await;

        let channel = tonic::transport::Endpoint::new(address)
            .unwrap()
            .connect()
            .await
            .unwrap();
        let mut client = HealthClient::new(channel);
        let mut request = tonic::Request::new(HealthCheckRequest {
            service: String::new(),
        });
        request.metadata_mut().insert(
            "authorization",
            format!("Bearer {}", token::generate_token())
                .parse()
                .unwrap(),
        );

        let status = client
            .check(request)
            .await
            .expect_err("an unknown token must be refused");
        assert_eq!(status.code(), Code::Unauthenticated);
    }

    /// Goal: plan criterion 2's other half. A token this daemon really did mint, but whose
    /// `expires_at` has passed, must be refused on the gRPC surface exactly like an
    /// unknown one. The digest fast path matches an expired token before expiry is
    /// checked, so a store hit is not on its own a grant.
    #[tokio::test]
    async fn grpc_with_an_expired_token_is_rejected() {
        let raw = token::generate_token();
        let address = serve(TokenHandle::with_tokens(vec![expired_token(&raw)])).await;

        let channel = tonic::transport::Endpoint::new(address)
            .unwrap()
            .connect()
            .await
            .unwrap();
        let mut client = HealthClient::new(channel);
        let mut request = tonic::Request::new(HealthCheckRequest {
            service: String::new(),
        });
        request
            .metadata_mut()
            .insert("authorization", format!("Bearer {raw}").parse().unwrap());

        let status = client
            .check(request)
            .await
            .expect_err("an expired token must be refused");
        assert_eq!(status.code(), Code::Unauthenticated);
    }

    /// Goal: only the statuses our own layers emit are rewritten. Reinterpreting an
    /// arbitrary status would turn an unrelated failure into a misleading gRPC code.
    #[test]
    fn only_our_own_error_statuses_map_to_grpc_codes() {
        assert_eq!(
            grpc_code_for(StatusCode::UNAUTHORIZED).map(|(code, _)| code),
            Some(tonic::Code::Unauthenticated)
        );
        assert_eq!(
            grpc_code_for(StatusCode::FORBIDDEN).map(|(code, _)| code),
            Some(tonic::Code::PermissionDenied)
        );
        assert_eq!(
            grpc_code_for(StatusCode::TOO_MANY_REQUESTS).map(|(code, _)| code),
            Some(tonic::Code::ResourceExhausted)
        );
        assert_eq!(
            grpc_code_for(StatusCode::REQUEST_TIMEOUT).map(|(code, _)| code),
            Some(tonic::Code::DeadlineExceeded)
        );
        assert_eq!(
            grpc_code_for(StatusCode::NOT_FOUND).map(|(code, _)| code),
            Some(tonic::Code::Unimplemented)
        );
        assert_eq!(grpc_code_for(StatusCode::OK), None);
        assert_eq!(grpc_code_for(StatusCode::INTERNAL_SERVER_ERROR), None);
        assert_eq!(grpc_code_for(StatusCode::BAD_GATEWAY), None);
    }

    /// Goal: the request timeout must reach a gRPC client as `DeadlineExceeded`. This is
    /// a layer-ordering test, not a mapping test: `TimeoutLayer` wraps the router in
    /// `create_api_server`, so the translation only ever sees a 408 while it sits outside
    /// that layer. Nest it the other way and this fails with a bare 408 and no
    /// `grpc-status`, which is what a user would have got.
    #[tokio::test]
    async fn a_timed_out_grpc_request_comes_back_as_deadline_exceeded() {
        async fn too_slow() -> StatusCode {
            tokio::time::sleep(std::time::Duration::from_secs(30)).await;
            StatusCode::OK
        }

        let router = axum::Router::new()
            .route("/slow", axum::routing::get(too_slow))
            .layer(TimeoutLayer::with_status_code(
                StatusCode::REQUEST_TIMEOUT,
                std::time::Duration::from_millis(10),
            ))
            .layer(axum::middleware::from_fn(grpc_error_middleware));

        let request = Request::builder()
            .uri("/slow")
            .header(header::CONTENT_TYPE, "application/grpc")
            .body(axum::body::Body::empty())
            .unwrap();
        let response = router.oneshot(request).await.unwrap();

        assert_eq!(
            response
                .headers()
                .get("grpc-status")
                .and_then(|v| v.to_str().ok()),
            Some((tonic::Code::DeadlineExceeded as i32).to_string().as_str()),
            "a 408 must be translated, not passed through as an HTTP status"
        );
    }

    /// Goal: the translation only fires for gRPC traffic, so a browser hitting the same
    /// port keeps getting ordinary JSON errors.
    #[test]
    fn grpc_content_type_detection_is_exact() {
        let mut headers = HeaderMap::new();
        assert!(is_grpc_content_type(&headers).not());
        headers.insert(header::CONTENT_TYPE, "application/json".parse().unwrap());
        assert!(is_grpc_content_type(&headers).not());
        headers.insert(header::CONTENT_TYPE, "application/grpc".parse().unwrap());
        assert!(is_grpc_content_type(&headers));
        headers.insert(
            header::CONTENT_TYPE,
            "application/grpc+proto".parse().unwrap(),
        );
        assert!(is_grpc_content_type(&headers));
    }
}
