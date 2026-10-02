// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Who may embed the UI in a frame, and the `Content-Security-Policy` values that say so.

use crate::api::{base, plugins};
use axum::http::HeaderValue;
use log::{error, info, warn};
use std::ops::Not;
use thiserror::Error;

/// What the UI document's policy says with no `frame_ancestors` configured.
const DOCUMENT_FRAME_ANCESTORS: &str = "frame-ancestors 'none'";
/// What a plugin page's policy says with no `frame_ancestors` configured: the UI frames it.
const PLUGIN_FRAME_ANCESTORS: &str = "frame-ancestors 'self'";

/// The most origins a config may name. One per address the embedding page is opened at is
/// typical, so a handful.
const FRAME_ANCESTORS_COUNT_MAX: usize = 16;
/// The longest a DNS name may be, and the longest any one label of it.
const HOST_LEN_MAX: usize = 253;
const LABEL_LEN_MAX: usize = 63;

/// The header values that depend on `frame_ancestors`. Composed once at startup, so no
/// request pays for it.
#[derive(Debug, Clone)]
pub struct FramePolicy {
    /// The `Content-Security-Policy` of the UI document.
    pub document_csp: HeaderValue,
    /// The `Content-Security-Policy` of a plugin's UI pages. The UI document frames those,
    /// and a browser checks every ancestor, so whatever frames the UI must be allowed here
    /// as well.
    pub plugin_csp: HeaderValue,
}

impl Default for FramePolicy {
    /// Nothing may frame the UI, and only the UI may frame a plugin page.
    fn default() -> Self {
        Self {
            document_csp: HeaderValue::from_static(base::CONTENT_SECURITY_POLICY),
            plugin_csp: HeaderValue::from_static(plugins::PLUGIN_CONTENT_SECURITY_POLICY),
        }
    }
}

impl FramePolicy {
    /// Widens the default policy by the configured origins. An invalid entry is logged and
    /// skipped: that origin then cannot frame the UI, which is the safe direction to fail.
    pub fn from_config(entries: &[String]) -> Self {
        let origins = valid_origins(entries);
        if origins.is_empty() {
            return Self::default();
        }
        let sources = origins.join(" ");
        let document = base::CONTENT_SECURITY_POLICY.replace(
            DOCUMENT_FRAME_ANCESTORS,
            &format!("frame-ancestors {sources}"),
        );
        let plugin = plugins::PLUGIN_CONTENT_SECURITY_POLICY.replace(
            PLUGIN_FRAME_ANCESTORS,
            &format!("{PLUGIN_FRAME_ANCESTORS} {sources}"),
        );
        debug_assert!(document.contains(&sources));
        debug_assert!(document.contains(DOCUMENT_FRAME_ANCESTORS).not());
        debug_assert!(plugin.contains(&sources));
        let (Ok(document_csp), Ok(plugin_csp)) = (
            HeaderValue::try_from(document),
            HeaderValue::try_from(plugin),
        ) else {
            // Validated origins are visible ASCII, so this is a bug. Keep the UI unframeable.
            error!("The frame_ancestors origins do not form a header value: {sources:?}");
            return Self::default();
        };
        info!("The UI may be embedded in a frame by: {sources}");
        Self {
            document_csp,
            plugin_csp,
        }
    }
}

/// The configured entries that name an origin, as the policy writes them. Whatever is left
/// out is logged.
fn valid_origins(entries: &[String]) -> Vec<&str> {
    let screened = screen(entries);
    for (entry, reason) in &screened.refused {
        warn!("Ignoring frame_ancestors entry {entry:?}: {reason}");
    }
    if screened.is_truncated {
        warn!("Only the first {FRAME_ANCESTORS_COUNT_MAX} frame_ancestors entries are used.");
    }
    screened.origins
}

/// The configured entries, sorted into what the policy takes and what it leaves out.
#[derive(Debug, PartialEq, Eq)]
struct Screened<'a> {
    origins: Vec<&'a str>,
    /// The entries that name no origin, each with the reason.
    refused: Vec<(&'a str, EntryError)>,
    /// Whether a valid origin was dropped for arriving past the cap.
    is_truncated: bool,
}

/// Sorts the entries without logging. Stops at the first valid origin past the cap, so the
/// list only counts as truncated when an origin is dropped.
fn screen(entries: &[String]) -> Screened<'_> {
    let mut origins = Vec::with_capacity(entries.len().min(FRAME_ANCESTORS_COUNT_MAX));
    let mut refused = Vec::new();
    let mut is_truncated = false;
    for entry in entries {
        match ancestor_origin(entry) {
            Ok(origin) => {
                if origins.len() < FRAME_ANCESTORS_COUNT_MAX {
                    origins.push(origin);
                } else {
                    is_truncated = true;
                    break;
                }
            }
            Err(reason) => refused.push((entry.as_str(), reason)),
        }
    }
    debug_assert!(origins.len() <= FRAME_ANCESTORS_COUNT_MAX);
    debug_assert!(origins.len() + refused.len() <= entries.len());
    Screened {
        origins,
        refused,
        is_truncated,
    }
}

