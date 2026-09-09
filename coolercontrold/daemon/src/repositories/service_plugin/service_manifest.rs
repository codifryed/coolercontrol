// SPDX-FileCopyrightText: 2025 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use crate::paths;
use anyhow::{anyhow, Context, Result};
use std::net::Ipv6Addr;
use std::ops::Not;
use std::path::PathBuf;
use strum::{Display, EnumString};
use toml_edit::DocumentMut;

#[derive(Debug, Clone)]
pub struct ServiceManifest {
    pub id: String,                // required for all service plugins
    pub service_type: ServiceType, // required for all service plugins
    pub description: Option<String>,
    pub version: Option<String>,
    pub url: Option<String>,
    pub executable: Option<PathBuf>, // required IF user wants to have the service managed
    pub args: Vec<String>,           // if needed (set log level, etc.) "--arg1 --arg2"
    pub envs: Vec<EnvVar>,           // if needed (set log level, etc.) "ENV1=value1 ENV2=value2"
    pub address: ConnectionType,     // required for all device service plugins
    pub privileged: bool,            // for device service plugins (false by default)
    pub proxy: Option<ProxyConfig>,  // for plugins that expose a local HTTP API
    pub path: PathBuf,               // This plugin's folder path
}

impl ServiceManifest {
    pub fn from_document(document: &DocumentMut, path: PathBuf) -> Result<Self> {
        let id = Self::get_optional_string(document, "id")
            .with_context(|| "Service manifest id should be present")?;
        Self::validate_id(&id)?;
        let service_type_str = Self::get_optional_string(document, "type")
            .with_context(|| "Service manifest service type should be present")?
            .to_lowercase();
        let service_type = match service_type_str.as_str() {
            "device" => ServiceType::Device,
            "integration" => ServiceType::Integration,
            _ => return Err(anyhow!("Invalid service type")),
        };
        // These three reach only the API and the UI: the generated unit's `Description=`
        // is built from the already strictly validated `id`. They were being run through
        // the unit-field allowlist anyway, which silently deleted ordinary punctuation.
        let description = Self::get_optional_string(document, "description")
            .map(|d| validate_field("description", &d))
            .transpose()?;
        let version = Self::get_optional_string(document, "version")
            .map(|v| validate_field("version", &v))
            .transpose()?;
        let url = Self::get_optional_string(document, "url")
            .map(|u| validate_field("url", &u))
            .transpose()?;
        let executable = Self::get_optional_string(document, "executable")
            .map(|exe| -> Result<PathBuf> {
                let exe = validate_field("executable", &exe)?;
                let mut exe_path = PathBuf::from(exe);
                if exe_path.is_relative() {
                    exe_path = paths::plugins_dir().join(&id).join(exe_path);
                }
                Ok(exe_path)
            })
            .transpose()?;
        let args = Self::get_args(document)?;
        let envs = Self::get_envs(document)?;
        let address = Self::get_address(document, &id, &service_type)?;
        // A mistyped `privileged` used to fall back to `false` without a word. It fails
        // safe, but a plugin that needs root then starts unprivileged and misbehaves for
        // a reason nothing points at.
        let privileged = match document.get("privileged") {
            None => false,
            Some(item) => item
                .as_bool()
                .context("Service manifest privileged should be a boolean")?,
        };
        let proxy = Self::get_proxy(document)?;
        Ok(Self {
            id,
            service_type,
            description,
            version,
            url,
            executable,
            args,
            envs,
            address,
            privileged,
            proxy,
            path,
        })
    }

