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

use ::governor::clock::{Clock, DefaultClock};
use tonic::body::Body;

use crate::api::limiter::ThrottlingError;
use axum::http::{HeaderMap, Request, Response};
use governor::{DefaultDirectRateLimiter, Quota, RateLimiter};
use pin_project::pin_project;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::{fmt, num::NonZeroU32, time::Duration};
use std::{future::Future, pin::Pin};
use tower::{Layer, Service};

const DEFAULT_SINGLE_TOKEN_CLEAR_PERIOD: Duration = Duration::from_millis(500);
const DEFAULT_BURST_SIZE: u32 = 8;

/// This is specifically for `tonic` and the grpc service.
#[derive(Clone)]
pub struct GRPCLimiterLayer {
    pub config: Arc<GRPCLimiterConfig>,
}

impl<S> Layer<S> for GRPCLimiterLayer {
    type Service = GRPCLimiter<S>;

    fn layer(&self, inner: S) -> Self::Service {
        GRPCLimiter::new(inner, &self.config)
    }
}

impl<S, ReqBody> Service<Request<ReqBody>> for GRPCLimiter<S>
where
    S: Service<Request<ReqBody>, Response = Response<Body>>,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = ResponseFuture<S::Future>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request<ReqBody>) -> Self::Future {
        match self.limiter.check() {
            Ok(()) => {
                let future = self.inner.call(req);
                ResponseFuture {
                    inner: Kind::Passthrough { future },
                }
            }
            Err(negative) => {
                let wait_time = negative
                    .wait_time_from(DefaultClock::default().now())
                    .as_secs();
                let mut headers = HeaderMap::new();
                headers.insert("retry-after", wait_time.into());
                let error_response = self.error_handler()(ThrottlingError::TooManyRequests {
                    wait_time,
                    headers: Some(headers),
                });
                ResponseFuture {
                    inner: Kind::Error {
                        error_response: Some(error_response),
                    },
                }
            }
        }
    }
}

#[derive(Debug)]
#[pin_project]
pub struct ResponseFuture<F> {
    #[pin]
    inner: Kind<F>,
}

#[derive(Debug)]
#[pin_project(project = KindProj)]
enum Kind<F> {
    Passthrough {
        #[pin]
        future: F,
    },
    Error {
        error_response: Option<Response<Body>>,
    },
}

impl<F, E> Future for ResponseFuture<F>
where
    F: Future<Output = Result<Response<Body>, E>>,
{
    type Output = Result<Response<Body>, E>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.project().inner.project() {
            KindProj::Passthrough { future } => future.poll(cx),
            KindProj::Error { error_response } => Poll::Ready(Ok(error_response.take().expect("
                <Limiter as Service<Request<_>>>::call must produce Response<String> when LimiterError occurs.
            "))),
        }
    }
}

#[derive(Debug, Clone)]
pub struct GRPCLimiterConfig {
    limiter: Arc<DefaultDirectRateLimiter>,
    error_handler: GRPCErrorHandler,
}

impl GRPCLimiterConfig {
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

impl Default for GRPCLimiterConfig {
    fn default() -> Self {
        Self {
            limiter: Arc::new(RateLimiter::direct(
                Quota::with_period(DEFAULT_SINGLE_TOKEN_CLEAR_PERIOD)
                    .unwrap()
                    .allow_burst(NonZeroU32::new(DEFAULT_BURST_SIZE).unwrap()),
            )),
            error_handler: GRPCErrorHandler::default(),
        }
    }
}

#[derive(Debug, Clone)]
#[allow(clippy::struct_field_names)]
pub struct GRPCLimiter<S> {
    pub limiter: Arc<DefaultDirectRateLimiter>,
    pub inner: S,
    error_handler: GRPCErrorHandler,
}

impl<S> GRPCLimiter<S> {
    pub fn new(inner: S, config: &GRPCLimiterConfig) -> Self {
        GRPCLimiter {
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
struct GRPCErrorHandler(Arc<dyn Fn(ThrottlingError) -> Response<Body> + Send + Sync>);

impl Default for GRPCErrorHandler {
    fn default() -> Self {
        Self(Arc::new(|mut e| e.as_response_body()))
    }
}

impl fmt::Debug for GRPCErrorHandler {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ErrorHandler").finish()
    }
}
