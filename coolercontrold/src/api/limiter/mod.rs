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

pub mod throttler;

use crate::api::limiter::throttler::{Limiter, LimiterConfig};
use ::governor::clock::{Clock, DefaultClock};
use axum::body::Body;

use axum::http::{HeaderMap, Request, Response, StatusCode};
use derive_more::{Display, Error};
use pin_project::pin_project;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::{future::Future, mem, pin::Pin};
use tower::{Layer, Service};

#[derive(Clone)]
pub struct LimiterLayer {
    pub config: Arc<LimiterConfig>,
}

impl<S> Layer<S> for LimiterLayer {
    type Service = Limiter<S>;

    fn layer(&self, inner: S) -> Self::Service {
        Limiter::new(inner, &self.config)
    }
}

impl<S, ReqBody> Service<Request<ReqBody>> for Limiter<S>
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

#[derive(Debug, Error, Display, Clone)]
pub enum ThrottlingError {
    #[display("Rate limit exceeded.")]
    TooManyRequests {
        wait_time: u64,
        headers: Option<HeaderMap>,
    },

    #[display("Unknown Limiter Error")]
    Unknown,
}

impl ThrottlingError {
    pub fn as_response<ResB>(&mut self) -> Response<ResB>
    where
        ResB: From<String>,
    {
        match mem::replace(self, Self::Unknown) {
            ThrottlingError::TooManyRequests { wait_time, headers } => {
                let response = Response::new(format!(
                    "Rate limit exceeded, please wait at least {wait_time}s"
                ));
                let (mut parts, body) = response.into_parts();
                parts.status = StatusCode::TOO_MANY_REQUESTS;
                if let Some(headers) = headers {
                    parts.headers = headers;
                }
                Response::from_parts(parts, ResB::from(body))
            }
            ThrottlingError::Unknown => {
                let response = Response::new("Rate limit returned unknown error".to_string());
                let (mut parts, body) = response.into_parts();
                parts.status = StatusCode::INTERNAL_SERVER_ERROR;
                Response::from_parts(parts, ResB::from(body))
            }
        }
    }
}
