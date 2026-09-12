// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The documented environment variables, as data.
//!
//! The `ENV_*` consts in `main.rs` give each variable its name; the rustdoc beside
//! them explains it to a reader of the source. Neither is reachable at runtime, so
//! this table carries the same explanations to `coolercontrold env`. It is the one
//! place a new variable has to be described, and `every_env_const_is_documented`
//! fails the build if a const is added without an entry here.

use std::io::{self, Write};
use std::ops::Not;

use crate::{
    ENV_CC_LOG, ENV_CERT_PATH, ENV_CONFIG_DIR, ENV_DATA_DIR, ENV_DBUS, ENV_DEVICE_EVENTS,
    ENV_HOST_IP4, ENV_HOST_IP6, ENV_KEY_PATH, ENV_NVML, ENV_PLUGINS_DIR, ENV_PORT,
    ENV_SERVICE_MANAGER, ENV_TLS,
};

/// The `ON`/`OFF` toggles all accept the same set, so it is written once.
const TOGGLE_VALUES: &str = "1 | 0 | ON | on | OFF | off";

/// What a toggle does when left unset. All of them default to enabled.
const TOGGLE_DEFAULT: &str = "ON (the feature is enabled)";

pub struct EnvVarDoc {
    pub name: &'static str,
    pub description: &'static str,
    pub values: &'static str,
    /// What applies when the variable is unset. Several fall back to a config
    /// setting rather than to a fixed value, which is worth stating plainly.
    pub default: &'static str,
}

/// `CC_SENSORS_DETECT` is only read on `x86_64`, where the const is compiled. It stays
/// in the table on every architecture, marked in its description, so that the
/// printed reference and the source scan agree regardless of the build target.
const SENSORS_DETECT_NAME: &str = "CC_SENSORS_DETECT";

/// `CC_RUNTIME_DRIVER` is only read under the `compio-rt` feature, where the const is
/// compiled. Named here the same way as `SENSORS_DETECT_NAME`, and for the same reason:
/// the printed reference and the source scan must agree regardless of the build's features.
const RUNTIME_DRIVER_NAME: &str = "CC_RUNTIME_DRIVER";

pub const ENV_VARS: &[EnvVarDoc] = &[
    EnvVarDoc {
        name: ENV_CC_LOG,
        description: "Log level. The --debug flag overrides it. The older \
                      COOLERCONTROL_LOG name is deprecated but still honored.",
        values: "ERROR | WARN | INFO | DEBUG | TRACE",
        default: "INFO",
    },
    EnvVarDoc {
        name: ENV_PORT,
        description: "Port the REST API listens on. The gRPC API takes this port plus one.",
        values: "a port number",
        default: "the config `port` setting, otherwise 11987",
    },
    EnvVarDoc {
        name: ENV_HOST_IP4,
        description: "IPv4 address the API binds to. Set it to a blank value to disable IPv4.",
        values: "an IPv4 address",
        default: "the config `ipv4_address` setting, otherwise all interfaces",
    },
    EnvVarDoc {
        name: ENV_HOST_IP6,
        description: "IPv6 address the API binds to. Set it to a blank value to disable IPv6.",
        values: "an IPv6 address",
        default: "the config `ipv6_address` setting, otherwise all interfaces",
    },
    EnvVarDoc {
        name: ENV_TLS,
        description: "Serve the REST API over TLS.",
        values: TOGGLE_VALUES,
        default: "the config `tls_enabled` setting",
    },
    EnvVarDoc {
        name: ENV_CERT_PATH,
        description: "Path to the TLS certificate file.",
        values: "a file path",
        default: "the config `tls_cert_path` setting, otherwise a generated certificate",
    },
    EnvVarDoc {
        name: ENV_KEY_PATH,
        description: "Path to the TLS private key file.",
        values: "a file path",
        default: "the config `tls_key_path` setting, otherwise a generated key",
    },
    EnvVarDoc {
        name: ENV_DBUS,
        description: "D-Bus integration, used to detect suspend, resume, and system \
                      power profile changes.",
        values: TOGGLE_VALUES,
        default: TOGGLE_DEFAULT,
    },
    EnvVarDoc {
        name: ENV_DEVICE_EVENTS,
        description: "Device change listener, which watches netlink uevents for hotplug.",
        values: TOGGLE_VALUES,
        default: TOGGLE_DEFAULT,
    },
    EnvVarDoc {
        name: SENSORS_DETECT_NAME,
        description: "Super-I/O sensor auto-detection at startup. Read on x86_64 only.",
        values: TOGGLE_VALUES,
        default: TOGGLE_DEFAULT,
    },
    EnvVarDoc {
        name: ENV_SERVICE_MANAGER,
        description: "Service manager integration. When off, plugins must be started manually.",
        values: TOGGLE_VALUES,
        default: TOGGLE_DEFAULT,
    },
    EnvVarDoc {
        name: ENV_NVML,
        description: "NVML integration for NVIDIA GPUs. When off, the CLI tools are used instead.",
        values: TOGGLE_VALUES,
        default: TOGGLE_DEFAULT,
    },
    EnvVarDoc {
        name: RUNTIME_DRIVER_NAME,
        description: "Reactor backend the async runtime uses. epoll avoids the io_uring wait \
                      being accounted as iowait, which shows an otherwise idle core as fully \
                      busy, at the cost of noticeably more wakeups. Only needed on kernels 6.5 \
                      to 6.14: 6.15+ drops the iowait accounting on its own. Read on builds \
                      with the compio runtime only.",
        values: "epoll | io_uring",
        default: "io_uring where the kernel supports it, otherwise epoll",
    },
    EnvVarDoc {
        name: ENV_CONFIG_DIR,
        description: "Directory holding the daemon configuration.",
        values: "a directory path",
        default: "/etc/coolercontrol",
    },
    EnvVarDoc {
        name: ENV_DATA_DIR,
        description: "Directory holding daemon data. The plugins directory is derived from it.",
        values: "a directory path",
        default: "/var/lib/coolercontrol",
    },
    EnvVarDoc {
        name: ENV_PLUGINS_DIR,
        description: "Directory holding service plugins. Overriding the data directory is \
                      usually enough.",
        values: "a directory path",
        default: "the `plugins` directory under the data directory",
    },
];

