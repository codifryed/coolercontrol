// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `Last-Modified` and `ETag` for the files the daemon embeds at build time.
//!
//! `include_dir` can record each file's times, but drops them all when any one is unreadable,
//! and musl cannot read the creation time. So build.rs records one time for every embedded
//! asset, and [`LastModified`] serves it. Packagers can pin that time across releases through
//! `SOURCE_DATE_EPOCH`, so build.rs also hashes the content into an `ETag` that cannot be.

use axum::body::{Body, Bytes, HttpBody};
use axum::http::header::{CONTENT_TYPE, ETAG, IF_MODIFIED_SINCE, IF_NONE_MATCH, LAST_MODIFIED};
use axum::http::{HeaderValue, Method, Request, Response, StatusCode};
use axum::BoxError;
use chrono::{DateTime, Utc};
use pin_project_lite::pin_project;
use std::future::Future;
use std::ops::Not;
use std::pin::Pin;
use std::sync::LazyLock;
use std::task::{ready, Context, Poll};
use tower::Service;

/// When the embedded assets were last modified, from build.rs.
const ASSETS_MODIFIED_UNIX_SECS: u64 =
    match u64::from_str_radix(env!("CC_ASSETS_MODIFIED_UNIX_SECS"), 10) {
        Ok(unix_secs) => unix_secs,
        Err(_) => panic!("build.rs emits a decimal Unix time"),
    };
/// 9999-12-31T23:59:59Z, the last time an HTTP date's four-digit year can show.
const HTTP_DATE_MAX_UNIX_SECS: u64 = 253_402_300_799;
const _: () = assert!(ASSETS_MODIFIED_UNIX_SECS <= HTTP_DATE_MAX_UNIX_SECS);
/// A quoted hash of the embedded assets' paths and bytes, from build.rs.
const ASSETS_ETAG: &str = env!("CC_ASSETS_ETAG");
/// The same hash tagged for the gzip copies, which api/base.rs swaps in.
pub const ASSETS_ETAG_GZIP: &str = env!("CC_ASSETS_ETAG_GZIP");

/// Formatted once, so every response borrows it rather than allocating.
static ASSETS_LAST_MODIFIED: LazyLock<String> =
    LazyLock::new(|| http_date(ASSETS_MODIFIED_UNIX_SECS));

/// Wraps a static file service: each file it serves gets the build's validators, and a
/// request that already holds them gets a 304. An `If-None-Match` holding the gzip tag
/// matches too, since only the outer layer knows which coding it sends.
///
/// One pair for every file means any change revalidates all of them, which is cheap: the
/// hashed bundles are pinned and only a few unhashed files remain. The match is exact, as nginx
/// does by default, so after a downgrade a newer cached copy is replaced rather than kept. The
/// inner service still runs first, so only a path that names a file gets a 304.
///
/// A plain service rather than `middleware::from_fn`, which boxes a future and the next
/// service on every request. This adds no allocation to the request it wraps.
#[derive(Clone)]
pub struct LastModified<S> {
    inner: S,
    /// Backed by static bytes, so the clone per request copies a pointer.
    date: HeaderValue,
    /// Static too.
    etag: HeaderValue,
    /// Static too. Only matched here; api/base.rs sends it.
    gzip_etag: HeaderValue,
}

impl<S> LastModified<S> {
    pub fn new(inner: S) -> Self {
        Self::with_values(
            inner,
            ASSETS_LAST_MODIFIED.as_str(),
            (ASSETS_ETAG, ASSETS_ETAG_GZIP),
        )
    }

    fn with_values(
        inner: S,
        last_modified: &'static str,
        (etag, gzip_etag): (&'static str, &'static str),
    ) -> Self {
        Self {
            inner,
            date: HeaderValue::from_static(last_modified),
            etag: HeaderValue::from_static(etag),
            gzip_etag: HeaderValue::from_static(gzip_etag),
        }
    }
}

