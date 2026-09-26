// SPDX-FileCopyrightText: 2023 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use crate::api::actor::CalibrationHandle;
use crate::api::{handle_error, AppState, CCError};
use crate::calibration::{effective_speed_options, Calibration, ChannelKey};
use crate::device::{ChannelName, DeviceInfo, DeviceType, DeviceUID, LcInfo, UID};
use crate::engine::processors::image;
use crate::setting::{LcdModeName, LcdSettings, LightingSettings, Setting};
use crate::Device;
use aide::axum::IntoApiResponse;
use axum::extract::multipart::Field;
use axum::extract::{Multipart, Path, Query, State};
use axum::http::header;
use axum::Json;
use mime::Mime;
use schemars::JsonSchema;
use serde::{de, Deserialize, Deserializer, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::ops::Not;
use std::str::FromStr;

/// Returns a list of all detected devices and their associated information.
/// Does not return Status, that's for another more-fine-grained endpoint
pub async fn get(
    State(AppState {
        device_handle,
        calibration_handle,
        ..
    }): State<AppState>,
) -> Result<Json<DevicesResponse>, CCError> {
    let mut all_devices = device_handle.devices_get().await?;
    let cal_map = build_calibration_map(&calibration_handle).await;
    apply_effective_speed_options(&mut all_devices, &cal_map);
    Ok(Json(DevicesResponse {
        devices: all_devices,
    }))
}

/// Snapshot of every persisted calibration keyed by `(device_uid,
/// channel_name)`. Built once per request that needs to surface
/// calibration-aware channel state so the per-channel adjustment loop
/// is O(1) per channel instead of O(N) actor round-trips.
pub async fn build_calibration_map(
    calibration_handle: &CalibrationHandle,
) -> HashMap<ChannelKey, Calibration> {
    let entries = calibration_handle.get_all().await;
    let mut map = HashMap::with_capacity(entries.len());
    for entry in entries {
        map.insert((entry.device_uid, entry.channel_name), entry.calibration);
    }
    map
}

/// Overwrite each channel's `speed_options.min_duty` / `max_duty` with
/// the calibration-aware effective range. Channels without a persisted
/// calibration (or whose calibration is `Stepped` / passthrough) keep
/// their raw device limits.
///
/// Applied at every API serialization boundary so external clients see
/// a consistent, calibration-aware range without having to fetch and
/// merge calibration state themselves.
pub fn apply_effective_speed_options(
    devices: &mut [DeviceDto],
    cal_map: &HashMap<ChannelKey, Calibration>,
) {
    for device in &mut *devices {
        for (channel_name, channel_info) in &mut device.info.channels {
            if let Some(so) = channel_info.speed_options_mut() {
                let key: ChannelKey = (device.uid.clone(), channel_name.clone());
                *so = effective_speed_options(so, cal_map.get(&key));
            }
        }
    }
}

/// Returns all the currently applied settings for the given device.
/// It returns the Config Settings model, which includes all possibilities for each channel.
pub async fn device_settings_get(
    Path(path): Path<DevicePath>,
    State(AppState { device_handle, .. }): State<AppState>,
) -> Result<Json<SettingsResponse>, CCError> {
    device_handle
        .device_settings_get(path.device_uid)
        .await
        .map(|settings| Json(SettingsResponse { settings }))
        .map_err(handle_error)
}

pub async fn device_setting_manual_modify(
    Path(path): Path<DeviceChannelPath>,
    State(AppState {
        device_handle,
        calibration_handle,
        ..
    }): State<AppState>,
    Json(manual_request): Json<SettingManualRequest>,
) -> Result<(), CCError> {
    reject_when_calibrating(&calibration_handle, &path).await?;
    device_handle
        .device_setting_manual(
            path.device_uid,
            path.channel_name,
            manual_request.speed_fixed,
        )
        .await
        .map_err(handle_error)
}

pub async fn device_setting_profile_modify(
    Path(path): Path<DeviceChannelPath>,
    State(AppState {
        device_handle,
        calibration_handle,
        ..
    }): State<AppState>,
    Json(profile_uid_json): Json<SettingProfileUID>,
) -> Result<(), CCError> {
    reject_when_calibrating(&calibration_handle, &path).await?;
    device_handle
        .device_setting_profile(
            path.device_uid,
            path.channel_name,
            profile_uid_json.profile_uid,
        )
        .await
        .map_err(handle_error)
}

/// Returns `Err(CCError::Conflict)` when a calibration diagnosis is
/// currently in flight for the channel addressed by `path`. The
/// channel is owned by the diagnoser during a sweep; manual and
/// profile updates would race with the sweep's writes and silently
/// no-op in the dispatch path, so we reject them at the API edge.
async fn reject_when_calibrating(
    calibration_handle: &crate::api::actor::CalibrationHandle,
    path: &DeviceChannelPath,
) -> Result<(), CCError> {
    if calibration_handle
        .in_progress(path.device_uid.clone(), path.channel_name.clone())
        .await
    {
        return Err(CCError::Conflict {
            msg: format!(
                "calibration in progress for channel '{}'; cannot change settings",
                path.channel_name
            ),
        });
    }
    Ok(())
}

pub async fn device_setting_lcd_modify(
    Path(path): Path<DeviceChannelPath>,
    Query(lcd_update_query): Query<LcdUpdateQuery>,
    State(AppState { device_handle, .. }): State<AppState>,
    Json(lcd_settings): Json<LcdSettings>,
) -> Result<(), CCError> {
    let log_success = lcd_update_query.log.unwrap_or(true);
    device_handle
        .device_setting_lcd(
            path.device_uid,
            path.channel_name,
            lcd_settings,
            log_success,
        )
        .await
        .map_err(handle_error)
}

/// To retrieve the currently applied image
pub async fn get_device_lcd_image(
    Path(path): Path<DeviceChannelPath>,
    State(AppState { device_handle, .. }): State<AppState>,
) -> Result<impl IntoApiResponse, CCError> {
    let (content_type, image_data) = device_handle
        .device_image_get(path.device_uid, path.channel_name)
        .await?;
    Ok((
        [(header::CONTENT_TYPE, content_type.to_string())],
        image_data,
    ))
}

/// Used to apply LCD settings that contain images.
pub async fn update_device_setting_lcd_image(
    Path(path): Path<DeviceChannelPath>,
    Query(lcd_image_update_query): Query<LcdImageUpdateQuery>,
    State(AppState { device_handle, .. }): State<AppState>,
    multipart: Multipart,
) -> Result<(), CCError> {
    let form = LcdImageForm::parse(multipart).await?;
    let log_success = lcd_image_update_query.log.unwrap_or(true);
    let mode = form.lcd_mode()?;
    device_handle
        .device_image_update(
            path.device_uid,
            path.channel_name,
            mode,
            form.brightness,
            form.orientation,
            form.images,
            log_success,
        )
        .await
        .map_err(handle_error)
}

/// Used to process image files for previewing
pub async fn process_device_lcd_images(
    Path(path): Path<DeviceChannelPath>,
    State(AppState { device_handle, .. }): State<AppState>,
    multipart: Multipart,
) -> Result<impl IntoApiResponse, CCError> {
    let form = LcdImageForm::parse(multipart).await?;
    device_handle
        .device_image_process(path.device_uid, path.channel_name, form.images)
        .await
        .map(|(content_type, file_data)| {
            (
                [(header::CONTENT_TYPE, content_type.to_string())],
                file_data,
            )
        })
        .map_err(handle_error)
}

/// Upload and save an LCD image to display when the daemon shuts down.
pub async fn set_device_lcd_shutdown_image(
    Path(path): Path<DeviceChannelPath>,
    State(AppState { device_handle, .. }): State<AppState>,
    multipart: Multipart,
) -> Result<(), CCError> {
    let form = LcdImageForm::parse(multipart).await?;
    let mode = form.lcd_mode()?;
    device_handle
        .device_set_lcd_shutdown_image(
            path.device_uid,
            path.channel_name,
            mode,
            form.brightness,
            form.orientation,
            form.images,
        )
        .await
        .map_err(handle_error)
}

/// Remove the saved LCD shutdown image for the given device channel.
pub async fn delete_device_lcd_shutdown_image(
    Path(path): Path<DeviceChannelPath>,
    State(AppState { device_handle, .. }): State<AppState>,
) -> Result<(), CCError> {
    device_handle
        .device_clear_lcd_shutdown_image(path.device_uid, path.channel_name)
        .await
        .map_err(handle_error)
}

pub async fn device_setting_lighting_modify(
    Path(path): Path<DeviceChannelPath>,
    State(AppState { device_handle, .. }): State<AppState>,
    Json(lighting_settings): Json<LightingSettings>,
) -> Result<(), CCError> {
    device_handle
        .device_setting_lighting(path.device_uid, path.channel_name, lighting_settings)
        .await
        .map_err(handle_error)
}

pub async fn device_setting_pwm_mode_modify(
    Path(path): Path<DeviceChannelPath>,
    State(AppState { device_handle, .. }): State<AppState>,
    Json(pwm_mode_json): Json<SettingPWMMode>,
) -> Result<(), CCError> {
    device_handle
        .device_setting_pwm_mode(path.device_uid, path.channel_name, pwm_mode_json.pwm_mode)
        .await
        .map_err(handle_error)
}

pub async fn device_setting_reset(
    Path(path): Path<DeviceChannelPath>,
    State(AppState { device_handle, .. }): State<AppState>,
) -> Result<(), CCError> {
    device_handle
        .device_setting_reset(path.device_uid, path.channel_name)
        .await
        .map_err(handle_error)
}

/// Set `AseTek` Cooler driver type
/// This is needed to set `Legacy690Lc` or `Modern690Lc` device driver type
pub async fn asetek_type_update(
    Path(path): Path<DevicePath>,
    State(AppState { device_handle, .. }): State<AppState>,
    Json(asetek690_request): Json<AseTek690Request>,
) -> Result<(), CCError> {
    device_handle
        .device_asetek_type(path.device_uid, asetek690_request.is_legacy690)
        .await
        .map_err(handle_error)
}

pub async fn thinkpad_fan_control_modify(
    State(AppState { device_handle, .. }): State<AppState>,
    Json(fan_control_request): Json<ThinkPadFanControlRequest>,
) -> Result<(), CCError> {
    device_handle
        .thinkpad_fan_control(fan_control_request.enable)
        .await
        .map_err(handle_error)
}

pub async fn amd_gpu_overdrive_enable(
    State(AppState {
        device_handle,
        notification_handle,
        ..
    }): State<AppState>,
) -> Result<Json<String>, CCError> {
    device_handle
        .amd_gpu_overdrive_enable(notification_handle)
        .await
        .map(Json)
        .map_err(handle_error)
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DeviceDto {
    pub name: String,
    #[serde(rename(serialize = "type"))]
    pub d_type: DeviceType,
    pub type_index: u8,
    pub uid: UID,
    pub lc_info: Option<LcInfo>,
    pub info: DeviceInfo,
}

impl From<&Device> for DeviceDto {
    fn from(device: &Device) -> Self {
        Self {
            name: device.name.clone(),
            d_type: device.d_type,
            type_index: device.type_index,
            uid: device.uid.clone(),
            lc_info: device.lc_info.clone(),
            info: device.info.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DevicesResponse {
    devices: Vec<DeviceDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SettingsResponse {
    settings: Vec<Setting>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AseTek690Request {
    is_legacy690: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SettingManualRequest {
    speed_fixed: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SettingProfileUID {
    profile_uid: UID,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SettingPWMMode {
    pwm_mode: u8,
}

/// The form field carrying the uploaded files. The trailing brackets are what
/// the UI's form serializer emits for a repeated field.
const LCD_IMAGE_FIELD: &str = "images[]";

/// Only a single image is ever applied to a channel, so a second upload is
/// rejected before its body is buffered.
const LCD_IMAGE_COUNT_MAX: usize = 1;

/// Which part of `LcdImageForm` a multipart field feeds. Resolved before the
/// field is consumed, because reading a body invalidates the borrowed name.
enum LcdFormField {
    Mode,
    Brightness,
    Orientation,
    Image,
    Unknown,
}

/// The multipart form shared by every LCD image endpoint. Fields are read as
/// they stream in, so an unsupported upload is rejected before it is buffered.
#[derive(Debug)]
struct LcdImageForm {
    mode: String,
    brightness: Option<u8>,
    orientation: Option<u16>,
    images: Vec<(Mime, Vec<u8>)>,
}

impl LcdImageForm {
    async fn parse(mut multipart: Multipart) -> Result<Self, CCError> {
        let mut mode: Option<String> = None;
        let mut brightness = None;
        let mut orientation = None;
        let mut images = Vec::with_capacity(LCD_IMAGE_COUNT_MAX);
        while let Some(field) = multipart.next_field().await? {
            match classify_field(field.name()) {
                LcdFormField::Mode => mode = Some(field.text().await?),
                LcdFormField::Brightness => brightness = parse_number(field, "brightness").await?,
                LcdFormField::Orientation => {
                    orientation = parse_number(field, "orientation").await?;
                }
                LcdFormField::Image => {
                    if images.len() >= LCD_IMAGE_COUNT_MAX {
                        return Err(CCError::UserError {
                            msg: "Only one image is supported at this time".to_string(),
                        });
                    }
                    images.push(read_image(field).await?);
                }
                // Unknown fields are ignored, matching the previous parser.
                LcdFormField::Unknown => {}
            }
        }
        if images.is_empty() {
            return Err(CCError::UserError {
                msg: "At least one image is required".to_string(),
            });
        }
        assert!(images.len() <= LCD_IMAGE_COUNT_MAX);
        let mode = mode.ok_or_else(|| CCError::UserError {
            msg: "A mode is required".to_string(),
        })?;
        Ok(Self {
            mode,
            brightness,
            orientation,
            images,
        })
    }

    fn lcd_mode(&self) -> Result<LcdModeName, CCError> {
        self.mode.parse().map_err(|_| CCError::UserError {
            msg: format!("Invalid LCD mode name: {}", self.mode),
        })
    }
}

fn classify_field(name: Option<&str>) -> LcdFormField {
    match name {
        Some("mode") => LcdFormField::Mode,
        Some("brightness") => LcdFormField::Brightness,
        Some("orientation") => LcdFormField::Orientation,
        Some(LCD_IMAGE_FIELD) => LcdFormField::Image,
        _ => LcdFormField::Unknown,
    }
}

/// An absent field and a field submitted empty both mean "unset", as the UI
/// omits these entirely when they do not apply.
async fn parse_number<T: FromStr>(field: Field<'_>, name: &str) -> Result<Option<T>, CCError> {
    let text = field.text().await?;
    if text.is_empty() {
        return Ok(None);
    }
    text.parse().map(Some).map_err(|_| CCError::UserError {
        msg: format!("Invalid {name} value: {text}"),
    })
}

/// The content type comes from the part headers and is checked before the body
/// is buffered, so an unsupported upload costs nothing to reject.
async fn read_image(field: Field<'_>) -> Result<(Mime, Vec<u8>), CCError> {
    let content_type = field
        .content_type()
        .and_then(|ct| Mime::from_str(ct).ok())
        .unwrap_or(mime::IMAGE_PNG);
    if image::supported_image_types().contains(&content_type).not() {
        return Err(CCError::UserError {
            msg: format!(
                "Only image types {:?} are supported. Found:{content_type}",
                image::supported_image_types()
            ),
        });
    }
    let bytes = field.bytes().await?;
    Ok((content_type, bytes.to_vec()))
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ThinkPadFanControlRequest {
    enable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DevicePath {
    pub device_uid: DeviceUID,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DeviceChannelPath {
    pub device_uid: DeviceUID,
    pub channel_name: ChannelName,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct LcdUpdateQuery {
    /// Set to false for frequent updates from an external program. The setting is applied
    /// to the device only: it is not saved and does not deactivate the active Mode. Success
    /// is logged at debug level.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    log: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct LcdImageUpdateQuery {
    /// Set to false to log a successful apply at debug level.
    #[serde(default, deserialize_with = "empty_string_as_none")]
    log: Option<bool>,
}

/// Serde deserialization decorator to map empty Strings to None,
fn empty_string_as_none<'de, D, T>(de: D) -> anyhow::Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: FromStr,
    T::Err: fmt::Display,
{
    let opt = Option::<String>::deserialize(de)?;
    match opt.as_deref() {
        None | Some("") => Ok(None),
        Some(s) => FromStr::from_str(s).map_err(de::Error::custom).map(Some),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::extract::FromRequest;
    use axum::http::Request;

    const BOUNDARY: &str = "cc-test-boundary";
    const PNG_BYTES: &str = "fake-png-bytes";

    fn text_part(name: &str, value: &str) -> String {
        format!(
            "--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"{name}\"\r\n\r\n{value}\r\n"
        )
    }

    fn image_part(content_type: Option<&str>, bytes: &str) -> String {
        let content_type =
            content_type.map_or_else(String::new, |ct| format!("Content-Type: {ct}\r\n"));
        format!(
            "--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"{LCD_IMAGE_FIELD}\"; \
             filename=\"lcd.png\"\r\n{content_type}\r\n{bytes}\r\n"
        )
    }

    fn png_part() -> String {
        image_part(Some("image/png"), PNG_BYTES)
    }

    /// Feeds the parts through the real axum extractor, so a test covers the
    /// same path a request takes rather than a hand-built stand-in.
    async fn parse_parts(parts: &[String]) -> Result<LcdImageForm, CCError> {
        let body = format!("{}--{BOUNDARY}--\r\n", parts.concat());
        let request = Request::builder()
            .method("POST")
            .header(
                header::CONTENT_TYPE,
                format!("multipart/form-data; boundary={BOUNDARY}"),
            )
            .body(Body::from(body))
            .unwrap();
        let multipart = Multipart::from_request(request, &()).await.unwrap();
        LcdImageForm::parse(multipart).await
    }

    /// Every field the UI sends on the settings-apply path round-trips.
    #[tokio::test]
    async fn parses_a_complete_form() {
        let form = parse_parts(&[
            text_part("mode", "image"),
            text_part("brightness", "50"),
            text_part("orientation", "90"),
            png_part(),
        ])
        .await
        .unwrap();

        assert_eq!(form.mode, "image");
        assert_eq!(form.brightness, Some(50));
        assert_eq!(form.orientation, Some(90));
        assert_eq!(form.images.len(), 1);
        assert_eq!(form.images[0].0, mime::IMAGE_PNG);
        assert_eq!(form.images[0].1, PNG_BYTES.as_bytes());
    }

    /// The preview path sends only a mode and the file, so the optional
    /// settings must stay unset rather than reject the request.
    #[tokio::test]
    async fn omitted_optionals_stay_unset() {
        let form = parse_parts(&[text_part("mode", "image"), png_part()])
            .await
            .unwrap();

        assert_eq!(form.brightness, None);
        assert_eq!(form.orientation, None);
    }

    /// A browser submits an untouched control as an empty string, which means
    /// unset and not a parse failure.
    #[tokio::test]
    async fn empty_optionals_are_unset() {
        let form = parse_parts(&[
            text_part("mode", "image"),
            text_part("brightness", ""),
            text_part("orientation", ""),
            png_part(),
        ])
        .await
        .unwrap();

        assert_eq!(form.brightness, None);
        assert_eq!(form.orientation, None);
    }

    #[tokio::test]
    async fn rejects_an_unparsable_number() {
        let error = parse_parts(&[
            text_part("mode", "image"),
            text_part("brightness", "bright"),
            png_part(),
        ])
        .await
        .unwrap_err();

        assert!(error.to_string().contains("Invalid brightness value"));
    }

    /// A value past the field's range is a client error, not a silent clamp.
    #[tokio::test]
    async fn rejects_an_out_of_range_number() {
        let error = parse_parts(&[
            text_part("mode", "image"),
            text_part("brightness", "300"),
            png_part(),
        ])
        .await
        .unwrap_err();

        assert!(error.to_string().contains("Invalid brightness value"));
    }

    #[tokio::test]
    async fn requires_an_image() {
        let error = parse_parts(&[text_part("mode", "image")])
            .await
            .unwrap_err();

        assert!(error.to_string().contains("At least one image is required"));
    }

    #[tokio::test]
    async fn requires_a_mode() {
        let error = parse_parts(&[png_part()]).await.unwrap_err();

        assert!(error.to_string().contains("A mode is required"));
    }

    #[tokio::test]
    async fn rejects_a_second_image() {
        let error = parse_parts(&[text_part("mode", "image"), png_part(), png_part()])
            .await
            .unwrap_err();

        assert!(error
            .to_string()
            .contains("Only one image is supported at this time"));
    }

    #[tokio::test]
    async fn rejects_an_unsupported_image_type() {
        let error = parse_parts(&[
            text_part("mode", "image"),
            image_part(Some("text/plain"), "not-an-image"),
        ])
        .await
        .unwrap_err();

        assert!(error.to_string().contains("Only image types"));
    }

    /// A part without its own content type is assumed to be a PNG, which is
    /// what the UI produces for a processed image.
    #[tokio::test]
    async fn defaults_a_typeless_part_to_png() {
        let form = parse_parts(&[text_part("mode", "image"), image_part(None, PNG_BYTES)])
            .await
            .unwrap();

        assert_eq!(form.images[0].0, mime::IMAGE_PNG);
    }

    /// Unknown fields are skipped so an older or newer client can send extras
    /// without breaking the request.
    #[tokio::test]
    async fn ignores_unknown_fields() {
        let form = parse_parts(&[
            text_part("mode", "image"),
            text_part("something_else", "ignored"),
            png_part(),
        ])
        .await
        .unwrap();

        assert_eq!(form.mode, "image");
        assert_eq!(form.images.len(), 1);
    }

    #[tokio::test]
    async fn resolves_the_lcd_mode() {
        let form = parse_parts(&[text_part("mode", "carousel"), png_part()])
            .await
            .unwrap();

        assert_eq!(form.lcd_mode().unwrap(), LcdModeName::Carousel);
    }

    #[tokio::test]
    async fn rejects_an_unknown_lcd_mode() {
        let form = parse_parts(&[text_part("mode", "hologram"), png_part()])
            .await
            .unwrap();

        assert!(form
            .lcd_mode()
            .unwrap_err()
            .to_string()
            .contains("Invalid LCD mode name: hologram"));
    }
}