/// Prints the reference table.
///
/// Output goes to stdout rather than the logger: this runs before logging is set up,
/// and the result is meant for a terminal or a paste into a bug report.
pub fn print_table() {
    // A closed pipe (`env | head`) is an ordinary way for this command to end, not a
    // failure worth a panic, so a write error just stops the output.
    let _ = write_table(&mut io::stdout().lock());
}

fn write_table(out: &mut impl Write) -> io::Result<()> {
    writeln!(out, "CoolerControl daemon environment variables")?;
    writeln!(out)?;
    writeln!(
        out,
        "Set these in the daemon's systemd unit, or in the environment of whatever"
    )?;
    writeln!(out, "starts it. Values are read at startup.")?;
    writeln!(out)?;
    for var in ENV_VARS {
        debug_assert!(var.name.is_empty().not());
        writeln!(out, "  {}", var.name)?;
        for line in wrap(var.description, DESCRIPTION_WRAP_COLUMNS) {
            writeln!(out, "      {line}")?;
        }
        writeln!(out, "      Values:   {}", var.values)?;
        writeln!(out, "      Default:  {}", var.default)?;
        writeln!(out)?;
    }
    Ok(())
}

/// Column budget for a wrapped description, chosen to keep the deepest indented
/// line inside 100 columns.
const DESCRIPTION_WRAP_COLUMNS: usize = 88;