    fn validate_id(id: &str) -> Result<()> {
        if id.is_empty() || id.len() > 64 {
            return Err(anyhow!("Service manifest id must be 1-64 characters"));
        }
        if !id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return Err(anyhow!(
                "Service manifest id contains invalid characters \
                 (only alphanumeric, hyphens, underscores allowed)"
            ));
        }
        Ok(())
    }

    /// The proxy configuration, or `None` when the plugin has no proxy or disabled it.
    ///
    /// Every problem here used to yield `None`, which silently disabled the proxy: a
    /// privileged port, a missing port, or a `proxy = { ... }` inline table, since the
    /// old code matched only a `[proxy]` table and an inline one is a value.
    fn get_proxy(document: &DocumentMut) -> Result<Option<ProxyConfig>> {
        let Some(item) = document.get("proxy") else {
            return Ok(None);
        };
        let table = item
            .as_table_like()
            .context("Service manifest proxy should be a table")?;
        let enabled = match table.get("enabled") {
            None => false,
            Some(value) => value
                .as_bool()
                .context("Service manifest proxy.enabled should be a boolean")?,
        };
        if enabled.not() {
            return Ok(None);
        }
        let port_value = table
            .get("port")
            .context("Service manifest proxy.port is required when the proxy is enabled")?
            .as_integer()
            .context("Service manifest proxy.port should be an integer")?;
        // Below 1024 needs privileges the plugin user does not have, so a proxy there
        // could never bind.
        let port = u16::try_from(port_value)
            .ok()
            .filter(|&port| port >= PROXY_PORT_MIN)
            .with_context(|| {
                format!(
                    "Service manifest proxy.port {port_value} should be between \
                     {PROXY_PORT_MIN} and {}",
                    u16::MAX
                )
            })?;
        Ok(Some(ProxyConfig { port }))
    }

    /// Where a device service is reached, defaulting to a socket named after the plugin.
    ///
    /// Only a device service connects anywhere, so an integration service is left with no
    /// address even when its manifest names one.
    fn get_address(
        document: &DocumentMut,
        id: &str,
        service_type: &ServiceType,
    ) -> Result<ConnectionType> {
        if *service_type != ServiceType::Device {
            return Ok(ConnectionType::None);
        }
        let address = Self::get_optional_string(document, "address")
            .map(|address| validate_field("address", &address))
            .transpose()?
            .unwrap_or_else(|| format!("/tmp/{id}.sock"));
        let path = PathBuf::from(&address);
        if path.is_absolute() {
            return Ok(ConnectionType::Uds(path));
        }
        validate_tcp_address(&address)?;
        Ok(ConnectionType::Tcp(address))
    }

    /// `args` as either a whitespace-separated string or an array of arguments.
    ///
    /// The array form is the only way to pass an argument that contains whitespace: the
    /// string form has to split somewhere, and it splits on whitespace. It also cannot
    /// produce an empty argument, which the old parser did whenever an argument consisted
    /// entirely of characters it stripped.
    ///
    /// Values are taken verbatim. Making them safe belongs to whichever init system file
    /// they are written into, and only that layer knows what needs escaping there.
    fn get_args(document: &DocumentMut) -> Result<Vec<String>> {
        let Some(item) = document.get("args") else {
            return Ok(Vec::new());
        };
        if let Some(array) = item.as_array() {
            let mut args = Vec::with_capacity(array.len());
            for value in array {
                let text = value
                    .as_str()
                    .context("Service manifest args array should hold only strings")?;
                args.push(validate_field("args", text)?);
            }
            return Ok(args);
        }
        let text = item
            .as_str()
            .context("Service manifest args should be a string or an array of strings")?;
        // Validated whole, before splitting: `split_whitespace` would otherwise eat a
        // newline and turn one argument into two without a word about it.
        validate_field("args", text)?;
        Ok(text.split_whitespace().map(str::to_string).collect())
    }

    /// `envs` as either a whitespace-separated `KEY=value` string or a table.
    ///
    /// The table form is the only way to give a value containing whitespace, and the only
    /// one in which a malformed entry cannot be written. In the string form a token
    /// without an `=` used to be dropped without a word, which turned a typo into a
    /// missing variable the author had no way to notice.
    fn get_envs(document: &DocumentMut) -> Result<Vec<EnvVar>> {
        let Some(item) = document.get("envs") else {
            return Ok(Vec::new());
        };
        if let Some(table) = item.as_table_like() {
            let mut envs = Vec::with_capacity(table.len());
            for (name, value) in table.iter() {
                let text = value
                    .as_str()
                    .with_context(|| format!("Service manifest env '{name}' should be a string"))?;
                envs.push(EnvVar::new(name, &validate_field("envs", text)?)?);
            }
            return Ok(envs);
        }
        let text = item
            .as_str()
            .context("Service manifest envs should be a string or a table")?;
        // Whole-string first, for the same reason as `get_args`.
        validate_field("envs", text)?;
        let mut envs = Vec::new();
        for entry in text.split_whitespace() {
            let (name, value) = entry.split_once('=').with_context(|| {
                format!("Service manifest env entry '{entry}' is missing its '='")
            })?;
            envs.push(EnvVar::new(name, value)?);
        }
        Ok(envs)
    }

    fn get_optional_string(document: &DocumentMut, field_name: &str) -> Option<String> {
        document
            .get(field_name)
            .and_then(|item| item.as_str())
            .map(|d| d.trim().to_string())
            .filter(|d| d.is_empty().not())
    }

    pub fn is_managed(&self) -> bool {
        self.executable.is_some()
    }
}

