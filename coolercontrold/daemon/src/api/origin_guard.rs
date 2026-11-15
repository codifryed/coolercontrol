// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Refuses state-changing requests that a browser sent from another origin.
//!
//! The session cookie is `SameSite=Strict`, which does not separate two origins on one site.
//! A page served from any other port of the daemon's host is same-site, so the browser
//! attaches the cookie to its requests. `Sec-Fetch-Site` tells the two apart, and page
//! script cannot set or change it.

use crate::api::CCError;
use axum::extract::Request;
use axum::http::{header, HeaderMap, Method};
use axum::response::{IntoResponse, Response};
use futures_util::future::Either;
use std::future::{ready, Ready};
use std::sync::Arc;
use std::task::{Context, Poll};
use tower::{Layer, Service};

const SEC_FETCH_SITE: &str = "sec-fetch-site";
const SAME_ORIGIN: &[u8] = b"same-origin";

/// The exact origins from the `origins` setting. A browser request from one of them may
/// write, as CORS already lets it read.
pub type ConfiguredOrigins = Arc<[String]>;

/// Keeps only the `origins` entries a browser could send as an `Origin` header.
pub fn configured_origins(origins: Vec<String>) -> ConfiguredOrigins {
    origins
        .into_iter()
        .filter(|o| o.starts_with("http://") || o.starts_with("https://"))
        .collect()
}

/// A plain service rather than `middleware::from_fn`, which boxes a future and the next
/// service on every request, and this runs on every request.
#[derive(Clone)]
pub struct OriginGuardLayer {
    configured: ConfiguredOrigins,
}

impl OriginGuardLayer {
    pub fn new(configured: ConfiguredOrigins) -> Self {
        Self { configured }
    }
}

impl<S> Layer<S> for OriginGuardLayer {
    type Service = OriginGuard<S>;

    fn layer(&self, inner: S) -> Self::Service {
        OriginGuard {
            inner,
            configured: Arc::clone(&self.configured),
        }
    }
}

#[derive(Clone)]
pub struct OriginGuard<S> {
    inner: S,
    configured: ConfiguredOrigins,
}

impl<S> Service<Request> for OriginGuard<S>
where
    S: Service<Request, Response = Response>,
{
    type Response = Response;
    type Error = S::Error;
    type Future = Either<S::Future, Ready<Result<Response, S::Error>>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: Request) -> Self::Future {
        if is_request_allowed(request.method(), request.headers(), &self.configured) {
            return Either::Left(self.inner.call(request));
        }
        Either::Right(ready(Ok(refusal())))
    }
}

fn refusal() -> Response {
    CCError::InsufficientScope {
        msg: "Refused a cross-origin request. To allow an origin, add it to the daemon's \
              origins setting."
            .to_string(),
    }
    .into_response()
}

/// Whether a request may reach the routes.
///
/// Reads pass: they change nothing, and CORS decides who may see the response. A request
/// without `Sec-Fetch-Site` passes too. Only browsers send it, so curl, scripts and the Qt
/// app's own requests are unaffected, while every browser the UI supports sends it on a
/// write. A bearer token earns no exemption: the session-only routes ignore
/// `Authorization` and read the cookie, so a junk token would carry the cookie past here.
fn is_request_allowed(method: &Method, headers: &HeaderMap, configured: &[String]) -> bool {
    if is_safe(method) {
        return true;
    }
    let Some(site) = headers.get(SEC_FETCH_SITE) else {
        return true;
    };
    // `same-site` and `none` are refused too: the gap this closes is a same-site page.
    if site.as_bytes() == SAME_ORIGIN {
        return true;
    }
    headers
        .get(header::ORIGIN)
        .and_then(|origin| origin.to_str().ok())
        .is_some_and(|origin| configured.iter().any(|c| c == origin))
}

