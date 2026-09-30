// SPDX-FileCopyrightText: 2024 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use crate::admin;
use crate::api::actor::{TokenCheck, TokenHandle, TokenValidation};
use crate::api::auth_throttle::{self, mark, CredentialOutcome};
use crate::api::peer::PeerKey;
use crate::api::{AppState, CCError};
use aide::axum::IntoApiResponse;
use aide::NoApi;
use anyhow::Result;
use axum::extract::{FromRequestParts, Request, State};
use axum::http::header;
use axum::http::request::Parts;
use axum::middleware::Next;
use axum::response::{IntoResponse as _, Response};
use axum::{Extension, Json};
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::time::Instant;
use strum::{Display, EnumString};
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
    ///
    /// Every rejection here is an operating error, not a programmer error: `/login`
    /// carries no auth layer, so this runs on wholly unauthenticated input and must
    /// never assert against it.
    fn parse_header_value(value: &str) -> Result<Self, CCError> {
        let encoded = value
            .strip_prefix("Basic ")
            .ok_or_else(|| CCError::InvalidCredentials {
                msg: "Authorization header must use Basic scheme.".to_string(),
            })?;
        if encoded.is_empty() {
            return Err(CCError::InvalidCredentials {
                msg: "Authorization header has an empty base64 payload.".to_string(),
            });
        }
        let decoded_bytes = BASE64
            .decode(encoded)
            .map_err(|_| CCError::InvalidCredentials {
                msg: "Invalid base64 in Authorization header.".to_string(),
            })?;
        if decoded_bytes.len() > MAX_BASIC_AUTH_DECODED_BYTES {
            return Err(CCError::InvalidCredentials {
                msg: "Authorization header credentials are too long.".to_string(),
            });
        }
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
        if username.is_empty() {
            return Err(CCError::InvalidCredentials {
                msg: "Authorization header has an empty username.".to_string(),
            });
        }
        Ok(Self {
            username: username.to_string(),
            password: password.to_string(),
        })
    }
}

impl<S: Send + Sync> FromRequestParts<S> for BasicAuth {
    type Rejection = CCError;

    // Signature is fixed by axum's FromRequestParts; this body needs no await.
    #[allow(clippy::unused_async_trait_impl)]
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

/// The bearer token a request presents, if it presents one at all.
fn bearer_token(request: &Request) -> Option<String> {
    let value = request
        .headers()
        .get(header::AUTHORIZATION)?
        .to_str()
        .ok()?;
    value.strip_prefix("Bearer ").map(str::to_string)
}

/// Left unmarked: the token was never checked, so its legacy charge is refunded.
fn token_check_busy() -> CCError {
    CCError::TooManyAttempts {
        msg: "Too many token checks in progress. Try again shortly.".to_string(),
    }
}

fn invalid_token() -> Response {
    CCError::InvalidCredentials {
        msg: "Invalid or expired access token.".to_string(),
    }
    .into_response()
}

/// Who records a bearer verdict against the peer's token budget.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Recorded {
    /// `token_throttle_middleware`, from the mark on the response.
    ByMark,
    /// Nobody further: the legacy check was charged on arrival and is already settled.
    Settled,
}

/// Marks `response` for the token throttle unless its verdict is already settled, which
/// would count it twice.
fn mark_unsettled(response: Response, outcome: CredentialOutcome, recorded: Recorded) -> Response {
    match recorded {
        Recorded::ByMark => mark(response, outcome),
        Recorded::Settled => response,
    }
}

/// Runs the free digest pass, then, only if it misses, the legacy argon2 pass charged to
/// `peer` on arrival. A request dropped mid-hash keeps its charge; `Busy` and an internal
/// error refund it. `Err` means the peer is in backoff, or the pass failed.
async fn validate_bearer(
    token_handle: &TokenHandle,
    raw_token: String,
    peer: Option<PeerKey>,
) -> Result<(TokenValidation, Recorded), CCError> {
    let legacy = match token_handle.check_digest(raw_token).await {
        TokenCheck::Done(validation) => {
            debug_assert!(
                validation != TokenValidation::Busy,
                "only the legacy pass fills"
            );
            return Ok((validation, Recorded::ByMark));
        }
        TokenCheck::Legacy(legacy) => legacy,
    };
    let attempt = match peer {
        Some(peer) => Some(auth_throttle::admit_legacy_token_check(
            peer,
            Instant::now(),
        )?),
        None => None,
    };
    let result = legacy.run().await;
    if let Some(attempt) = attempt {
        attempt.settle(legacy_outcome(&result), Instant::now());
    }
    match result {
        Ok(validation) => Ok((validation, Recorded::Settled)),
        Err(_) => Err(CCError::InternalError {
            msg: "Token validation error.".to_string(),
        }),
    }
}

