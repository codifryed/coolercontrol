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
use crate::api::{AppState, CCError};
use aide::axum::IntoApiResponse;
use aide::NoApi;
use anyhow::Result;
use axum::extract::{Request, State};
use axum::middleware::Next;
use axum::Json;
use axum_extra::TypedHeader;
use derive_more::Display;
use headers::authorization::Basic;
use headers::Authorization;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use strum::EnumString;
use tower_sessions::Session;

const SESSION_USER_ID: &str = "CCAdmin";
const SESSION_PERMISSIONS: &str = "permissions";
const INVALID_MESSAGE: &str = "Invalid username or password.";

/// This middleware function is used to verify if the user is logged in.
/// If the user is not logged in, then the request is rejected.
/// It should be used on all routes that require authentication.
pub async fn auth_middleware(
    session: Session,
    request: Request,
    next: Next,
) -> impl IntoApiResponse {
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
    NoApi(TypedHeader(auth_header)): NoApi<TypedHeader<Authorization<Basic>>>,
    NoApi(session): NoApi<Session>,
    State(AppState { auth_handle, .. }): State<AppState>,
) -> Result<(), CCError> {
    // if the headers aren't present, then `TypedHeaderRejection` is used. (Like JsonRejection)
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
    Ok(())
}

#[derive(Deserialize, JsonSchema)]
pub struct SetPasswdRequest {
    current_password: String,
}

pub async fn set_passwd(
    NoApi(TypedHeader(auth_header)): NoApi<TypedHeader<Authorization<Basic>>>,
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
    auth_handle
        .save_passwd(auth_header.password().to_string())
        .await?;
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