/// Configuration for a plugin's local HTTP proxy.
#[derive(Debug, Clone)]
pub struct ProxyConfig {
    /// The loopback port the plugin's HTTP server listens on.
    pub port: u16,
}

#[derive(Debug, PartialEq, Clone, EnumString, Display)]
pub enum ServiceType {
    Device,
    Integration,
}

#[derive(Debug, PartialEq, Clone, EnumString, Display)]
pub enum ConnectionType {
    None,
    Uds(PathBuf),
    Tcp(String),
}

/// Lowest port a plugin's proxy may use, since anything below needs privileges the
/// plugin user does not have.
const PROXY_PORT_MIN: u16 = 1024;

/// Checks that a non-path address is something a URI can be built from.
///
/// Anything at all used to be accepted, so `address = "not a hostname"` parsed happily
/// and then failed at connect time with an opaque URI error that named neither the
/// manifest nor the field.
///
/// Deliberately not a full hostname grammar: this rejects what cannot possibly work, and
/// leaves the resolver to judge whether a well-formed host actually exists.
fn validate_tcp_address(address: &str) -> Result<()> {
    let malformed = || {
        anyhow!(
            "Service manifest address '{address}' should be 'host:port', or an absolute \
             path for a Unix socket"
        )
    };
    let (host, port) = split_host_port(address).ok_or_else(malformed)?;
    if host.is_empty() {
        return Err(malformed());
    }
    if host.chars().any(char::is_whitespace) {
        return Err(anyhow!(
            "Service manifest address host '{host}' contains whitespace"
        ));
    }
    // A colon left in the host means an IPv6 literal that was not bracketed. Accept it
    // when it really is one, since it is unambiguous, and reject the leftovers.
    if host.contains(':') && host.parse::<Ipv6Addr>().is_err() {
        return Err(malformed());
    }
    let port: u16 = port.parse().map_err(|_| {
        anyhow!(
            "Service manifest address '{address}' should end in a port from 1 to {}",
            u16::MAX
        )
    })?;
    if port == 0 {
        return Err(anyhow!(
            "Service manifest address '{address}' cannot use port 0"
        ));
    }
    Ok(())
}

/// Splits `host:port`, tolerating a bracketed IPv6 literal.
fn split_host_port(address: &str) -> Option<(&str, &str)> {
    if let Some(rest) = address.strip_prefix('[') {
        let (host, tail) = rest.split_once(']')?;
        return tail.strip_prefix(':').map(|port| (host, port));
    }
    address.rsplit_once(':')
}

/// Rejects the one thing no manifest field may contain.
///
/// A control character is the only universal hazard: a newline injects a directive into a
/// generated unit file or a statement into an `OpenRC` script, and a NUL cannot survive a
/// syscall. Every other character is the generated file's problem to escape, which is why
/// the escaping now lives in `service_management` next to the syntax it has to satisfy.
///
/// This replaces an allowlist that deleted anything it did not recognise. That silently
/// corrupted ordinary values: `--rate 50%` reached the plugin as `--rate 50`, and
/// `--listen=[::1]:8080` lost its brackets. Rejecting is loud, and a control character in
/// a manifest is always an authoring mistake rather than something to salvage.
fn validate_field(field_name: &str, value: &str) -> Result<String> {
    if let Some(found) = value.chars().find(|c| c.is_control()) {
        return Err(anyhow!(
            "Service manifest {field_name} contains the control character '{}'",
            found.escape_debug()
        ));
    }
    Ok(value.to_string())
}