fn legacy_outcome(result: &Result<TokenValidation>) -> Option<CredentialOutcome> {
    match result {
        Ok(TokenValidation::ValidReadWrite | TokenValidation::ValidReadOnly) => {
            Some(CredentialOutcome::Accepted)
        }
        Ok(TokenValidation::Invalid) => Some(CredentialOutcome::Rejected),
        Ok(TokenValidation::Busy) | Err(_) => None,
    }
}

/// Read-access middleware. Validates Bearer tokens (any valid token) or
/// session cookies. Used for read-only routes.
///
/// Every bearer outcome is recorded for `auth_throttle`, which counts only credentials
/// actually adjudicated here. A dispatch failure is left unmarked: it says nothing about
/// the token.
pub async fn auth_middleware(
    Extension(token_handle): Extension<TokenHandle>,
    session: Session,
    request: Request,
    next: Next,
) -> impl IntoApiResponse {
    if let Some(raw_token) = bearer_token(&request) {
        let peer = auth_throttle::peer_key(&request);
        let (validation, recorded) = match validate_bearer(&token_handle, raw_token, peer).await {
            Ok(checked) => checked,
            Err(err) => return Err(err),
        };
        return match validation {
            TokenValidation::ValidReadWrite | TokenValidation::ValidReadOnly => {
                let response = next.run(request).await;
                Ok(mark_unsettled(
                    response,
                    CredentialOutcome::Accepted,
                    recorded,
                ))
            }
            TokenValidation::Invalid => Ok(mark_unsettled(
                invalid_token(),
                CredentialOutcome::Rejected,
                recorded,
            )),
            TokenValidation::Busy => Err(token_check_busy()),
        };
    }
    check_session_permission(session, request, next).await
}