impl<S, ReqBody, ResBody> Service<Request<ReqBody>> for LastModified<S>
where
    S: Service<Request<ReqBody>, Response = Response<ResBody>>,
    ResBody: HttpBody<Data = Bytes> + Send + 'static,
    ResBody::Error: Into<BoxError>,
{
    type Response = Response<Body>;
    type Error = S::Error;
    type Future = LastModifiedFuture<S::Future>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: Request<ReqBody>) -> Self::Future {
        let etags = [&self.etag, &self.gzip_etag];
        let holds_current = holds_validator(&request, &self.date, etags);
        LastModifiedFuture {
            inner: self.inner.call(request),
            last_modified: self.date.clone(),
            etag: self.etag.clone(),
            holds_current,
        }
    }
}

pin_project! {
    /// The inner service's future, finished by [`apply_validators`].
    pub struct LastModifiedFuture<F> {
        #[pin]
        inner: F,
        last_modified: HeaderValue,
        etag: HeaderValue,
        holds_current: bool,
    }
}

impl<F, ResBody, E> Future for LastModifiedFuture<F>
where
    F: Future<Output = Result<Response<ResBody>, E>>,
    ResBody: HttpBody<Data = Bytes> + Send + 'static,
    ResBody::Error: Into<BoxError>,
{
    type Output = Result<Response<Body>, E>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();
        let response = ready!(this.inner.poll(cx))?;
        let validators = (this.last_modified.clone(), this.etag.clone());
        Poll::Ready(Ok(apply_validators(
            response,
            validators,
            *this.holds_current,
        )))
    }
}

/// Whether `request` is a conditional GET or HEAD that holds the current validators. Other
/// methods ignore the condition, as RFC 9110 specifies.
///
/// An `If-None-Match` decides alone (RFC 9110 13.2.2), so a pinned date cannot vouch for an
/// outdated copy. It holds the build when it names any of `etags`. Without one,
/// `If-Modified-Since` must be exactly `last_modified`.
fn holds_validator<B>(
    request: &Request<B>,
    last_modified: &HeaderValue,
    etags: [&HeaderValue; 2],
) -> bool {
    if matches!(*request.method(), Method::GET | Method::HEAD).not() {
        return false;
    }
    let headers = request.headers();
    let mut if_none_match = headers.get_all(IF_NONE_MATCH).iter().peekable();
    if if_none_match.peek().is_some() {
        return if_none_match.any(|tags| etags.iter().any(|etag| lists_etag(tags, etag)));
    }
    headers.get(IF_MODIFIED_SINCE) == Some(last_modified)
}

/// Whether the `If-None-Match` list `tags` names `etag`, compared weakly as RFC 9110 asks.
fn lists_etag(tags: &HeaderValue, etag: &HeaderValue) -> bool {
    tags.as_bytes().split(|byte| *byte == b',').any(|tag| {
        let tag = tag.trim_ascii();
        tag.strip_prefix(b"W/").unwrap_or(tag) == etag.as_bytes()
    })
}

/// Adds the validators to a served file, or turns it into a 304 when the client holds them.
/// Anything but a 200 passes through. The 304 keeps the response's own header map, and its
/// static body is dropped unread.
fn apply_validators<B>(
    response: Response<B>,
    (last_modified, etag): (HeaderValue, HeaderValue),
    holds_current: bool,
) -> Response<Body>
where
    B: HttpBody<Data = Bytes> + Send + 'static,
    B::Error: Into<BoxError>,
{
    if response.status() != StatusCode::OK {
        return response.map(Body::new);
    }
    let (mut parts, body) = response.into_parts();
    // The inner service runs without include_dir metadata, so these are the only validators.
    debug_assert!(
        parts.headers.contains_key(LAST_MODIFIED).not(),
        "the inner service sets no Last-Modified"
    );
    debug_assert!(
        parts.headers.contains_key(ETAG).not(),
        "the inner service sets no ETag"
    );
    parts.headers.insert(LAST_MODIFIED, last_modified);
    parts.headers.insert(ETAG, etag);
    if holds_current.not() {
        return Response::from_parts(parts, Body::new(body));
    }
    // RFC 9110 keeps representation metadata off a 304.
    parts.status = StatusCode::NOT_MODIFIED;
    parts.headers.remove(CONTENT_TYPE);
    Response::from_parts(parts, Body::empty())
}

