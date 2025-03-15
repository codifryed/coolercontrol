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
use crate::api::{AppState, CCError};
use aide::axum::IntoApiResponse;
use aide::NoApi;
use anyhow::Result;
use axum::extract::State;
use axum_extra::TypedHeader;
use derive_more::Display;
use headers::authorization::Basic;
use headers::Authorization;
use serde::{Deserialize, Serialize};
use std::ops::Not;
use strum::EnumString;
use tower_sessions::Session;

const SESSION_USER_ID: &str = "CCAdmin";
const SESSION_PERMISSIONS: &str = "permissions";
const INVALID_MESSAGE: &str = "Invalid username or password.";

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
pub async fn verify_session(NoApi(session): NoApi<Session>) -> Result<(), CCError> {
    verify_admin_permissions(&session).await?;
    Ok(())
}

pub async fn set_passwd(
    NoApi(TypedHeader(auth_header)): NoApi<TypedHeader<Authorization<Basic>>>,
    NoApi(session): NoApi<Session>,
    State(AppState { auth_handle, .. }): State<AppState>,
) -> Result<(), CCError> {
    verify_admin_permissions(&session).await?;
    if auth_header.username() == SESSION_USER_ID && auth_header.password().is_empty().not() {
        auth_handle
            .save_passwd(auth_header.password().to_string())
            .await?;
        Ok(())
    } else {
        Err(CCError::InvalidCredentials {
            msg: INVALID_MESSAGE.to_string(),
        })
    }
}

pub async fn logout(NoApi(session): NoApi<Session>) -> impl IntoApiResponse {
    session.clear().await;
}

pub async fn verify_admin_permissions(session: &Session) -> Result<(), CCError> {
    let permission = session
        .get::<Permission>(SESSION_PERMISSIONS)
        .await
        .unwrap_or(Some(Permission::Guest))
        .unwrap_or(Permission::Guest);
    match permission {
        Permission::Admin => Ok(()),
        Permission::Guest => Err(CCError::InvalidCredentials {
            msg: "Invalid Credentials".to_string(),
        }),
    }
}

#[derive(Debug, Clone, Display, EnumString, Serialize, Deserialize)]
pub enum Permission {
    Admin,
    Guest,
}