/// Write-access middleware. Validates Bearer tokens (requires write access)
/// or session cookies. Used for write/mutating routes.
/// An under-scoped token is recorded `Accepted`: it authenticated, and only the
/// authorization check refused it. Counting it would throttle a client holding a
/// perfectly valid credential.
pub async fn auth_write_middleware(
    Extension(token_handle): Extension<TokenHandle>,
    session: Session,
    request: Request,
    next: Next,
) -> impl IntoApiResponse {
    if let Some(raw_token) = bearer_token(&request) {
        let peer = auth_throttle::peer_key(&request);
        let (validation, recorded) = match validate_bearer(&token_handle, raw_token, peer).await {
            Ok(checked) => checked,
            Err(err) => return Err(err),
        };
        return match validation {
            TokenValidation::ValidReadWrite => {
                let response = next.run(request).await;
                Ok(mark_unsettled(
                    response,
                    CredentialOutcome::Accepted,
                    recorded,
                ))
            }
            TokenValidation::ValidReadOnly => Ok(mark_unsettled(
                CCError::InsufficientScope {
                    msg: "This token does not have write access.".to_string(),
                }
                .into_response(),
                CredentialOutcome::Accepted,
                recorded,
            )),
            TokenValidation::Invalid => Ok(mark_unsettled(
                invalid_token(),
                CredentialOutcome::Rejected,
                recorded,
            )),
            TokenValidation::Busy => Err(token_check_busy()),
        };
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

/// Grants an admin session the way a successful `/login` does, for tests of the layers
/// that sit behind the session check.
#[cfg(test)]
pub async fn grant_admin_session(session: &Session) {
    session
        .insert(SESSION_PERMISSIONS, Permission::Admin)
        .await
        .unwrap();
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
///
/// The default-password check is an argon2 verify, so it goes through the auth actor, which
/// runs one at a time, rather than letting concurrent calls hash in parallel.
pub async fn verify_session(
    State(AppState { auth_handle, .. }): State<AppState>,
) -> Result<(), CCError> {
    if auth_handle
        .match_passwd(admin::DEFAULT_PASS.to_string())
        .await?
    {
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
    use std::str::FromStr;

    /// Goal: the printed and parsed forms of a Permission must stay the same
    /// string, since Display and EnumString are derived independently.
    #[test]
    fn permission_display_round_trips_through_parsing() {
        for permission in [Permission::Admin, Permission::Guest] {
            let printed = permission.to_string();
            let parsed = Permission::from_str(&printed).expect("printed form must parse back");
            assert_eq!(parsed.to_string(), printed);
        }
        assert_eq!(Permission::Admin.to_string(), "Admin");
        assert_eq!(Permission::Guest.to_string(), "Guest");
    }

    /// Goal: a busy legacy pass answers 429 with no verdict, so the token throttle charges
    /// nothing for a token it never checked.
    #[test]
    fn busy_token_check_is_an_unmarked_429() {
        let response = token_check_busy().into_response();
        assert_eq!(response.status(), axum::http::StatusCode::TOO_MANY_REQUESTS);
        assert!(response.extensions().get::<CredentialOutcome>().is_none());
    }

    /// Both bearer layers wired as the router wires them, inside the token throttle.
    fn token_app(token_handle: TokenHandle) -> axum::Router {
        use crate::api::session_store::MemorySessionStore;
        use axum::middleware::from_fn;
        use axum::routing::get;
        use tower_sessions::SessionManagerLayer;
        let ok = || async { axum::http::StatusCode::OK };
        axum::Router::new()
            .route("/read", get(ok).layer(from_fn(auth_middleware)))
            .route("/write", get(ok).layer(from_fn(auth_write_middleware)))
            .layer(Extension(token_handle))
            .layer(SessionManagerLayer::new(MemorySessionStore::new(4)))
            .layer(from_fn(auth_throttle::token_throttle_middleware))
    }

    /// The throttle statics are process-wide, so each test owns one TEST-NET-2 address.
    fn test_peer(last_octet: u8) -> std::net::SocketAddr {
        std::net::SocketAddr::from(([198, 51, 100, last_octet], 40000))
    }

    async fn call(
        app: &axum::Router,
        uri: &str,
        raw_token: &str,
        peer: std::net::SocketAddr,
    ) -> Response {
        let mut request = Request::get(uri)
            .header(header::AUTHORIZATION, format!("Bearer {raw_token}"))
            .body(axum::body::Body::empty())
            .unwrap();
        request
            .extensions_mut()
            .insert(axum::extract::ConnectInfo(peer));
        tower::ServiceExt::oneshot(app.clone(), request)
            .await
            .unwrap()
    }

    /// A token as stored since 5.0.0 (`digest`) or before it (`legacy`).
    fn stored_token(raw: &str, digest: bool) -> crate::token::StoredToken {
        crate::token::StoredToken {
            id: uuid::Uuid::new_v4().to_string(),
            label: "Test Token".to_string(),
            hash: crate::token::hash_token(raw).unwrap(),
            digest: digest.then(|| crate::token::digest_token(raw)),
            created_at: chrono::Local::now(),
            expires_at: None,
            last_used: None,
            write_access: true,
        }
    }

    fn legacy_app() -> (axum::Router, String) {
        let raw = crate::token::generate_token();
        let app = token_app(TokenHandle::with_tokens(vec![stored_token(&raw, false)]));
        (app, raw)
    }

    fn unknown_token() -> String {
        crate::token::generate_token()
    }

    /// Goal: a legacy check dropped mid-hash, as by a client reset, stays charged, so a
    /// peer that keeps resetting them backs off instead of holding the shared pass free.
    /// Method: each request is polled once, far enough to be charged and start its pass,
    /// then dropped.
    #[tokio::test]
    async fn dropped_legacy_checks_stay_charged() {
        use futures_util::FutureExt as _;
        let peer = test_peer(40);
        let (app, _) = legacy_app();
        for _ in 0..=auth_throttle::FAILURE_THRESHOLD {
            drop(call(&app, "/read", &unknown_token(), peer).now_or_never());
        }
        assert_eq!(
            auth_throttle::token_failures(peer.ip()),
            Some(auth_throttle::FAILURE_THRESHOLD + 1)
        );
        let response = call(&app, "/read", &unknown_token(), peer).await;
        assert_eq!(response.status(), axum::http::StatusCode::TOO_MANY_REQUESTS);
    }

    /// Goal: a burst of valid digest tokens from one peer, past the failure allowance, is
    /// never refused or charged, even with legacy tokens stored.
    #[tokio::test]
    async fn valid_digest_burst_is_never_charged() {
        let peer = test_peer(41);
        let raw = crate::token::generate_token();
        let tokens = vec![
            stored_token(&unknown_token(), false),
            stored_token(&raw, true),
        ];
        let app = token_app(TokenHandle::with_tokens(tokens));
        let burst = (auth_throttle::FAILURE_THRESHOLD + 1) * 3;
        let calls = (0..burst).map(|_| call(&app, "/read", &raw, peer));
        let responses = futures_util::future::join_all(calls).await;
        for response in responses {
            assert_eq!(response.status(), axum::http::StatusCode::OK);
        }
        assert_eq!(auth_throttle::token_failures(peer.ip()), None);
    }

    /// Goal: a legacy check refused because the pass is full gives its charge back, since
    /// the token was never checked. Method: one rejection first, so a kept charge would
    /// show as a second failure; the pass is then filled by requests outside the throttle.
    #[tokio::test]
    async fn busy_legacy_check_refunds_its_charge() {
        let peer = test_peer(42);
        let handle = TokenHandle::with_tokens(vec![stored_token(&unknown_token(), false)]);
        let app = token_app(handle.clone());
        let rejected = call(&app, "/read", &unknown_token(), peer).await;
        assert_eq!(rejected.status(), axum::http::StatusCode::UNAUTHORIZED);
        let (_permit, parked) = handle.fill_legacy_pass().await;
        let busy = call(&app, "/read", &unknown_token(), peer).await;
        assert_eq!(busy.status(), axum::http::StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(auth_throttle::token_failures(peer.ip()), Some(1));
        parked.iter().for_each(tokio::task::JoinHandle::abort);
    }

    /// Goal: a legacy token that matches clears the peer's streak through its settled
    /// charge, and its response is left unmarked, so the throttle records it only once.
    #[tokio::test]
    async fn legacy_match_clears_the_streak_once() {
        let peer = test_peer(43);
        let (app, raw) = legacy_app();
        for _ in 0..3 {
            let rejected = call(&app, "/read", &unknown_token(), peer).await;
            assert_eq!(rejected.status(), axum::http::StatusCode::UNAUTHORIZED);
        }
        assert_eq!(auth_throttle::token_failures(peer.ip()), Some(3));
        let response = call(&app, "/read", &raw, peer).await;
        assert_eq!(response.status(), axum::http::StatusCode::OK);
        assert!(response.extensions().get::<CredentialOutcome>().is_none());
        assert_eq!(auth_throttle::token_failures(peer.ip()), None);
    }

    /// Goal: an invalid token on the legacy path is charged exactly once, on arrival, and
    /// not again by the throttle reading a mark.
    #[tokio::test]
    async fn invalid_legacy_token_is_charged_once() {
        let peer = test_peer(44);
        let (app, _) = legacy_app();
        let response = call(&app, "/write", &unknown_token(), peer).await;
        assert_eq!(response.status(), axum::http::StatusCode::UNAUTHORIZED);
        assert!(response.extensions().get::<CredentialOutcome>().is_none());
        assert_eq!(auth_throttle::token_failures(peer.ip()), Some(1));
    }

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
        // Goal: an empty username is rejected as bad credentials, not by panicking.
        // `/login` is unauthenticated, so anyone who can reach the port reaches this.
        let header = encode_basic("", "password");
        let err = BasicAuth::parse_header_value(&header).unwrap_err();
        assert!(matches!(err, CCError::InvalidCredentials { .. }));
    }

    #[test]
    fn reject_empty_base64_payload() {
        // Goal: "Basic " with nothing after it is rejected rather than panicking.
        let err = BasicAuth::parse_header_value("Basic ").unwrap_err();
        assert!(matches!(err, CCError::InvalidCredentials { .. }));
    }

    #[test]
    fn reject_oversized_credentials() {
        // Goal: an over-long Basic payload is rejected rather than panicking. The
        // boundary is exercised from both sides so the comparison cannot drift.
        let at_limit = "a".repeat(MAX_BASIC_AUTH_DECODED_BYTES - "user:".len());
        let header = encode_basic("user", &at_limit);
        assert!(BasicAuth::parse_header_value(&header).is_ok());

        let over_limit = "a".repeat(MAX_BASIC_AUTH_DECODED_BYTES);
        let header = encode_basic("user", &over_limit);
        let err = BasicAuth::parse_header_value(&header).unwrap_err();
        assert!(matches!(err, CCError::InvalidCredentials { .. }));
    }

    #[test]
    fn parse_header_value_never_panics_on_arbitrary_input() {
        // Goal: no input reaching this unauthenticated boundary may unwind. Covers the
        // shapes that used to assert, plus adjacent malformed ones.
        let inputs = [
            "",
            "Basic",
            "Basic ",
            "Basic ====",
            "Basic  ",
            &format!("Basic {}", BASE64.encode(":")),
            &format!("Basic {}", BASE64.encode(":pass")),
            &format!("Basic {}", BASE64.encode([0xff, 0xfe])),
            &format!("Basic {}", BASE64.encode("a".repeat(4096))),
        ];
        for input in inputs {
            let result = std::panic::catch_unwind(|| drop(BasicAuth::parse_header_value(input)));
            assert!(result.is_ok(), "panicked on input: {input:?}");
        }
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
