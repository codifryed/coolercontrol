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

use crate::api::limiter::ThrottlingError;
use axum::body::Body;
use axum::http::Response;
use governor::{DefaultDirectRateLimiter, Quota, RateLimiter};
use std::{fmt, num::NonZeroU32, sync::Arc, time::Duration};

const DEFAULT_SINGLE_TOKEN_CLEAR_PERIOD: Duration = Duration::from_millis(500);
const DEFAULT_BURST_SIZE: u32 = 8;

#[derive(Debug, Clone)]
pub struct LimiterConfig {
    limiter: Arc<DefaultDirectRateLimiter>,
    error_handler: ErrorHandler,
}

impl LimiterConfig {
    pub fn new(single_token_clear_duration: Duration, burst: u32) -> Self {
        Self {
            limiter: Arc::new(RateLimiter::direct(
                Quota::with_period(single_token_clear_duration)
                    .unwrap()
                    .allow_burst(NonZeroU32::new(burst).unwrap()),
            )),
            ..Self::default()
        }
    }
}

impl Default for LimiterConfig {
    fn default() -> Self {
        Self {
            limiter: Arc::new(RateLimiter::direct(
                Quota::with_period(DEFAULT_SINGLE_TOKEN_CLEAR_PERIOD)
                    .unwrap()
                    .allow_burst(NonZeroU32::new(DEFAULT_BURST_SIZE).unwrap()),
            )),
            error_handler: ErrorHandler::default(),
        }
    }
}

#[derive(Debug, Clone)]
#[allow(clippy::struct_field_names)]
pub struct Limiter<S> {
    pub limiter: Arc<DefaultDirectRateLimiter>,
    pub inner: S,
    error_handler: ErrorHandler,
}

impl<S> Limiter<S> {
    pub fn new(inner: S, config: &LimiterConfig) -> Self {
        Limiter {
            limiter: config.limiter.clone(),
            inner,
            error_handler: config.error_handler.clone(),
        }
    }

    pub(crate) fn error_handler(
        &self,
    ) -> &(dyn Fn(ThrottlingError) -> Response<Body> + Send + Sync) {
        &*self.error_handler.0
    }
}

#[derive(Clone)]
struct ErrorHandler(Arc<dyn Fn(ThrottlingError) -> Response<Body> + Send + Sync>);

impl Default for ErrorHandler {
    fn default() -> Self {
        Self(Arc::new(|mut e| e.as_response()))
    }
}

impl fmt::Debug for ErrorHandler {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ErrorHandler").finish()
    }
}