/// Why a `frame_ancestors` entry is not used.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
enum EntryError {
    #[error("it must start with http:// or https://")]
    Scheme,
    #[error("it must be an origin without a path, such as https://host:9090")]
    Path,
    #[error(
        "an IPv6 address cannot be written in a Content-Security-Policy, \
        use a hostname or an IPv4 address"
    )]
    Ipv6,
    #[error("the host must be a hostname or an IPv4 address")]
    Host,
    #[error("the port must be a number up to 65535")]
    Port,
}

/// The origin an entry names, as the policy writes it: `scheme://host[:port]`.
///
/// Only an exact origin is taken. A wildcard or a keyword would let more frame the UI than
/// the entry reads as, and a path never matches, since browsers compare ancestors by origin.
/// What is returned holds nothing that could end the directive or start another.
fn ancestor_origin(entry: &str) -> Result<&str, EntryError> {
    let after_scheme = entry
        .strip_prefix("https://")
        .or_else(|| entry.strip_prefix("http://"))
        .ok_or(EntryError::Scheme)?;
    // An origin's own trailing slash, as an address bar shows it.
    let authority = after_scheme.strip_suffix('/').unwrap_or(after_scheme);
    if authority.contains('/') {
        return Err(EntryError::Path);
    }
    if authority.starts_with('[') {
        return Err(EntryError::Ipv6);
    }
    let (host, port) = match authority.split_once(':') {
        Some((host, port)) => (host, Some(port)),
        None => (authority, None),
    };
    if host_is_valid(host).not() {
        return Err(EntryError::Host);
    }
    if let Some(port) = port {
        if port_is_valid(port).not() {
            return Err(EntryError::Port);
        }
    }
    let slash_len = after_scheme.len() - authority.len();
    debug_assert!(slash_len <= 1);
    let origin = &entry[..entry.len() - slash_len];
    debug_assert!(origin.ends_with(authority));
    debug_assert!(
        origin
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b".-:/".contains(&byte)),
        "an origin holds no separator or quote"
    );
    Ok(origin)
}

/// Whether `host` is a DNS name or a dotted IPv4 address. Both are dot-separated labels, and
/// a policy's host allows no character beyond theirs.
fn host_is_valid(host: &str) -> bool {
    if host.is_empty() {
        return false;
    }
    if host.len() > HOST_LEN_MAX {
        return false;
    }
    host.split('.').all(label_is_valid)
}

fn label_is_valid(label: &str) -> bool {
    if label.is_empty() {
        return false;
    }
    if label.len() > LABEL_LEN_MAX {
        return false;
    }
    if label.starts_with('-') {
        return false;
    }
    if label.ends_with('-') {
        return false;
    }
    label
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
}

