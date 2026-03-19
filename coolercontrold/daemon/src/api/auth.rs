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

use crate::admin;
use crate::api::actor::{TokenHandle, TokenValidation};
use crate::api::{AppState, CCError};
use aide::axum::IntoApiResponse;
use aide::NoApi;
use anyhow::Result;
use axum::extract::{FromRequestParts, Request, State};
use axum::http::header;
use axum::http::request::Parts;
use axum::middleware::Next;
use axum::{Extension, Json};
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;
use derive_more::Display;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use strum::EnumString;
use tower_sessions::Session;

const SESSION_USER_ID: &str = "CCAdmin";
const SESSION_PERMISSIONS: &str = "permissions";
const INVALID_MESSAGE: &str = "Invalid username or password.";

/// Maximum length of a decoded `Authorization: Basic` value.
/// Base64-decoded "user:password" should never exceed this.
const MAX_BASIC_AUTH_DECODED_BYTES: usize = 1024;

/// Credentials extracted from an `Authorization: Basic` header.
///
/// Replaces the `headers` crate's `Authorization<Basic>` extractor.
/// Format: `Authorization: Basic base64(username:password)`
#[derive(Debug, Clone)]
pub struct BasicAuth {
    username: String,
    password: String,
}

impl BasicAuth {
    pub fn username(&self) -> &str {
        &self.username
    }

    pub fn password(&self) -> &str {
        &self.password
    }

    /// Parse a raw header value into `BasicAuth` credentials.
    fn parse_header_value(value: &str) -> Result<Self, CCError> {
        let encoded = value
            .strip_prefix("Basic ")
            .ok_or_else(|| CCError::InvalidCredentials {
                msg: "Authorization header must use Basic scheme.".to_string(),
            })?;
        assert!(!encoded.is_empty(), "Base64 payload must not be empty.");
        let decoded_bytes = BASE64
            .decode(encoded)
            .map_err(|_| CCError::InvalidCredentials {
                msg: "Invalid base64 in Authorization header.".to_string(),
            })?;
        assert!(
            decoded_bytes.len() <= MAX_BASIC_AUTH_DECODED_BYTES,
            "Decoded Basic auth exceeds maximum length."
        );
        let decoded =
            String::from_utf8(decoded_bytes).map_err(|_| CCError::InvalidCredentials {
                msg: "Authorization header contains invalid UTF-8.".to_string(),
            })?;
        let (username, password) =
            decoded
                .split_once(':')
                .ok_or_else(|| CCError::InvalidCredentials {
                    msg: "Authorization header missing ':' separator.".to_string(),
                })?;
        assert!(
            !username.is_empty(),
            "Username in Basic auth must not be empty."
        );
        Ok(Self {
            username: username.to_string(),
            password: password.to_string(),
        })
    }
}

impl<S: Send + Sync> FromRequestParts<S> for BasicAuth {
    type Rejection = CCError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let auth_value = parts.headers.get(header::AUTHORIZATION).ok_or_else(|| {
            CCError::InvalidCredentials {
                msg: "Missing Authorization header.".to_string(),
            }
        })?;
        let value = auth_value
            .to_str()
            .map_err(|_| CCError::InvalidCredentials {
                msg: "Authorization header contains invalid characters.".to_string(),
            })?;
        Self::parse_header_value(value)
    }
}

/// Read-access middleware. Validates Bearer tokens (any valid token) or
/// session cookies. Used for read-only routes.
pub async fn auth_middleware(
    Extension(token_handle): Extension<TokenHandle>,
    session: Session,
    request: Request,
    next: Next,
) -> impl IntoApiResponse {
    if let Some(auth_value) = request.headers().get(header::AUTHORIZATION) {
        if let Ok(value) = auth_value.to_str() {
            if let Some(raw_token) = value.strip_prefix("Bearer ") {
                return match token_handle.validate(raw_token.to_string()).await {
                    Ok(TokenValidation::ValidReadWrite | TokenValidation::ValidReadOnly) => {
                        Ok(next.run(request).await)
                    }
                    Ok(TokenValidation::Invalid) => Err(CCError::InvalidCredentials {
                        msg: "Invalid or expired access token.".to_string(),
                    }),
                    Err(_) => Err(CCError::InternalError {
                        msg: "Token validation error.".to_string(),
                    }),
                };
            }
        }
    }
    check_session_permission(session, request, next).await
}

