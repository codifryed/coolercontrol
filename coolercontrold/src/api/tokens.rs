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

use crate::api::{handle_error, AppState, CCError};
use axum::extract::{Path, State};
use axum::Json;
use chrono::{DateTime, Local};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CreateTokenRequest {
    pub label: String,
    pub expires_at: Option<DateTime<Local>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CreateTokenResponse {
    pub id: String,
    pub label: String,
    pub token: String,
    pub created_at: DateTime<Local>,
    pub expires_at: Option<DateTime<Local>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TokenInfo {
    pub id: String,
    pub label: String,
    pub created_at: DateTime<Local>,
    pub expires_at: Option<DateTime<Local>>,
    pub last_used: Option<DateTime<Local>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TokenListResponse {
    pub tokens: Vec<TokenInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TokenPath {
    pub token_id: String,
}

pub async fn create(
    State(AppState { token_handle, .. }): State<AppState>,
    Json(body): Json<CreateTokenRequest>,
) -> Result<Json<CreateTokenResponse>, CCError> {
    let label = body.label.trim().to_string();
    if label.is_empty() {
        return Err(CCError::UserError {
            msg: "Token label cannot be empty.".to_string(),
        });
    }
    if label.len() > 50 {
        return Err(CCError::UserError {
            msg: "Token label cannot be longer than 50 characters.".to_string(),
        });
    }
    if let Some(expires_at) = body.expires_at {
        if expires_at <= Local::now() {
            return Err(CCError::UserError {
                msg: "Expiry date must be in the future.".to_string(),
            });
        }
    }
    let (stored, raw_token) = token_handle
        .create(label, body.expires_at)
        .await
        .map_err(handle_error)?;
    Ok(Json(CreateTokenResponse {
        id: stored.id,
        label: stored.label,
        token: raw_token,
        created_at: stored.created_at,
        expires_at: stored.expires_at,
    }))
}

pub async fn list(
    State(AppState { token_handle, .. }): State<AppState>,
) -> Result<Json<TokenListResponse>, CCError> {
    let tokens = token_handle.list().await.map_err(handle_error)?;
    Ok(Json(TokenListResponse {
        tokens: tokens
            .into_iter()
            .map(|t| TokenInfo {
                id: t.id,
                label: t.label,
                created_at: t.created_at,
                expires_at: t.expires_at,
                last_used: t.last_used,
            })
            .collect(),
    }))
}

pub async fn delete(
    Path(path): Path<TokenPath>,
    State(AppState { token_handle, .. }): State<AppState>,
) -> Result<(), CCError> {
    token_handle
        .delete(path.token_id)
        .await
        .map_err(handle_error)
}