/// Digits only. Parsing alone would take a leading `+`, which no browser reads as a port.
fn port_is_valid(port: &str) -> bool {
    if port.bytes().all(|byte| byte.is_ascii_digit()).not() {
        return false;
    }
    port.parse::<u16>().is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entries(entries: &[&str]) -> Vec<String> {
        entries.iter().map(ToString::to_string).collect()
    }

    /// Goal: a daemon with no `frame_ancestors` sends exactly the policy it always has, with
    /// no composition involved. Method: the default, an empty list and a list of only
    /// invalid entries all yield the baseline constant, which itself denies framing.
    #[test]
    fn no_valid_entry_keeps_the_baseline() {
        assert_eq!(
            base::CONTENT_SECURITY_POLICY
                .matches(DOCUMENT_FRAME_ANCESTORS)
                .count(),
            1,
            "the baseline must deny framing, in the one place composition replaces"
        );
        assert_eq!(
            plugins::PLUGIN_CONTENT_SECURITY_POLICY
                .matches(PLUGIN_FRAME_ANCESTORS)
                .count(),
            1,
            "a plugin page is framed by the UI alone, in the one place composition widens"
        );
        let policies = [
            FramePolicy::default(),
            FramePolicy::from_config(&entries(&[])),
            FramePolicy::from_config(&entries(&["bad.value", "'self'", "*"])),
        ];
        for policy in policies {
            assert_eq!(policy.document_csp, base::CONTENT_SECURITY_POLICY);
            assert_eq!(policy.plugin_csp, plugins::PLUGIN_CONTENT_SECURITY_POLICY);
        }
    }

    /// Goal: configured origins replace `'none'` and nothing else in the policy changes.
    /// Method: compose with two valid entries around an invalid one, and compare against the
    /// baseline with only that directive swapped.
    #[test]
    fn valid_entries_replace_none() {
        let policy = FramePolicy::from_config(&entries(&[
            "https://localhost:9090",
            "bad.value",
            "http://192.168.2.1:1234",
        ]));
        let csp = policy.document_csp.to_str().unwrap();
        assert!(csp.contains("; frame-ancestors https://localhost:9090 http://192.168.2.1:1234; "));
        assert!(csp.contains("'none'; base-uri"), "object-src keeps its own");
        assert!(csp.contains(DOCUMENT_FRAME_ANCESTORS).not());
        assert!(csp.contains("bad.value").not());
        assert_eq!(csp.matches("frame-ancestors").count(), 1);
        assert_eq!(
            csp.replace(
                "frame-ancestors https://localhost:9090 http://192.168.2.1:1234",
                DOCUMENT_FRAME_ANCESTORS
            ),
            base::CONTENT_SECURITY_POLICY
        );
    }

    /// Goal: a plugin page stays framed by the UI and additionally admits what frames the
    /// UI, with nothing else in its policy changed. Method: compose and compare against
    /// the baseline with only that directive restored.
    #[test]
    fn valid_entries_join_self_on_plugin_pages() {
        let policy = FramePolicy::from_config(&entries(&[
            "https://localhost:9090",
            "http://192.168.2.1:1234",
        ]));
        let csp = policy.plugin_csp.to_str().unwrap();
        let widened = "frame-ancestors 'self' https://localhost:9090 http://192.168.2.1:1234";
        assert!(csp.contains(&format!("; {widened}; ")));
        assert_eq!(csp.matches("frame-ancestors").count(), 1);
        assert_eq!(
            csp.replace(widened, PLUGIN_FRAME_ANCESTORS),
            plugins::PLUGIN_CONTENT_SECURITY_POLICY
        );
    }

    /// Goal: every shape of exact origin is taken, and comes out as the policy writes it.
    /// Method: each scheme, host form and port, with and without the address bar's
    /// trailing slash, must yield the entry minus that slash.
    #[test]
    fn an_exact_origin_is_taken() {
        let hosts = [
            "192.168.0.1",
            "localhost",
            "example.com",
            "sub.example.com",
            "my-host.lan",
            "EXAMPLE.com",
        ];
        for scheme in ["https://", "http://"] {
            for host in hosts {
                for port in ["", ":0", ":80", ":9090", ":65535"] {
                    let origin = format!("{scheme}{host}{port}");
                    assert_eq!(ancestor_origin(&origin), Ok(origin.as_str()));
                    assert_eq!(ancestor_origin(&format!("{origin}/")), Ok(origin.as_str()));
                }
            }
        }
    }

    /// Entries that are not an exact origin, by the reason each is refused for.
    const REFUSED: [(EntryError, &[&str]); 5] = [
        (
            EntryError::Scheme,
            &[
                "",
                "*",
                "'self'",
                "'none'",
                "https:",
                "data:",
                "example.com",
                "example.com:9090",
                "//example.com",
                "HTTPS://example.com",
                "ftp://example.com",
                "wss://example.com",
                " https://example.com",
            ],
        ),
        (
            EntryError::Path,
            &[
                "https://example.com/test",
                "https://example.com:9090/a/b/",
                "https://example.com//",
                "https://example.com:9090/with space",
                "https://[::1]/test",
            ],
        ),
        (
            EntryError::Ipv6,
            &[
                "https://[::1]",
                "http://[2001:db8::1]:9090",
                "https://[2001:db8::1]/",
                "https://[::1]junk",
                "https://[invalid",
            ],
        ),
        (
            EntryError::Host,
            &[
                "https://",
                "https:///",
                "https://:9090",
                "https://*",
                "https://*.example.com",
                "https://example.com.",
                "https://-example.com",
                "https://example-.com",
                "https://example..com",
                "https://exa_mple.com",
                "https://user@example.com",
                "https://example.com?query",
                "https://example.com#fragment",
                "https://bücher.example",
            ],
        ),
        (
            EntryError::Port,
            &[
                "https://example.com:",
                "https://example.com:*",
                "https://example.com:abc",
                "https://example.com:+80",
                "https://example.com:-1",
                "https://example.com:65536",
                "https://example.com:9090x",
                "https://example.com:80:80",
                "https://example.com: 80",
            ],
        ),
    ];

    /// Goal: anything looser or other than an exact origin is refused, for the reason the
    /// log will give. Method: one table per reason, covering the keywords and wildcards a
    /// policy would otherwise honor.
    #[test]
    fn anything_but_an_exact_origin_is_refused() {
        for (reason, table) in REFUSED {
            for entry in table {
                assert_eq!(ancestor_origin(entry), Err(reason), "{entry:?}");
            }
        }
        let host_too_long = format!("https://{}", vec!["a".repeat(63); 4].join("."));
        assert_eq!(ancestor_origin(&host_too_long), Err(EntryError::Host));
        let label_too_long = format!("https://{}.com", "a".repeat(64));
        assert_eq!(ancestor_origin(&label_too_long), Err(EntryError::Host));
    }

    /// Goal: an entry cannot smuggle a second source or directive into the policy. Method:
    /// entries carrying each separator a policy has are refused whole, and the composed
    /// policies stay the baselines.
    #[test]
    fn an_entry_cannot_extend_the_policy() {
        let smuggling = entries(&[
            "https://example.com; script-src *",
            "https://example.com;script-src *",
            "https://example.com 'unsafe-inline'",
            "https://example.com https://evil.example",
            "https://example.com,https://evil.example",
            "https://example.com\nx-injected: 1",
            "https://example.com\r\n",
            "https://example.com\t*",
            "https://example.com'",
            "https://example.com\"",
        ]);
        for entry in &smuggling {
            assert!(ancestor_origin(entry).is_err(), "{entry:?}");
        }
        let policy = FramePolicy::from_config(&smuggling);
        assert_eq!(policy.document_csp, base::CONTENT_SECURITY_POLICY);
        assert_eq!(policy.plugin_csp, plugins::PLUGIN_CONTENT_SECURITY_POLICY);
    }

    /// Goal: the list is bounded, and the bound drops the tail rather than the head.
    /// Method: one entry more than the cap, each a distinct origin.
    #[test]
    fn entries_past_the_cap_are_dropped() {
        let configured: Vec<String> = (0..=FRAME_ANCESTORS_COUNT_MAX)
            .map(|index| format!("https://host{index}.example.com"))
            .collect();
        let origins = valid_origins(&configured);
        assert_eq!(origins.len(), FRAME_ANCESTORS_COUNT_MAX);
        assert_eq!(origins.first(), Some(&"https://host0.example.com"));
        assert_eq!(origins.last(), Some(&"https://host15.example.com"));

        // Invalid entries do not count toward the cap.
        let mut padded = entries(&["bad.value"; FRAME_ANCESTORS_COUNT_MAX]);
        padded.push("https://cockpit.example.com:9090".to_string());
        assert_eq!(valid_origins(&padded), ["https://cockpit.example.com:9090"]);
    }

    /// Goal: the log reports a dropped origin only when one is dropped, and an invalid entry
    /// past a full list is still reported. Method: fill the list to the cap, then follow
    /// it with an invalid entry, a valid one, and an invalid one past the truncation.
    #[test]
    fn only_a_valid_origin_past_the_cap_truncates() {
        let mut configured: Vec<String> = (0..FRAME_ANCESTORS_COUNT_MAX)
            .map(|index| format!("https://host{index}.example.com"))
            .collect();
        configured.extend(entries(&[
            "bad.value",
            "https://late.example.com",
            "https://late.example.com/path",
        ]));
        let full = screen(&configured[..FRAME_ANCESTORS_COUNT_MAX]);
        assert_eq!(full.origins.len(), FRAME_ANCESTORS_COUNT_MAX);
        assert!(full.refused.is_empty());
        assert!(full.is_truncated.not());

        let followed_by_invalid = screen(&configured[..=FRAME_ANCESTORS_COUNT_MAX]);
        assert_eq!(followed_by_invalid.origins, full.origins);
        assert_eq!(
            followed_by_invalid.refused,
            [("bad.value", EntryError::Scheme)]
        );
        assert!(followed_by_invalid.is_truncated.not());

        let followed_by_valid = screen(&configured);
        assert_eq!(followed_by_valid.origins, full.origins);
        assert_eq!(
            followed_by_valid.refused,
            [("bad.value", EntryError::Scheme)],
            "nothing is screened once the list is truncated"
        );
        assert!(followed_by_valid.is_truncated);
    }

    /// Goal: the trailing slash of an entry never reaches the policy, where it would read
    /// as a path. Method: compose from an entry written with one.
    #[test]
    fn a_trailing_slash_is_not_written() {
        let policy = FramePolicy::from_config(&entries(&["https://cockpit.example.com:9090/"]));
        let csp = policy.document_csp.to_str().unwrap();
        assert!(csp.contains("; frame-ancestors https://cockpit.example.com:9090; "));
    }
}