/// Write-access middleware. Validates Bearer tokens (requires write access)
/// or session cookies. Used for write/mutating routes.
pub async fn auth_write_middleware(
    Extension(token_handle): Extension<TokenHandle>,
    session: Session,
    request: Request,
    next: Next,
) -> impl IntoApiResponse {
    if let Some(auth_value) = request.headers().get(header::AUTHORIZATION) {
        if let Ok(value) = auth_value.to_str() {
            if let Some(raw_token) = value.strip_prefix("Bearer ") {
                return match token_handle.validate(raw_token.to_string()).await {
                    Ok(TokenValidation::ValidReadWrite) => Ok(next.run(request).await),
                    Ok(TokenValidation::ValidReadOnly) => Err(CCError::InsufficientScope {
                        msg: "This token does not have write access.".to_string(),
                    }),
                    Ok(TokenValidation::Invalid) => Err(CCError::InvalidCredentials {
                        msg: "Invalid or expired access token.".to_string(),
                    }),
                    Err(_) => Err(CCError::InternalError {
                        msg: "Token validation error.".to_string(),
                    }),
                };
            }
        }
    }
    check_session_permission(session, request, next).await
}

/// Session-only authentication middleware. Used for session-sensitive routes
/// like token management, password changes, and logout.
/// Bearer tokens are not accepted on these routes.
pub async fn session_auth_middleware(
    session: Session,
    request: Request,
    next: Next,
) -> impl IntoApiResponse {
    check_session_permission(session, request, next).await
}

async fn check_session_permission(
    session: Session,
    request: Request,
    next: Next,
) -> Result<axum::response::Response, CCError> {
    let permission = session
        .get::<Permission>(SESSION_PERMISSIONS)
        .await
        .unwrap_or(Some(Permission::Guest))
        .unwrap_or(Permission::Guest);
    match permission {
        Permission::Admin => Ok(next.run(request).await),
        Permission::Guest => Err(CCError::InvalidCredentials {
            msg: "Invalid Credentials".to_string(),
        }),
    }
}

pub async fn login(
    NoApi(auth_header): NoApi<BasicAuth>,
    NoApi(session): NoApi<Session>,
    State(AppState { auth_handle, .. }): State<AppState>,
) -> Result<(), CCError> {
    if auth_header.username() == SESSION_USER_ID
        && auth_handle
            .match_passwd(auth_header.password().to_string())
            .await?
    {
        session
            .insert(SESSION_PERMISSIONS, Permission::Admin)
            .await
            .map_err(|err| CCError::InternalError {
                msg: err.to_string(),
            })?;
        Ok(())
    } else {
        Err(CCError::InvalidCredentials {
            msg: INVALID_MESSAGE.to_string(),
        })
    }
}

/// This endpoint is used to verify if the login session is still valid
pub async fn verify_session() -> Result<(), CCError> {
    if admin::match_passwd(admin::DEFAULT_PASS).await {
        return Err(CCError::InvalidCredentials {
            msg: "The Default password or a reset has invalidated the session.".to_string(),
        });
    }
    Ok(())
}

#[derive(Deserialize, JsonSchema)]
pub struct SetPasswdRequest {
    current_password: String,
}

pub async fn set_passwd(
    NoApi(auth_header): NoApi<BasicAuth>,
    NoApi(session): NoApi<Session>,
    State(AppState { auth_handle, .. }): State<AppState>,
    Json(body): Json<SetPasswdRequest>,
) -> Result<(), CCError> {
    if auth_header.username() != SESSION_USER_ID || auth_header.password().is_empty() {
        return Err(CCError::InvalidCredentials {
            msg: INVALID_MESSAGE.to_string(),
        });
    }
    if !auth_handle.match_passwd(body.current_password).await? {
        return Err(CCError::InvalidCredentials {
            msg: "Current password is incorrect.".to_string(),
        });
    }
    if auth_header.password() == admin::DEFAULT_PASS {
        return Err(CCError::UserError {
            msg: "The default password cannot be used as a new password.".to_string(),
        });
    }
    auth_handle
        .save_passwd(auth_header.password().to_string())
        .await?;

    // Delete current session — flows through CachingSessionStore::delete() which
    // calls both MemorySessionStore::delete() (clear all) and
    // FileSessionStore::delete() (delete all files).
    let _ = session.delete().await;
    Ok(())
}