/// Greedy word wrap. A description is a short sentence or two of ASCII, so this
/// needs neither grapheme handling nor a dependency. A word longer than the limit
/// is left on its own line rather than split.
fn wrap(text: &str, columns: usize) -> Vec<String> {
    debug_assert!(columns > 0);
    let mut lines = Vec::with_capacity(2);
    let mut line = String::with_capacity(columns);
    for word in text.split_whitespace() {
        if line.is_empty() {
            line.push_str(word);
            continue;
        }
        if line.len() + 1 + word.len() > columns {
            // Cloned rather than taken so `line` keeps its capacity for the next one.
            lines.push(line.clone());
            line.clear();
            line.push_str(word);
            continue;
        }
        line.push(' ');
        line.push_str(word);
    }
    if line.is_empty().not() {
        lines.push(line);
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    /// Scans the `ENV_*` const declarations out of `main.rs` and checks each one has a
    /// table entry. The scan is textual rather than reflective because Rust cannot
    /// enumerate consts, and it reads the embedded source so it sees every declaration
    /// regardless of the target architecture's cfg gates.
    fn env_names_declared_in_main() -> Vec<String> {
        const MAIN_SOURCE: &str = include_str!("main.rs");
        let mut names = Vec::with_capacity(ENV_VARS.len() + 1);
        for line in MAIN_SOURCE.lines() {
            let trimmed = line.trim_start();
            let is_declaration =
                trimmed.starts_with("const ENV_") || trimmed.starts_with("pub const ENV_");
            if is_declaration.not() {
                continue;
            }
            let Some((_, after_equals)) = trimmed.split_once('=') else {
                continue;
            };
            let Some((_, after_quote)) = after_equals.split_once('"') else {
                continue;
            };
            let Some((name, _)) = after_quote.split_once('"') else {
                continue;
            };
            names.push(name.to_string());
        }
        names
    }

    /// The whole point of the table: adding an `ENV_*` const without describing it here
    /// must fail the build rather than ship a command that silently omits it.
    #[test]
    fn every_env_const_is_documented() {
        // The deprecated older name for CC_LOG. Still honored as a fallback, and named
        // inside the CC_LOG description, but deliberately not offered as its own entry.
        const DEPRECATED: &str = "COOLERCONTROL_LOG";
        let documented: HashSet<&str> = ENV_VARS.iter().map(|var| var.name).collect();
        let declared = env_names_declared_in_main();
        assert!(
            declared.len() >= ENV_VARS.len(),
            "scan found {} consts, fewer than the {} documented; the scan is broken",
            declared.len(),
            ENV_VARS.len()
        );
        for name in declared {
            if name == DEPRECATED {
                continue;
            }
            assert!(
                documented.contains(name.as_str()),
                "{name} is declared in main.rs but has no ENV_VARS entry"
            );
        }
    }

    /// The reverse direction: an entry naming a variable nothing reads would document a
    /// knob that does not exist.
    #[test]
    fn no_documented_var_is_undeclared() {
        let declared: HashSet<String> = env_names_declared_in_main().into_iter().collect();
        for var in ENV_VARS {
            assert!(
                declared.contains(var.name),
                "{} is documented but declared nowhere in main.rs",
                var.name
            );
        }
    }

    /// Negative space: no blank fields, no duplicates, and nothing outside the CC_
    /// namespace, since the command claims to list CoolerControl's own variables.
    #[test]
    fn entries_are_well_formed() {
        let mut seen = HashSet::with_capacity(ENV_VARS.len());
        for var in ENV_VARS {
            assert!(var.name.starts_with("CC_"), "{} is not a CC_ var", var.name);
            assert!(
                var.description.is_empty().not(),
                "{} has no description",
                var.name
            );
            assert!(var.values.is_empty().not(), "{} has no values", var.name);
            assert!(var.default.is_empty().not(), "{} has no default", var.name);
            assert!(
                var.description.ends_with('.'),
                "{} description is not a sentence",
                var.name
            );
            assert!(seen.insert(var.name), "{} is listed twice", var.name);
        }
    }

    fn rendered(write: fn(&mut Vec<u8>) -> io::Result<()>) -> String {
        let mut buffer = Vec::with_capacity(4096);
        write(&mut buffer).expect("a Vec write cannot fail");
        String::from_utf8(buffer).expect("output is UTF-8")
    }

    /// The table has to name every variable and stay inside the line limit, since it is
    /// read in a terminal and pasted into bug reports.
    #[test]
    fn table_lists_every_var_within_the_line_limit() {
        const LINE_LIMIT: usize = 100;
        let output = rendered(write_table);
        for var in ENV_VARS {
            assert!(
                output.contains(var.name),
                "{} missing from the table",
                var.name
            );
            assert!(output.contains(var.values), "{} values missing", var.name);
            assert!(output.contains(var.default), "{} default missing", var.name);
        }
        for line in output.lines() {
            assert!(
                line.len() <= LINE_LIMIT,
                "line over {LINE_LIMIT} columns: {line}"
            );
        }
    }

    /// Output stops quietly when the reader goes away, so `env | head` ends without a
    /// panic. Modelled with a writer that refuses every write.
    #[test]
    fn a_closed_pipe_is_not_an_error() {
        struct ClosedPipe;
        impl Write for ClosedPipe {
            fn write(&mut self, _buf: &[u8]) -> io::Result<usize> {
                Err(io::Error::from(io::ErrorKind::BrokenPipe))
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        assert!(write_table(&mut ClosedPipe).is_err());
        // `print_table` swallows it rather than panicking, which is what its `let _ =`
        // is for. Calling it here would write to the test's stdout, so the contract is
        // asserted on the writer layer above.
    }

    /// Wrapping keeps the printed table inside the line limit, and must not lose or
    /// reorder any word on the way.
    #[test]
    fn wrap_preserves_words_within_the_limit() {
        let text = "Log level. The --debug flag overrides it. The older COOLERCONTROL_LOG \
                    name is deprecated but still honored.";
        let lines = wrap(text, DESCRIPTION_WRAP_COLUMNS);
        assert!(lines.len() > 1, "the sample is long enough to wrap");
        for line in &lines {
            assert!(
                line.len() <= DESCRIPTION_WRAP_COLUMNS,
                "line too long: {line}"
            );
            assert!(
                line.starts_with(' ').not(),
                "line has leading space: {line}"
            );
        }
        assert_eq!(
            lines.join(" "),
            text.split_whitespace().collect::<Vec<_>>().join(" ")
        );
    }

    /// A word wider than the budget has nowhere to break, so it gets its own line
    /// rather than being split mid-token.
    #[test]
    fn wrap_keeps_an_overlong_word_whole() {
        let long = "a".repeat(DESCRIPTION_WRAP_COLUMNS + 10);
        let lines = wrap(&format!("short {long} tail"), DESCRIPTION_WRAP_COLUMNS);
        assert_eq!(lines, vec!["short".to_string(), long, "tail".to_string()]);
    }

    /// Empty input must not produce a blank line, which would print as stray
    /// indentation under the variable name.
    #[test]
    fn wrap_of_empty_text_has_no_lines() {
        assert!(wrap("", DESCRIPTION_WRAP_COLUMNS).is_empty());
    }
}