/// `unix_secs` as an HTTP date (IMF-fixdate), such as `Sun, 06 Nov 1994 08:49:37 GMT`.
fn http_date(unix_secs: u64) -> String {
    assert!(
        unix_secs <= HTTP_DATE_MAX_UNIX_SECS,
        "four-digit years only"
    );
    // Neither can fail: the bound above keeps the value well inside i64 and chrono's range.
    let unix_secs = i64::try_from(unix_secs).expect("bounded above");
    let date = DateTime::<Utc>::from_timestamp(unix_secs, 0).expect("bounded above");
    let formatted = date.format("%a, %d %b %Y %H:%M:%S GMT").to_string();
    assert_eq!(formatted.len(), 29, "IMF-fixdate is fixed width");
    formatted
}

#[cfg(test)]
mod tests {
    use super::*;
    use include_dir::{Dir, DirEntry, File};
    use tower::ServiceExt;
    use tower_serve_static::ServeDir;

    const TEST_LAST_MODIFIED: &str = "Sun, 06 Nov 1994 08:49:37 GMT";
    const TEST_ETAG: &str = "\"0123abcd\"";
    const TEST_ETAG_GZIP: &str = "\"0123abcd-gzip\"";

    static TEST_ASSETS: Dir<'static> = Dir::new(
        "",
        &[
            DirEntry::File(File::new("index.html", b"original")),
            DirEntry::Dir(Dir::new(
                "icons",
                &[DirEntry::File(File::new("icons/app.png", b"png"))],
            )),
        ],
    );

    fn request(method: Method, path: &str, if_modified_since: Option<&str>) -> Request<Body> {
        let mut builder = Request::builder().method(method).uri(path);
        if let Some(value) = if_modified_since {
            builder = builder.header(IF_MODIFIED_SINCE, value);
        }
        builder.body(Body::empty()).unwrap()
    }

    /// A GET for `/index.html` carrying `if_none_match` and, optionally, a date.
    fn tagged_request(if_none_match: &str, if_modified_since: Option<&str>) -> Request<Body> {
        let mut request = request(Method::GET, "/index.html", if_modified_since);
        let tags = HeaderValue::from_str(if_none_match).unwrap();
        request.headers_mut().insert(IF_NONE_MATCH, tags);
        request
    }

    /// `request` through a real `ServeDir` over `TEST_ASSETS`, wrapped.
    async fn serve(request: Request<Body>) -> Response<Body> {
        let etags = (TEST_ETAG, TEST_ETAG_GZIP);
        LastModified::with_values(ServeDir::new(&TEST_ASSETS), TEST_LAST_MODIFIED, etags)
            .oneshot(request)
            .await
            .unwrap()
    }

    async fn body_bytes(response: Response<Body>) -> Vec<u8> {
        axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap()
            .to_vec()
    }

    // Goal: the header is a valid IMF-fixdate at both ends of its range. Method: the
    // example date from RFC 9110, the epoch, and the last representable second.
    #[test]
    fn test_http_date() {
        assert_eq!(http_date(784_111_777), TEST_LAST_MODIFIED);
        assert_eq!(http_date(0), "Thu, 01 Jan 1970 00:00:00 GMT");
        assert_eq!(
            http_date(HTTP_DATE_MAX_UNIX_SECS),
            "Fri, 31 Dec 9999 23:59:59 GMT"
        );
    }

    // Goal: negative space for the bound. A year past 9999 cannot fit the format. Method:
    // format the second after the last representable one and expect the assertion.
    #[test]
    #[should_panic(expected = "four-digit years only")]
    fn test_http_date_rejects_five_digit_years() {
        http_date(HTTP_DATE_MAX_UNIX_SECS + 1);
    }

    // Goal: the production values are the build's time, formatted as a header can carry it,
    // and its content hash as a quoted strong ETag. Method: compare the build.rs values with
    // what `LastModified::new` carries.
    #[test]
    fn test_assets_last_modified_is_the_build_time() {
        assert_eq!(*ASSETS_LAST_MODIFIED, http_date(ASSETS_MODIFIED_UNIX_SECS));
        let service = LastModified::new(());
        assert_eq!(service.date, ASSETS_LAST_MODIFIED.as_str());
        assert_eq!(service.etag, ASSETS_ETAG);
        assert_eq!(service.gzip_etag, ASSETS_ETAG_GZIP);
        let unquoted = ASSETS_ETAG.trim_end_matches('"');
        assert_eq!(ASSETS_ETAG_GZIP, format!("{unquoted}-gzip\""));
        assert!(ASSETS_ETAG.len() > 2);
        assert!(
            ASSETS_ETAG.starts_with('"'),
            "{ASSETS_ETAG} opens with a quote"
        );
        assert!(
            ASSETS_ETAG.ends_with('"'),
            "{ASSETS_ETAG} closes with a quote"
        );
    }

    // Goal: a plain request gets the full body, its content type, and both validators.
    // Method: an unconditional GET through a real `ServeDir`.
    #[tokio::test]
    async fn test_served_file_carries_last_modified() {
        let response = serve(request(Method::GET, "/index.html", None)).await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()[LAST_MODIFIED], TEST_LAST_MODIFIED);
        assert_eq!(response.headers()[ETAG], TEST_ETAG);
        assert_eq!(response.headers()[CONTENT_TYPE], "text/html");
        assert_eq!(body_bytes(response).await, b"original");
    }

    // Goal: a GET or HEAD that holds the current validator gets an empty 304 that repeats
    // it and carries no content type. Method: GET and HEAD with the exact date through a
    // real `ServeDir`.
    #[tokio::test]
    async fn test_matching_validator_is_not_modified() {
        for method in [Method::GET, Method::HEAD] {
            let since = Some(TEST_LAST_MODIFIED);
            let response = serve(request(method.clone(), "/index.html", since)).await;
            assert_eq!(response.status(), StatusCode::NOT_MODIFIED, "{method}");
            assert_eq!(response.headers()[LAST_MODIFIED], TEST_LAST_MODIFIED);
            assert_eq!(response.headers()[ETAG], TEST_ETAG);
            assert!(response.headers().get(CONTENT_TYPE).is_none(), "{method}");
            assert!(body_bytes(response).await.is_empty(), "{method}");
        }
    }

    // Goal: a client holding the current ETag, plain or gzip, gets a 304. Method: each tag
    // alone, weak, and inside a list, each with no date.
    #[tokio::test]
    async fn test_matching_etag_is_not_modified() {
        for tags in [
            TEST_ETAG,
            "W/\"0123abcd\"",
            "\"old\", \"0123abcd\"",
            TEST_ETAG_GZIP,
            "W/\"0123abcd-gzip\"",
        ] {
            let response = serve(tagged_request(tags, None)).await;
            assert_eq!(response.status(), StatusCode::NOT_MODIFIED, "{tags}");
            assert_eq!(response.headers()[ETAG], TEST_ETAG);
            assert!(body_bytes(response).await.is_empty(), "{tags}");
        }
    }

    // Goal: the upgrade under a pinned SOURCE_DATE_EPOCH. The date still matches but the
    // content changed, so the stale ETag must win and the body be sent. Method: a
    // mismatching ETag and a prefix of the current one, each with the matching date.
    #[tokio::test]
    async fn test_mismatching_etag_overrides_matching_date() {
        for tags in ["\"old\"", "\"0123abc\"", "\"0123abcd-br\""] {
            let response = serve(tagged_request(tags, Some(TEST_LAST_MODIFIED))).await;
            assert_eq!(response.status(), StatusCode::OK, "{tags}");
            assert_eq!(response.headers()[ETAG], TEST_ETAG);
            assert_eq!(body_bytes(response).await, b"original", "{tags}");
        }
    }

    // Goal: anything but an exact match gets the full body. Method: an older date (an
    // upgrade), a newer one (a downgrade, which a date comparison would wrongly 304), and
    // garbage.
    #[tokio::test]
    async fn test_other_validators_get_the_body() {
        for since in [
            "Sat, 05 Nov 1994 08:49:37 GMT",
            "Mon, 07 Nov 1994 08:49:37 GMT",
            "not a date",
        ] {
            let response = serve(request(Method::GET, "/index.html", Some(since))).await;
            assert_eq!(response.status(), StatusCode::OK, "{since}");
            assert_eq!(response.headers()[LAST_MODIFIED], TEST_LAST_MODIFIED);
            assert_eq!(body_bytes(response).await, b"original", "{since}");
        }
    }

    // Goal: only a served file is validated. A 404 and a directory redirect keep their status
    // and gain no validator, and a method the condition does not apply to is not a 304.
    // Method: the current date on a missing path, a directory, and a POST.
    #[tokio::test]
    async fn test_validator_skips_non_files_and_other_methods() {
        for (path, status) in [
            ("/missing.html", StatusCode::NOT_FOUND),
            ("/icons", StatusCode::TEMPORARY_REDIRECT),
        ] {
            let response = serve(request(Method::GET, path, Some(TEST_LAST_MODIFIED))).await;
            assert_eq!(response.status(), status, "{path}");
            assert!(response.headers().get(LAST_MODIFIED).is_none(), "{path}");
            assert!(response.headers().get(ETAG).is_none(), "{path}");
        }
        let since = Some(TEST_LAST_MODIFIED);
        let response = serve(request(Method::POST, "/index.html", since)).await;
        assert_ne!(response.status(), StatusCode::NOT_MODIFIED);
    }

    // Goal: the wrapper adds no allocation to the request it wraps, and a 304 costs less than
    // serving the file. Method: count one request through a bare `ServeDir`, turned into an
    // axum response as a router would, against the same request wrapped, for a 200 and a 304
    // by date and by ETag.
    #[tokio::test]
    #[cfg(feature = "gated-tests")]
    async fn test_wrapper_adds_no_allocations() {
        use crate::alloc_count::count_allocations_async;
        use axum::response::IntoResponse;

        let bare_request = request(Method::GET, "/index.html", None);
        let (response, bare) = count_allocations_async(async {
            ServeDir::new(&TEST_ASSETS)
                .oneshot(bare_request)
                .await
                .unwrap()
                .into_response()
        })
        .await;
        assert_eq!(response.status(), StatusCode::OK);

        let wrapped_request = request(Method::GET, "/index.html", None);
        let (response, wrapped) =
            count_allocations_async(async { serve(wrapped_request).await.into_response() }).await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(wrapped, bare, "a served file costs what it costs unwrapped");

        let revalidation = request(Method::GET, "/index.html", Some(TEST_LAST_MODIFIED));
        let (response, not_modified) =
            count_allocations_async(async { serve(revalidation).await.into_response() }).await;
        assert_eq!(response.status(), StatusCode::NOT_MODIFIED);
        assert!(
            not_modified < bare,
            "a 304 skips boxing the body: {not_modified} vs {bare}"
        );

        let tagged = tagged_request(TEST_ETAG, None);
        let (response, tagged_not_modified) =
            count_allocations_async(async { serve(tagged).await.into_response() }).await;
        assert_eq!(response.status(), StatusCode::NOT_MODIFIED);
        assert_eq!(
            tagged_not_modified, not_modified,
            "matching the ETag allocates nothing"
        );
    }
}