pub async fn logout(NoApi(session): NoApi<Session>) -> impl IntoApiResponse {
    session.clear().await;
}

#[derive(Debug, Clone, Display, EnumString, Serialize, Deserialize)]
pub enum Permission {
    Admin,
    Guest,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encode_basic(username: &str, password: &str) -> String {
        format!("Basic {}", BASE64.encode(format!("{username}:{password}")))
    }

    #[test]
    fn parse_valid_basic_auth() {
        // Goal: verify standard Basic auth header is parsed correctly.
        let header = encode_basic("CCAdmin", "mypassword");
        let auth = BasicAuth::parse_header_value(&header).unwrap();
        assert_eq!(auth.username(), "CCAdmin");
        assert_eq!(auth.password(), "mypassword");
    }

    #[test]
    fn parse_empty_password() {
        // Goal: verify that an empty password is accepted (valid per RFC 7617).
        let header = encode_basic("CCAdmin", "");
        let auth = BasicAuth::parse_header_value(&header).unwrap();
        assert_eq!(auth.username(), "CCAdmin");
        assert_eq!(auth.password(), "");
    }

    #[test]
    fn parse_password_with_colon() {
        // Goal: verify passwords containing ':' are not split incorrectly.
        // Only the first ':' separates username from password.
        let header = encode_basic("CCAdmin", "pass:with:colons");
        let auth = BasicAuth::parse_header_value(&header).unwrap();
        assert_eq!(auth.username(), "CCAdmin");
        assert_eq!(auth.password(), "pass:with:colons");
    }

    #[test]
    fn parse_unicode_password() {
        // Goal: verify UTF-8 passwords are handled correctly.
        let header = encode_basic("CCAdmin", "pässwörd🔒");
        let auth = BasicAuth::parse_header_value(&header).unwrap();
        assert_eq!(auth.username(), "CCAdmin");
        assert_eq!(auth.password(), "pässwörd🔒");
    }

    #[test]
    fn reject_non_basic_scheme() {
        // Goal: verify non-Basic schemes are rejected.
        let header = "Bearer some_token";
        let err = BasicAuth::parse_header_value(header).unwrap_err();
        assert!(
            matches!(err, CCError::InvalidCredentials { .. }),
            "Expected InvalidCredentials, got: {err:?}"
        );
    }

    #[test]
    fn reject_invalid_base64() {
        // Goal: verify corrupted base64 is rejected.
        let header = "Basic !!!not-valid-base64!!!";
        let err = BasicAuth::parse_header_value(header).unwrap_err();
        assert!(matches!(err, CCError::InvalidCredentials { .. }));
    }

    #[test]
    fn reject_missing_colon_separator() {
        // Goal: verify that a decoded value without ':' is rejected.
        let header = format!("Basic {}", BASE64.encode("nocolon"));
        let err = BasicAuth::parse_header_value(&header).unwrap_err();
        assert!(matches!(err, CCError::InvalidCredentials { .. }));
    }

    #[test]
    fn reject_empty_username() {
        // Goal: verify that an empty username triggers an assertion panic.
        let header = encode_basic("", "password");
        let result = std::panic::catch_unwind(|| BasicAuth::parse_header_value(&header));
        assert!(result.is_err(), "Expected panic for empty username.");
    }

    #[test]
    fn reject_bearer_scheme() {
        // Goal: verify "Bearer" prefix is not confused with "Basic".
        let header = "Bearer eyJhbGciOiJIUzI1NiJ9";
        let err = BasicAuth::parse_header_value(header).unwrap_err();
        assert!(matches!(err, CCError::InvalidCredentials { .. }));
    }

    #[test]
    fn reject_no_scheme() {
        // Goal: verify a raw value without any scheme prefix is rejected.
        let header = "dXNlcjpwYXNz"; // base64("user:pass") without "Basic " prefix
        let err = BasicAuth::parse_header_value(header).unwrap_err();
        assert!(matches!(err, CCError::InvalidCredentials { .. }));
    }

    #[test]
    fn accessors_return_correct_values() {
        // Goal: verify username() and password() accessors match parsed values.
        let header = encode_basic("admin", "secret");
        let auth = BasicAuth::parse_header_value(&header).unwrap();
        assert_eq!(auth.username(), "admin");
        assert_eq!(auth.password(), "secret");
    }
}