fn is_safe(method: &Method) -> bool {
    method == Method::GET || method == Method::HEAD || method == Method::OPTIONS
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;
    use std::ops::Not;

    const CONFIGURED: &str = "https://coolercontrol.home.lan";

    fn headers(site: Option<&'static str>, origin: Option<&'static str>) -> HeaderMap {
        let mut headers = HeaderMap::new();
        if let Some(site) = site {
            headers.insert(SEC_FETCH_SITE, HeaderValue::from_static(site));
        }
        if let Some(origin) = origin {
            headers.insert(header::ORIGIN, HeaderValue::from_static(origin));
        }
        headers
    }

    fn allowed(method: &Method, site: Option<&'static str>, origin: Option<&'static str>) -> bool {
        is_request_allowed(method, &headers(site, origin), &[CONFIGURED.to_string()])
    }

    const UNSAFE_METHODS: [Method; 4] = [Method::POST, Method::PUT, Method::PATCH, Method::DELETE];

    /// Goal: the attack this module exists for is refused. Method: a write from another port
    /// of the same host arrives as `same-site`; from another host as `cross-site`. Every
    /// unsafe method is checked, with and without an `Origin` header.
    #[test]
    fn a_write_from_another_origin_is_refused() {
        for method in &UNSAFE_METHODS {
            for site in ["same-site", "cross-site", "none"] {
                assert!(allowed(method, Some(site), None).not(), "{method} {site}");
                assert!(
                    allowed(method, Some(site), Some("http://localhost:9090")).not(),
                    "{method} {site}"
                );
            }
        }
    }

    /// Goal: the UI's own writes, through the daemon or the dev server's proxy, pass.
    /// Method: `same-origin` with each unsafe method, with an `Origin` that is not
    /// configured, as the proxy forwards the dev server's.
    #[test]
    fn a_same_origin_write_passes() {
        for method in &UNSAFE_METHODS {
            assert!(allowed(method, Some("same-origin"), None), "{method}");
            assert!(
                allowed(method, Some("same-origin"), Some("http://localhost:5173")),
                "{method}"
            );
        }
    }

    /// Goal: clients that are not browsers keep working. Method: writes without
    /// `Sec-Fetch-Site`, with and without an `Origin`.
    #[test]
    fn a_write_without_fetch_metadata_passes() {
        for method in &UNSAFE_METHODS {
            assert!(allowed(method, None, None), "{method}");
            assert!(
                allowed(method, None, Some("http://localhost:9090")),
                "{method}"
            );
        }
    }

    /// Goal: an origin from the `origins` setting may write, and only that exact origin.
    /// Method: cross-site writes from the configured origin, then from neighbours that
    /// differ by port, scheme or a trailing slash.
    #[test]
    fn only_an_exactly_configured_origin_may_write_cross_origin() {
        assert!(allowed(&Method::POST, Some("cross-site"), Some(CONFIGURED)));
        assert!(allowed(
            &Method::DELETE,
            Some("same-site"),
            Some(CONFIGURED)
        ));
        for neighbour in [
            "https://coolercontrol.home.lan:8443",
            "http://coolercontrol.home.lan",
            "https://coolercontrol.home.lan/",
            "https://evil.coolercontrol.home.lan",
        ] {
            assert!(
                allowed(&Method::POST, Some("cross-site"), Some(neighbour)).not(),
                "{neighbour}"
            );
        }
    }

    /// Goal: reads are never refused here. Method: safe methods from another origin.
    #[test]
    fn a_read_from_another_origin_passes() {
        for method in [Method::GET, Method::HEAD, Method::OPTIONS] {
            assert!(allowed(&method, Some("cross-site"), None), "{method}");
            assert!(allowed(&method, Some("same-site"), None), "{method}");
        }
    }

    /// Goal: a bearer token does not open the cookie path. Method: a same-site write with
    /// an `Authorization: Bearer` header, as a page could add to reach a session-only route.
    #[test]
    fn a_bearer_header_earns_no_exemption() {
        let mut headers = headers(Some("same-site"), Some("http://localhost:9090"));
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Bearer junk"),
        );
        assert!(is_request_allowed(&Method::POST, &headers, &[]).not());
    }

    /// Goal: the middleware turns a refusal into a 403 that never reaches the route, and lets
    /// an allowed write through untouched. Method: a route that counts its calls, behind the
    /// middleware with one configured origin.
    #[tokio::test]
    async fn the_middleware_refuses_with_403_and_skips_the_route() {
        use axum::body::Body;
        use axum::routing::post;
        use axum::Router;
        use std::sync::atomic::{AtomicUsize, Ordering};
        use tower::ServiceExt;

        let calls = Arc::new(AtomicUsize::new(0));
        let route_calls = Arc::clone(&calls);
        let configured: ConfiguredOrigins = Arc::from([CONFIGURED.to_string()]);
        let app = Router::new()
            .route(
                "/write",
                post(move || async move {
                    route_calls.fetch_add(1, Ordering::Relaxed);
                    "written"
                }),
            )
            .layer(OriginGuardLayer::new(configured));
        let send = |site: &'static str, origin: &'static str| {
            let request = axum::http::Request::post("/write")
                .header(SEC_FETCH_SITE, site)
                .header(header::ORIGIN, origin)
                .body(Body::empty())
                .unwrap();
            app.clone().oneshot(request)
        };

        let refused = send("same-site", "http://localhost:9090").await.unwrap();
        assert_eq!(refused.status(), axum::http::StatusCode::FORBIDDEN);
        assert_eq!(calls.load(Ordering::Relaxed), 0);

        let same_origin = send("same-origin", "http://localhost:11987").await.unwrap();
        assert_eq!(same_origin.status(), axum::http::StatusCode::OK);
        let configured = send("cross-site", CONFIGURED).await.unwrap();
        assert_eq!(configured.status(), axum::http::StatusCode::OK);
        assert_eq!(calls.load(Ordering::Relaxed), 2);
    }

    /// Goal: only `http(s)` entries become configured origins. Method: a mixed list.
    #[test]
    fn configured_origins_keep_only_web_origins() {
        let origins = configured_origins(vec![
            "https://a.lan".to_string(),
            "http://b.lan:8080".to_string(),
            "a.lan".to_string(),
            "file://x".to_string(),
            String::new(),
        ]);
        assert_eq!(origins.as_ref(), ["https://a.lan", "http://b.lan:8080"]);
    }
}
