// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Who may embed the UI in a frame, and the `Content-Security-Policy` value that says so.

use crate::api::base;
use axum::http::HeaderValue;
use log::error;
use std::net::{Ipv4Addr, Ipv6Addr};
use std::ops::Not;

/// What the UI document's policy says with no `frame_ancestors` configured.
const DOCUMENT_FRAME_ANCESTORS: &str = "frame-ancestors 'none'";

/// The header values that depend on `frame_ancestors`. Composed once at startup, so no
/// request pays for it.
#[derive(Debug, Clone)]
pub struct FramePolicy {
    /// The `Content-Security-Policy` of the UI document.
    pub document_csp: HeaderValue,
}

impl Default for FramePolicy {
    /// Nothing may frame the UI.
    fn default() -> Self {
        Self {
            document_csp: HeaderValue::from_static(base::CONTENT_SECURITY_POLICY),
        }
    }
}

impl FramePolicy {
    /// Widens the default policy by the configured origins. An invalid entry is skipped:
    /// that origin then cannot frame the UI, which is the safe direction to fail.
    pub fn from_config(entries: &[String]) -> Self {
        let origins: Vec<&str> = entries
            .iter()
            .map(String::as_str)
            .filter(|entry| csp_frame_ancestor_is_valid(entry))
            .collect();
        if origins.is_empty() {
            return Self::default();
        }
        let sources = origins.join(" ");
        let document = base::CONTENT_SECURITY_POLICY.replace(
            DOCUMENT_FRAME_ANCESTORS,
            &format!("frame-ancestors {sources}"),
        );
        debug_assert!(document.contains(&sources));
        debug_assert!(document.contains(DOCUMENT_FRAME_ANCESTORS).not());
        let Ok(document_csp) = HeaderValue::try_from(document) else {
            // Validated origins are visible ASCII, so this is a bug. Keep the UI unframeable.
            error!("The frame_ancestors origins do not form a header value: {sources:?}");
            return Self::default();
        };
        Self { document_csp }
    }
}

fn hostname_is_valid(hostname: &str) -> bool {
    if hostname.is_empty() || hostname.len() > 253 {
        return false;
    }
    hostname.split('.').all(|label| {
        if label.is_empty() || label.len() > 63 {
            return false;
        }
        if label.starts_with('-') {
            return false;
        }
        if label.ends_with('-') {
            return false;
        }
        label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
    })
}

struct CspFrameAncestorParts<'a> {
    /// "http://", "https://", etc.
    scheme: &'a str,
    host: &'a str,
    /// port, not including ':'
    port: Option<&'a str>,
    /// path after host (and port), not including leading '/'
    path: Option<&'a str>,
}

fn csp_frame_ancestor_parse(ancestor: &str) -> Option<CspFrameAncestorParts<'_>> {
    let (scheme, after_scheme) = ancestor.split_once("://")?;
    let (origin, path) = after_scheme
        .split_once('/')
        .map_or((after_scheme, None), |(origin, path)| (origin, Some(path)));
    let (host, port) = (if origin.starts_with('[') {
        origin.find(']').and_then(|host_end_index| {
            let host = &origin[0..=host_end_index];
            let after_host = &origin[host_end_index + 1..];
            match after_host {
                s if s.starts_with(':') => Some((host, after_host.strip_prefix(':'))),
                "" => Some((host, None)),
                _ => None,
            }
        })
    } else {
        Some(
            origin
                .split_once(':')
                .map_or((origin, None), |(host, port)| (host, Some(port))),
        )
    })?;
    Some(CspFrameAncestorParts {
        scheme,
        host,
        port,
        path,
    })
}

fn csp_frame_ancestor_is_valid(ancestor: &str) -> bool {
    let Some(parsed) = csp_frame_ancestor_parse(ancestor) else {
        return false;
    };
    // validate scheme
    if ["http", "https"].contains(&parsed.scheme).not() {
        return false;
    }
    // validate host
    let host_valid = parsed
        .host
        .strip_prefix('[')
        .and_then(|h| h.strip_suffix(']'))
        .map_or_else(
            || parsed.host.parse::<Ipv4Addr>().is_ok() || hostname_is_valid(parsed.host),
            |ipv6addr| ipv6addr.parse::<Ipv6Addr>().is_ok(),
        );
    if host_valid.not() {
        return false;
    }
    // validate port
    if let Some(port) = parsed.port {
        if port.parse::<u16>().is_err() {
            return false;
        }
    }
    // validate path
    if let Some(path) = parsed.path {
        // only allow no path or bare /
        if path.is_empty().not() {
            return false;
        }
    }
    true
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
        for config in [entries(&[]), entries(&["bad.value", "'self'", "*"])] {
            assert_eq!(
                FramePolicy::from_config(&config).document_csp,
                base::CONTENT_SECURITY_POLICY
            );
        }
        assert_eq!(
            FramePolicy::default().document_csp,
            base::CONTENT_SECURITY_POLICY
        );
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

    #[test]
    fn test_csp_frame_ancestor_is_valid() {
        let valid_schemes = vec!["https://", "http://"];
        let valid_hosts = vec![
            "192.168.0.1",
            "[2001:db8::1]",
            "example.com",
            "sub.example.com",
        ];
        let valid_ports = vec!["", ":9090", ":65535"];
        let valid_paths = vec!["", "/"];

        for scheme in &valid_schemes {
            for host in &valid_hosts {
                for port in &valid_ports {
                    for path in &valid_paths {
                        assert!(csp_frame_ancestor_is_valid(&format!(
                            "{scheme}{host}{port}{path}"
                        )));
                    }
                }
            }
        }

        let invalid_hosts = [
            "",
            "[2001:db8::1",
            "[invalid]",
            "example.com.",
            "-example.com",
            "example-.com",
            "example..com",
        ];
        for host in invalid_hosts {
            assert!(!csp_frame_ancestor_is_valid(&format!(
                "https://{host}:9090/test"
            )));
        }

        let invalid_ports = [":", ":abc", ":65536", ":-1", ":9090x"];
        for port in invalid_ports {
            assert!(!csp_frame_ancestor_is_valid(&format!(
                "https://example.com{port}/test"
            )));
        }

        let invalid_paths = [
            "/test",
            "/a/b/123/test_path/~/./",
            "/ABC",
            "//",
            "/test//",
            "/test//sdf",
            "/@#$%^",
            "/with space",
        ];
        for path in invalid_paths {
            assert!(
                !csp_frame_ancestor_is_valid(&format!("https://example.com:9090{path}")),
                "failed on path {path}"
            );
        }

        assert!(!csp_frame_ancestor_is_valid("https://[::1]junk"));
    }
}