/// One environment variable handed to a plugin's service.
///
/// A named type rather than a `(String, String)`, so a name can only be validated once,
/// here, and no consumer downstream can pair the halves the wrong way round.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvVar {
    pub name: String,
    pub value: String,
}

impl EnvVar {
    /// Rejects a name outside `systemd.exec(5)`: ASCII letters, digits and underscores,
    /// non-empty, and not starting with a digit.
    ///
    /// Rejecting here beats writing a unit file that systemd then refuses to load, which
    /// would surface as the whole plugin failing to start for no stated reason.
    pub fn new(name: &str, value: &str) -> Result<Self> {
        if name.is_empty() {
            return Err(anyhow!("Service manifest env name must not be empty"));
        }
        if name.starts_with(|c: char| c.is_ascii_digit()) {
            return Err(anyhow!(
                "Service manifest env name '{name}' must not start with a digit"
            ));
        }
        if name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
            .not()
        {
            return Err(anyhow!(
                "Service manifest env name '{name}' may hold only ASCII letters, digits \
                 and underscores"
            ));
        }
        Ok(Self {
            name: name.to_string(),
            value: value.to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_manifest_toml(overrides: &[(&str, &str)]) -> String {
        let mut fields: Vec<(String, String)> = vec![
            ("id".into(), "\"test-plugin\"".into()),
            ("type".into(), "\"device\"".into()),
        ];
        for (key, value) in overrides {
            if let Some(pos) = fields.iter().position(|(k, _)| k == *key) {
                fields[pos].1 = value.to_string();
            } else {
                fields.push((key.to_string(), value.to_string()));
            }
        }
        fields
            .iter()
            .map(|(k, v)| format!("{k} = {v}"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn env(name: &str, value: &str) -> EnvVar {
        EnvVar::new(name, value).unwrap()
    }

    fn parse_manifest(toml_str: &str) -> Result<ServiceManifest> {
        let doc: DocumentMut = toml_str.parse()?;
        ServiceManifest::from_document(&doc, PathBuf::from("/tmp/test"))
    }

    #[test]
    fn test_valid_manifest_parses() {
        let toml = make_manifest_toml(&[
            ("description", "\"A test plugin\""),
            ("version", "\"1.0.0\""),
            ("executable", "\"/usr/bin/test\""),
            ("args", "\"--verbose --port=8080\""),
            ("envs", "\"LOG_LEVEL=debug PORT=3000\""),
        ]);
        let manifest = parse_manifest(&toml).unwrap();
        assert_eq!(manifest.id, "test-plugin");
        assert_eq!(manifest.description, Some("A test plugin".into()));
        assert_eq!(manifest.version, Some("1.0.0".into()));
        assert_eq!(manifest.args, vec!["--verbose", "--port=8080"]);
        assert_eq!(
            manifest.envs,
            vec![env("LOG_LEVEL", "debug"), env("PORT", "3000")]
        );
    }

    #[test]
    fn test_id_with_newlines_rejected() {
        let toml = make_manifest_toml(&[("id", "\"test\\nplugin\"")]);
        let result = parse_manifest(&toml);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("invalid characters"));
    }

    #[test]
    fn test_id_with_shell_metacharacters_rejected() {
        let toml = make_manifest_toml(&[("id", "\"test;rm -rf /\"")]);
        let result = parse_manifest(&toml);
        assert!(result.is_err());
    }

    #[test]
    fn test_id_with_path_traversal_rejected() {
        let toml = make_manifest_toml(&[("id", "\"../etc/passwd\"")]);
        let result = parse_manifest(&toml);
        assert!(result.is_err());
    }

    #[test]
    fn test_id_exceeding_64_chars_rejected() {
        let long_id = "a".repeat(65);
        let toml = make_manifest_toml(&[("id", &format!("\"{long_id}\""))]);
        let result = parse_manifest(&toml);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("1-64 characters"));
    }

    #[test]
    fn test_id_exactly_64_chars_accepted() {
        let id = "a".repeat(64);
        let toml = make_manifest_toml(&[("id", &format!("\"{id}\""))]);
        let manifest = parse_manifest(&toml).unwrap();
        assert_eq!(manifest.id, id);
    }

    /// Goal: an address that cannot become a URI is refused where the message can name
    /// the manifest. It used to parse happily and fail much later inside the gRPC client,
    /// with nothing pointing back at the field that was wrong.
    #[test]
    fn test_malformed_tcp_address_rejected() {
        for bad in [
            "not a hostname",
            "192.168.1.100",
            "192.168.1.100:",
            "192.168.1.100:notaport",
            "192.168.1.100:0",
            "192.168.1.100:99999",
            ":11987",
            "my host:11987",
            "::1",
            "[::1]11987",
        ] {
            let toml = make_manifest_toml(&[("address", &format!("\"{bad}\""))]);
            assert!(parse_manifest(&toml).is_err(), "{bad} should be rejected");
        }
    }

    /// Goal: every shape that must keep working, so the new check cannot turn a valid
    /// address away. A bracketed IPv6 literal and an unbracketed one are both accepted,
    /// since an unbracketed one is unambiguous when it really parses as an address.
    #[test]
    fn test_valid_addresses_accepted() {
        for good in [
            "192.168.1.100:11987",
            "localhost:11987",
            "my-server.example.com:11987",
            "[::1]:11987",
            "[fe80::1]:11987",
            "::1:11987",
        ] {
            let toml = make_manifest_toml(&[("address", &format!("\"{good}\""))]);
            let manifest = parse_manifest(&toml)
                .unwrap_or_else(|err| panic!("{good} should parse, got {err}"));
            assert!(
                matches!(manifest.address, ConnectionType::Tcp(_)),
                "{good} should be a TCP address, got {:?}",
                manifest.address
            );
        }
    }

    /// Goal: an absolute path is a Unix socket and is not held to the host:port rule.
    #[test]
    fn test_absolute_address_is_a_unix_socket() {
        let toml = make_manifest_toml(&[("address", "\"/run/my plugin.sock\"")]);
        let manifest = parse_manifest(&toml).unwrap();
        assert_eq!(
            manifest.address,
            ConnectionType::Uds(PathBuf::from("/run/my plugin.sock"))
        );
    }

    /// Goal: an integration service has nothing to connect to, so it gets no address even
    /// when its manifest names one. This also pins the default a device service falls
    /// back to, which the old code reached through a branch that could never run.
    #[test]
    fn test_address_defaults_only_for_a_device_service() {
        let device = parse_manifest(&make_manifest_toml(&[])).unwrap();
        assert_eq!(
            device.address,
            ConnectionType::Uds(PathBuf::from("/tmp/test-plugin.sock"))
        );

        let integration =
            parse_manifest(&make_manifest_toml(&[("type", "\"integration\"")])).unwrap();
        assert_eq!(integration.address, ConnectionType::None);

        let addressed_integration = parse_manifest(&make_manifest_toml(&[
            ("type", "\"integration\""),
            ("address", "\"192.168.1.100:11987\""),
        ]))
        .unwrap();
        assert_eq!(addressed_integration.address, ConnectionType::None);
    }

    /// Goal: a mistyped `privileged` says so. Falling back to `false` is safe but silent,
    /// so a plugin that needs root starts unprivileged and fails for a reason nothing
    /// points at.
    #[test]
    fn test_non_boolean_privileged_rejected() {
        for bad in ["\"true\"", "1", "\"yes\""] {
            let toml = make_manifest_toml(&[("privileged", bad)]);
            assert!(parse_manifest(&toml).is_err(), "{bad} should be rejected");
        }
        assert!(
            parse_manifest(&make_manifest_toml(&[("privileged", "true")]))
                .unwrap()
                .privileged
        );
    }

    /// Goal: a proxy the daemon cannot honour is an error, not a silent omission. Every
    /// one of these used to disable the proxy without a word.
    #[test]
    fn test_unusable_proxy_rejected() {
        for bad in [
            "{ enabled = true, port = 80 }",
            "{ enabled = true, port = 0 }",
            "{ enabled = true, port = 70000 }",
            "{ enabled = true }",
            "{ enabled = \"yes\", port = 8080 }",
            "{ enabled = true, port = \"8080\" }",
            "\"true\"",
        ] {
            let toml = make_manifest_toml(&[("proxy", bad)]);
            assert!(parse_manifest(&toml).is_err(), "{bad} should be rejected");
        }
    }

    /// Goal: an inline `proxy` table works. The old code matched only a `[proxy]` table,
    /// and an inline table is a value, so `proxy = { enabled = true, port = 8080 }` was
    /// silently ignored and the proxy never came up.
    #[test]
    fn test_inline_proxy_table_is_honoured() {
        let toml = make_manifest_toml(&[("proxy", "{ enabled = true, port = 8080 }")]);
        let manifest = parse_manifest(&toml).unwrap();
        assert_eq!(manifest.proxy.map(|proxy| proxy.port), Some(8080));
    }

    /// Goal: a disabled proxy needs no port and is not an error, which is the normal way
    /// to turn one off.
    #[test]
    fn test_disabled_proxy_needs_no_port() {
        let toml = make_manifest_toml(&[("proxy", "{ enabled = false }")]);
        assert!(parse_manifest(&toml).unwrap().proxy.is_none());
    }

    /// Goal: a control character is refused rather than deleted. A newline in `args`
    /// would inject a directive into the generated unit, and the old parser dropped it
    /// silently, which meant the injected text became part of the argument instead.
    #[test]
    fn test_args_with_a_control_character_rejected() {
        let toml = make_manifest_toml(&[("args", "\"--verbose\\nExecStart=/bin/malicious\"")]);
        let error = parse_manifest(&toml).unwrap_err().to_string();
        assert!(error.contains("args"), "{error}");
        assert!(error.contains("control character"), "{error}");
    }

    /// Goal: the reported bug. Every one of these was silently corrupted by the old
    /// allowlist, most visibly by losing the final character of an argument.
    #[test]
    fn test_args_reach_the_plugin_verbatim() {
        for (written, expected) in [
            ("--rate 50%", vec!["--rate", "50%"]),
            ("--pct 100%", vec!["--pct", "100%"]),
            ("--tilde x~", vec!["--tilde", "x~"]),
            ("--listen=[::1]:8080", vec!["--listen=[::1]:8080"]),
            (
                "--config ~/.config/foo.toml",
                vec!["--config", "~/.config/foo.toml"],
            ),
            ("--filter *", vec!["--filter", "*"]),
            ("--sep ;", vec!["--sep", ";"]),
            ("--pass p@ss!word", vec!["--pass", "p@ss!word"]),
            ("--path C:\\\\tmp", vec!["--path", "C:\\tmp"]),
            (
                "--url https://ex.com/a?b=1&c=2",
                vec!["--url", "https://ex.com/a?b=1&c=2"],
            ),
        ] {
            let toml = make_manifest_toml(&[("args", &format!("\"{written}\""))]);
            let manifest = parse_manifest(&toml)
                .unwrap_or_else(|err| panic!("{written} should parse, got {err}"));
            assert_eq!(manifest.args, expected, "for {written}");
        }
    }

    /// Goal: the string form cannot express an argument containing whitespace, because it
    /// has to split somewhere. The array form is the answer, and it used to be ignored
    /// outright: a non-string `args` fell through and produced no arguments at all.
    #[test]
    fn test_args_array_keeps_whitespace_in_one_argument() {
        let toml = make_manifest_toml(&[("args", "[\"--name\", \"My Device\", \"-v\"]")]);
        let manifest = parse_manifest(&toml).unwrap();
        assert_eq!(manifest.args, vec!["--name", "My Device", "-v"]);
    }

    /// Goal: the string form can no longer produce an empty argument, which the old
    /// parser did whenever a whole argument was stripped away. An empty argv entry is
    /// read as a positional argument by most parsers, so it is worse than none.
    #[test]
    fn test_no_argument_is_ever_empty() {
        let toml = make_manifest_toml(&[("args", "\"--filter * --sep ;\"")]);
        let manifest = parse_manifest(&toml).unwrap();
        assert!(manifest.args.iter().all(|arg| arg.is_empty().not()));
    }

    /// Goal: a mistyped `args` must say so rather than silently yielding nothing.
    #[test]
    fn test_args_of_the_wrong_type_rejected() {
        let numeric = make_manifest_toml(&[("args", "42")]);
        assert!(parse_manifest(&numeric).is_err());

        let mixed_array = make_manifest_toml(&[("args", "[\"--a\", 42]")]);
        assert!(parse_manifest(&mixed_array).is_err());
    }

    /// Goal: env values are verbatim too. `%Y-%m-%d` used to arrive as `Y-m-d`, which
    /// silently changed the meaning of a log-format variable.
    #[test]
    fn test_env_values_reach_the_plugin_verbatim() {
        let toml = make_manifest_toml(&[("envs", "\"FMT=%Y-%m-%d PATH=/a:/b\"")]);
        let manifest = parse_manifest(&toml).unwrap();
        assert_eq!(
            manifest.envs,
            vec![env("FMT", "%Y-%m-%d"), env("PATH", "/a:/b")]
        );
    }

    /// Goal: the table form, the only way to give a value containing whitespace. The
    /// string form silently kept `hello` and threw `world` away.
    #[test]
    fn test_env_table_keeps_whitespace_in_a_value() {
        let toml = make_manifest_toml(&[("envs", "{ GREETING = \"hello world\", N = \"1\" }")]);
        let manifest = parse_manifest(&toml).unwrap();
        assert_eq!(
            manifest.envs,
            vec![env("GREETING", "hello world"), env("N", "1")]
        );
    }

    /// Goal: a malformed entry is an error, not a silent omission. A token with no `=`
    /// used to be dropped without a word, so a typo became a missing variable that the
    /// author had no way to notice.
    #[test]
    fn test_env_entry_without_an_equals_rejected() {
        let toml = make_manifest_toml(&[("envs", "\"NOEQUALS OTHER=1\"")]);
        let error = parse_manifest(&toml).unwrap_err().to_string();
        assert!(error.contains("NOEQUALS"), "{error}");
        assert!(error.contains("missing"), "{error}");
    }

    /// Goal: an env name systemd would refuse is caught here, where the message can name
    /// it, rather than at unit load where the whole plugin just fails to start.
    #[test]
    fn test_env_names_follow_the_systemd_rules() {
        for bad in ["1FIRST=x", "WITH-DASH=x", "=x", "WITH.DOT=x"] {
            let toml = make_manifest_toml(&[("envs", &format!("\"{bad}\""))]);
            assert!(parse_manifest(&toml).is_err(), "{bad} should be rejected");
        }
        let good = make_manifest_toml(&[("envs", "\"_UNDER1=x\"")]);
        assert_eq!(
            parse_manifest(&good).unwrap().envs,
            vec![env("_UNDER1", "x")]
        );
    }

    /// Goal: a control character is still refused in `executable`, which the old parser
    /// checked by hand and now goes through the shared validator.
    #[test]
    fn test_executable_with_control_chars_rejected() {
        let toml = make_manifest_toml(&[("executable", "\"/usr/bin/test\\n--malicious\"")]);
        let error = parse_manifest(&toml).unwrap_err().to_string();
        assert!(error.contains("executable"), "{error}");
        assert!(error.contains("control character"), "{error}");
    }

    /// Goal: `description`, `version` and `url` reach only the API and the UI, never a
    /// unit file, whose `Description=` is built from the validated `id`. They were being
    /// run through the unit allowlist anyway, which deleted ordinary punctuation from
    /// human-written text.
    #[test]
    fn test_ui_fields_keep_their_punctuation() {
        let toml = make_manifest_toml(&[
            ("description", "\"Bob's plugin (v2) 100% better!\""),
            ("version", "\"1.0~beta2\""),
            ("url", "\"https://ex.com/a?b=1&c=2#frag\""),
        ]);
        let manifest = parse_manifest(&toml).unwrap();
        assert_eq!(
            manifest.description.as_deref(),
            Some("Bob's plugin (v2) 100% better!")
        );
        assert_eq!(manifest.version.as_deref(), Some("1.0~beta2"));
        assert_eq!(
            manifest.url.as_deref(),
            Some("https://ex.com/a?b=1&c=2#frag")
        );
    }

    /// Goal: a newline in a UI field is still refused. It cannot reach a unit file, but
    /// it has no legitimate use and the old behaviour, collapsing it into the text, left
    /// the injected directive sitting in the description.
    #[test]
    fn test_description_with_a_control_character_rejected() {
        let toml = make_manifest_toml(&[("description", "\"Normal desc\\nExecStart=/bin/evil\"")]);
        let error = parse_manifest(&toml).unwrap_err().to_string();
        assert!(error.contains("description"), "{error}");
    }
}
