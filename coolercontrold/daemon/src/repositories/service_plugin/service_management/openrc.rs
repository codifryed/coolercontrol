// SPDX-FileCopyrightText: 2025 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::{ensure_plugin_user, find_on_path, ServiceId, ServiceIdExt};
use crate::cc_fs;
use crate::paths;
use crate::repositories::service_plugin::service_management::manager::{
    ServiceDefinition, ServiceManager, ServiceStatus,
};
use crate::repositories::service_plugin::service_plugin_repo::CC_PLUGIN_USER;
use crate::repositories::utils::DirectCommand;
use crate::rt::sleep;
use anyhow::{anyhow, Result};
use log::warn;
use std::fmt::Write;
use std::fs::Permissions;
use std::ops::Not;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::time::Duration;

const RC_SERVICE: &str = "rc-service";
/// The directory the plugin init scripts are written to. `CC_SERVICE_DIR` overrides it, for
/// distros that mount `/etc` read only.
const DEFAULT_OPENRC_DIR: &str = "/etc/init.d";
const RC_SERVICE_TIMEOUT: Duration = Duration::from_secs(10);
const SERVICE_FILE_PERMISSIONS: u32 = 0o755;
/// `rc-service stop` returns once it has signalled the supervisor, not once the supervised
/// process has gone. These bound the wait for it to actually leave.
const STOP_VERIFY_INTERVAL: Duration = Duration::from_millis(250);
const STOP_VERIFY_ATTEMPTS: u8 = 20;
/// A single poll would race every shutdown that is not instant.
const _: () = assert!(STOP_VERIFY_ATTEMPTS > 1);

#[derive(Clone, Debug, Default)]
pub struct OpenRcManager {}

impl OpenRcManager {
    pub fn detected() -> bool {
        find_on_path(RC_SERVICE).is_some()
    }

    /// Returns `(exit_code, stdout, stderr)`. `Err` only on spawn failure or timeout.
    async fn rc_service(cmd: &str, service_id: &ServiceId) -> Result<(i32, String, String)> {
        DirectCommand::new(RC_SERVICE, RC_SERVICE_TIMEOUT)
            .arg(service_id.to_service_name())
            .arg(cmd)
            .run_with_code()
            .await
    }

    /// Waits until the service is no longer running.
    ///
    /// Acting on a stop that has been reported but not finished is what leaves two
    /// processes alive: the old supervisor is still up when the next start creates a
    /// second one, and `supervise-daemon` keeps respawning the child of each.
    async fn await_stopped(&self, service_id: &ServiceId) -> Result<()> {
        for _ in 0..STOP_VERIFY_ATTEMPTS {
            // Any status but running means there is nothing left to wait for: stopped, or
            // the script is already gone. A status that cannot be read is not proof that
            // the service went down, so it keeps waiting.
            if let Ok(status) = self.status(service_id).await {
                if matches!(status, ServiceStatus::Running).not() {
                    return Ok(());
                }
            }
            sleep(STOP_VERIFY_INTERVAL).await;
        }
        Err(anyhow!(
            "Service {} was still running after its stop was reported",
            service_id.to_service_name()
        ))
    }
}

impl ServiceManager for OpenRcManager {
    async fn add(&self, service_definition: ServiceDefinition) -> Result<()> {
        let dir_path = paths::service_dir(Path::new(DEFAULT_OPENRC_DIR));
        cc_fs::create_dir_all(&dir_path).await?;
        let service_name = service_definition.service_id.to_service_name();
        let service_description = service_definition.service_id.to_description();
        let service_path = dir_path.join(&service_name);
        if service_definition.username.is_some() {
            ensure_plugin_user(CC_PLUGIN_USER).await;
        }
        let service_file =
            create_service_file(&service_description, &service_name, &service_definition);
        cc_fs::write_string(&service_path, service_file).await?;
        cc_fs::set_permissions(
            &service_path,
            Permissions::from_mode(SERVICE_FILE_PERMISSIONS),
        )
        .await
    }

    async fn remove(&self, service_id: &ServiceId) -> Result<()> {
        // The stop has to be confirmed before the script goes. `rc-service` needs the
        // script to address the service at all, so unlinking it after a failed stop
        // strands a supervised process that no later service command can reach, and
        // `supervise-daemon` goes on respawning its child until the machine reboots.
        if let ServiceStatus::Unmanaged = self.status(service_id).await? {
            // Never installed, or already removed: there is nothing to stop or unlink.
            return Ok(());
        }
        self.stop(service_id).await?;
        let service_path =
            paths::service_dir(Path::new(DEFAULT_OPENRC_DIR)).join(service_id.to_service_name());
        cc_fs::remove_file(service_path).await
    }

    async fn start(&self, service_id: &ServiceId) -> Result<()> {
        let (code, _, stderr) = Self::rc_service("start", service_id).await?;
        if code != 0 {
            Err(anyhow!(
                "rc-service start {} failed: {stderr}",
                service_id.to_service_name()
            ))
        } else {
            Ok(())
        }
    }

    async fn stop(&self, service_id: &ServiceId) -> Result<()> {
        let (code, _, stderr) = Self::rc_service("stop", service_id).await?;
        if code != 0 {
            return Err(anyhow!(
                "rc-service stop {} failed: {stderr}",
                service_id.to_service_name()
            ));
        }
        self.await_stopped(service_id).await
    }

    async fn restart(&self, service_id: &ServiceId) -> Result<()> {
        let (code, _, stderr) = Self::rc_service("restart", service_id).await?;
        if code != 0 {
            Err(anyhow!(
                "rc-service restart {} failed: {stderr}",
                service_id.to_service_name()
            ))
        } else {
            Ok(())
        }
    }

    async fn status(&self, service_id: &ServiceId) -> Result<ServiceStatus> {
        let (code, stdout, stderr) = Self::rc_service("status", service_id).await?;
        let status_text = if stderr.trim().is_empty() {
            stdout.trim().to_string()
        } else {
            stderr.trim().to_string()
        };
        #[allow(clippy::match_same_arms)]
        match code {
            0 => Ok(ServiceStatus::Running),
            // Exit code 3 is the POSIX standard for "stopped".
            3 => Ok(ServiceStatus::Stopped(Some(status_text))),
            // Exit code 1: either "does not exist" or a crashed/unclear state.
            1 if status_text.contains("does not exist") => Ok(ServiceStatus::Unmanaged),
            1 => Ok(ServiceStatus::Stopped(Some(status_text))),
            _ => Err(anyhow!(
                "Unexpected rc-service status exit code {} for {}: {}",
                code,
                service_id.to_service_name(),
                status_text,
            )),
        }
    }
}

fn create_service_file(
    description: &str,
    provide: &str,
    service_definition: &ServiceDefinition,
) -> String {
    let mut script = String::new();
    warn_about_collapsed_whitespace(service_definition);
    let args = service_definition
        .args
        .iter()
        .map(|arg| openrc_word(arg))
        .collect::<Vec<_>>()
        .join(" ");
    let program_path = openrc_word(&service_definition.executable.to_string_lossy());
    let _ = writeln!(script, "#!/sbin/openrc-run");
    let _ = writeln!(script);
    let _ = writeln!(script, "description=\"{description}\"");
    let _ = writeln!(script, "supervisor=\"supervise-daemon\"");
    let _ = writeln!(script, "command=\"{program_path}\"");
    let _ = writeln!(script, "command_args=\"{args}\"");
    let sd_args = build_supervise_daemon_args(service_definition);
    if sd_args.is_empty().not() {
        let _ = writeln!(script, "supervise_daemon_args=\"{sd_args}\"");
    }
    let _ = writeln!(script, "output_logger=\"logger -et '${{RC_SVCNAME}}'\"");
    let _ = writeln!(script, "error_logger=\"logger -et '${{RC_SVCNAME}}' -p3\"");
    let _ = writeln!(script);
    let _ = writeln!(script, "depend() {{");
    let _ = writeln!(script, "    use logger");
    let _ = writeln!(script, "    provide {provide}");
    let _ = write!(script, "}}");
    script
}

/// Builds the `supervise_daemon_args` value from user and env settings.
fn build_supervise_daemon_args(service_definition: &ServiceDefinition) -> String {
    let mut parts = Vec::with_capacity(4);
    if let Some(username) = &service_definition.username {
        parts.push(format!("-u {}", openrc_word(username)));
        // Same rule as the systemd unit: harden the unprivileged plugin, leave a plugin the
        // user deliberately gave root alone. `--no-new-privs` is a bare flag, so it needs
        // no quoting of its own.
        parts.push("--no-new-privs".to_string());
    }
    if let Some(envs) = &service_definition.envs {
        for env_var in envs {
            // Quoted whole, so `supervise-daemon` receives one `VAR=value` argument even
            // when the value contains whitespace.
            let assignment = format!("{}={}", env_var.name, env_var.value);
            parts.push(format!("-e {}", openrc_word(&assignment)));
        }
    }
    parts.join(" ")
}

/// One word for the generated script, quoted so it survives to `supervise-daemon` intact.
///
/// Two layers read it, and both have to be satisfied at once. `sh/supervise-daemon.sh`
/// runs `eval supervise-daemon ... ${supervise_daemon_args} $command -- $command_args`,
/// so the shell parses the assignment in this script and then `eval` parses the resulting
/// value a second time. Escaping only for the assignment is not enough: a value written
/// as a backslash-escaped backtick leaves a literal backtick pair in the variable, and
/// the `eval` then executes it. The allowlist this replaced hid that by deleting the
/// character outright.
///
/// So each word is single-quoted for the `eval`, then escaped for the double-quoted
/// assignment that carries it there. Single quotes are what make a value inert: the shell
/// expands nothing inside them.
///
/// One thing this still cannot carry: `$command_args` is expanded unquoted, so the shell
/// field-splits it before `eval` rejoins the fields with single spaces. An argument
/// holding repeated whitespace therefore arrives with it collapsed. Tabs and newlines
/// cannot reach here at all, since the manifest parser rejects control characters.
fn openrc_word(value: &str) -> String {
    debug_assert!(
        value.chars().any(char::is_control).not(),
        "the manifest parser rejects control characters before they reach here"
    );
    escape_dquoted(&single_quoted(value))
}

/// Warns about an argument `OpenRC` cannot carry through unaltered.
///
/// `$command_args` is expanded unquoted, so the shell field-splits it before `eval`
/// rejoins the fields with single spaces. Nothing here can prevent that, but a plugin
/// that behaves differently under `OpenRC` than under systemd should say why rather than
/// leave the author to find it.
fn warn_about_collapsed_whitespace(service_definition: &ServiceDefinition) {
    for arg in &service_definition.args {
        if whitespace_is_collapsed(arg) {
            warn!(
                "Plugin {} argument '{arg}' reaches it with the whitespace collapsed: \
                 OpenRC re-splits the command line and cannot carry it verbatim",
                service_definition.service_id
            );
        }
    }
}

/// Whether the shell's field splitting would alter this argument's whitespace.
fn whitespace_is_collapsed(arg: &str) -> bool {
    arg.split_whitespace().collect::<Vec<_>>().join(" ") != arg
}

/// POSIX single-quoting: wrap in `'`, and close, escape, reopen around each `'`.
fn single_quoted(value: &str) -> String {
    let mut quoted = String::with_capacity(value.len() + 2);
    quoted.push('\'');
    debug_assert!(quoted.starts_with('\''), "a quoted word must open quoted");
    for character in value.chars() {
        if character == '\'' {
            quoted.push_str("'\\''");
        } else {
            quoted.push(character);
        }
    }
    quoted.push('\'');
    quoted
}

/// Escapes the four characters that keep their meaning inside the double-quoted
/// assignment a value is written into. Applied after [`single_quoted`], whose own
/// backslashes need it too.
fn escape_dquoted(value: &str) -> String {
    debug_assert!(
        value.starts_with('\'') && value.ends_with('\''),
        "escape_dquoted takes a single-quoted word"
    );
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        if matches!(character, '\\' | '"' | '$' | '`') {
            escaped.push('\\');
        }
        escaped.push(character);
    }
    // Nothing that ends the assignment or starts an expansion may survive unescaped: the
    // rest of the value would then be read as script rather than as a word.
    debug_assert!(
        escaped
            .char_indices()
            .filter(|(_, character)| matches!(character, '"' | '$' | '`'))
            .all(|(index, _)| escaped[..index].ends_with('\\')),
        "{escaped} would break out of its assignment"
    );
    escaped
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repositories::service_plugin::service_manifest::EnvVar;
    use std::path::PathBuf;

    fn env(name: &str, value: &str) -> EnvVar {
        EnvVar::new(name, value).unwrap()
    }

    fn base_definition() -> ServiceDefinition {
        ServiceDefinition {
            service_id: "test-plugin".to_string(),
            executable: PathBuf::from("/usr/bin/test-plugin"),
            args: vec!["--port".to_string(), "8080".to_string()],
            username: None,
            wrk_dir: None,
            envs: None,
            disable_restart_on_failure: false,
        }
    }

    fn line_of(script: &str, variable: &str) -> String {
        script
            .lines()
            .find(|line| line.starts_with(&format!("{variable}=")))
            .unwrap_or_else(|| panic!("{variable} is present in {script}"))
            .to_string()
    }

    fn command_args_of(args: Vec<String>) -> String {
        let mut definition = base_definition();
        definition.args = args;
        line_of(
            &create_service_file("Test", "test", &definition),
            "command_args",
        )
    }

    /// Puts a generated assignment through a real shell exactly the way OpenRC does, and
    /// returns the words `supervise-daemon` would receive.
    ///
    /// This is the only honest way to test the escaping. Two layers parse the value, the
    /// assignment and then `eval`, and reasoning about their composition by hand is what
    /// produced a command injection on the first attempt. `printf` prints one word per
    /// line, which is what the caller compares against.
    fn words_after_eval(assignment: &str, variable: &str) -> Vec<String> {
        // `eval set -- $var` unquoted, which is exactly how `supervise-daemon.sh`
        // expands it: the shell field-splits the value, `eval` rejoins the fields with
        // single spaces and parses the result. The words are then printed outside any
        // `eval`, since a second parse would consume printf's own escape.
        let program = format!(
            "{assignment}\neval set -- ${variable}\nfor word in \"$@\"; do printf '%s\\n' \"$word\"; done\n"
        );
        let output = std::process::Command::new("/bin/sh")
            .arg("-c")
            .arg(&program)
            .output()
            .expect("/bin/sh runs");
        assert!(
            output.status.success(),
            "shell failed on {program}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(str::to_string)
            .collect()
    }

    fn args_as_the_plugin_sees_them(args: Vec<String>) -> Vec<String> {
        words_after_eval(&command_args_of(args), "command_args")
    }

    /// Goal: an argument arrives at the plugin byte for byte, including every character
    /// the old allowlist deleted. This is the reported bug, checked through a real shell
    /// rather than against an expected string.
    #[test]
    fn init_scripts_default_to_openrcs_own_directory() {
        // Goal: a daemon with no CC_SERVICE_DIR override writes where OpenRC has always
        // read from. Method: the override is sandboxed off in test builds, so resolving
        // this manager's default must give that directory back unchanged.
        assert_eq!(
            paths::service_dir(Path::new(DEFAULT_OPENRC_DIR)),
            PathBuf::from("/etc/init.d")
        );
    }

    #[test]
    fn arguments_reach_the_plugin_unaltered() {
        let args: Vec<String> = [
            "--rate",
            "50%",
            "x~",
            "--listen=[::1]:8080",
            "~/.config/foo.toml",
            "*",
            ";",
            "p@ss!word",
            "https://ex.com/a?b=1&c=2",
            r"C:\tmp",
            "--name",
            "My Device",
            "it's",
            r#"say "hi""#,
        ]
        .iter()
        .map(|arg| (*arg).to_string())
        .collect();
        assert_eq!(args_as_the_plugin_sees_them(args.clone()), args);
    }

    /// Goal: the injection this escaping exists to stop. `supervise-daemon.sh` runs the
    /// value through `eval`, so a value carrying a command substitution or a variable
    /// reference would be executed or expanded as root. Each must arrive as literal text.
    #[test]
    fn arguments_are_never_expanded_or_executed_by_the_eval() {
        for payload in [
            "`id`",
            "$(id)",
            "$HOME",
            "${HOME}",
            "a\";id;\"b",
            "a';id;'b",
            "$(touch /tmp/coolercontrol-openrc-injection-probe)",
        ] {
            let words = args_as_the_plugin_sees_them(vec![payload.to_string()]);
            assert_eq!(words, vec![payload.to_string()], "for {payload}");
        }
        assert!(
            std::path::Path::new("/tmp/coolercontrol-openrc-injection-probe")
                .exists()
                .not(),
            "the eval executed a command substitution"
        );
    }

    /// Goal: an argument containing whitespace arrives as one argument. The `eval` is
    /// what makes this possible, since single quotes survive to be parsed there.
    #[test]
    fn whitespace_stays_inside_one_argument() {
        assert_eq!(
            args_as_the_plugin_sees_them(vec!["--name".into(), "My Device".into()]),
            vec!["--name".to_string(), "My Device".to_string()]
        );
    }

    /// Goal: the program path goes through the same two layers, since `$command` is
    /// expanded unquoted inside the same `eval`.
    #[test]
    fn the_program_path_survives_the_eval() {
        let mut definition = base_definition();
        definition.executable = PathBuf::from("/opt/my plugin/run$x");
        let script = create_service_file("Test", "test", &definition);
        assert_eq!(
            words_after_eval(&line_of(&script, "command"), "command"),
            vec!["/opt/my plugin/run$x".to_string()]
        );
    }

    /// Goal: an environment value reaches `supervise-daemon` as a single `VAR=value`
    /// argument, because `supervise_daemon_args` is expanded inside the same `eval`.
    #[test]
    fn environment_values_survive_the_eval() {
        let mut definition = base_definition();
        definition.username = Some("cc-plugin-user".to_string());
        definition.envs = Some(vec![
            env("GREETING", "hello world"),
            env("LITERAL", "$HOME"),
        ]);
        let script = create_service_file("Test", "test", &definition);
        let words = words_after_eval(
            &line_of(&script, "supervise_daemon_args"),
            "supervise_daemon_args",
        );
        assert!(
            words.contains(&"GREETING=hello world".to_string()),
            "{words:?}"
        );
        assert!(words.contains(&"LITERAL=$HOME".to_string()), "{words:?}");
        assert!(words.contains(&"--no-new-privs".to_string()), "{words:?}");
    }

    /// Goal: the wait for a reported stop to finish must terminate. An unbounded wait
    /// here would hang plugin registration on a service that never goes down, which is
    /// worse than the orphan it exists to prevent. Method: bound the loop arithmetic.
    #[test]
    fn stop_verification_terminates() {
        let window = STOP_VERIFY_INTERVAL * u32::from(STOP_VERIFY_ATTEMPTS);
        assert!(window > Duration::ZERO, "a zero window verifies nothing");
        assert!(
            window <= RC_SERVICE_TIMEOUT,
            "the verify window must not outlast the command timeout that precedes it"
        );
    }

    /// Goal: the one thing OpenRC cannot carry verbatim is recognised, so the author is
    /// told rather than left to find that the plugin behaves differently from systemd.
    /// Method: the predicate the warning is gated on, over the shapes that matter.
    #[test]
    fn collapsed_whitespace_is_recognised() {
        for altered in ["--flag  value", " --flag", "--flag ", "a  b  c"] {
            assert!(whitespace_is_collapsed(altered), "{altered} is altered");
        }
        for intact in ["--flag value", "--flag", "", "--port=8080"] {
            assert!(
                whitespace_is_collapsed(intact).not(),
                "{intact} survives intact"
            );
        }
    }

    #[test]
    fn service_file_contains_required_directives() {
        // A basic service file must contain the shebang, supervisor,
        // command, logger directives, and depend block.
        let script =
            create_service_file("Test Plugin", "cc-plugin-test-plugin", &base_definition());
        assert!(script.starts_with("#!/sbin/openrc-run"));
        assert!(script.contains("description=\"Test Plugin\""));
        assert!(script.contains("supervisor=\"supervise-daemon\""));
        assert!(script.contains("command=\"'/usr/bin/test-plugin'\""));
        assert!(script.contains("command_args=\"'--port' '8080'\""));
        assert!(script.contains("provide cc-plugin-test-plugin"));
        assert!(script.contains("use logger"));
    }

    #[test]
    fn service_file_does_not_use_old_background_mode() {
        // The script must use supervise-daemon, not the older
        // command_background approach.
        let script =
            create_service_file("Test Plugin", "cc-plugin-test-plugin", &base_definition());
        assert!(script.contains("command_background").not());
        assert!(script.contains("pidfile").not());
        assert!(script.contains("output_log=").not());
        assert!(script.contains("error_log=").not());
    }

    #[test]
    fn service_file_logs_to_syslog() {
        // Both stdout and stderr must be piped through logger to
        // syslog, matching the main daemon's init script pattern.
        let script =
            create_service_file("Test Plugin", "cc-plugin-test-plugin", &base_definition());
        assert!(script.contains("output_logger=\"logger -et '${RC_SVCNAME}'\""));
        assert!(script.contains("error_logger=\"logger -et '${RC_SVCNAME}' -p3\""));
    }

    #[test]
    fn service_file_includes_user_when_specified() {
        // When a username is provided, the supervise-daemon must
        // receive the -u flag to run as that user.
        let mut def = base_definition();
        def.username = Some("cc-plugin-user".to_string());
        let script = create_service_file("Test Plugin", "cc-plugin-test-plugin", &def);
        assert!(script.contains("supervise_daemon_args=\"-u 'cc-plugin-user' --no-new-privs\""));
    }

    /// Goal: an unprivileged plugin must not be able to climb back out through a setuid binary
    /// or a file capability, since running it as its own user is the only thing containing it.
    /// Methodology: generate with a username and check for the bare flag.
    #[test]
    fn service_file_blocks_privilege_escalation_for_an_unprivileged_plugin() {
        let mut def = base_definition();
        def.username = Some("cc-plugin-user".to_string());

        let script = create_service_file("Test Plugin", "cc-plugin-test-plugin", &def);

        assert!(script.contains("--no-new-privs"), "{script}");
    }

    /// Goal: a plugin the user deliberately gave root must keep working. The flag would add
    /// nothing there (it is already root) but would break a setuid or capability helper it calls.
    /// Methodology: generate without a username, which is how a privileged plugin is expressed.
    #[test]
    fn service_file_does_not_restrict_a_privileged_plugin() {
        let script =
            create_service_file("Test Plugin", "cc-plugin-test-plugin", &base_definition());

        assert!(script.contains("--no-new-privs").not(), "{script}");
    }

    #[test]
    fn service_file_omits_daemon_args_when_unneeded() {
        // Without a username or envs, supervise_daemon_args must be absent.
        let script =
            create_service_file("Test Plugin", "cc-plugin-test-plugin", &base_definition());
        assert!(script.contains("supervise_daemon_args").not());
    }

    #[test]
    fn service_file_includes_env_vars() {
        // Environment variables must appear as -e flags in
        // supervise_daemon_args.
        let mut def = base_definition();
        def.envs = Some(vec![env("MY_VAR", "value")]);
        let script = create_service_file("Test Plugin", "cc-plugin-test-plugin", &def);
        assert!(script.contains("supervise_daemon_args=\"-e 'MY_VAR=value'\""));
    }

    #[test]
    fn service_file_combines_user_and_env_vars() {
        // When both username and env vars are set, they must appear
        // together in supervise_daemon_args.
        let mut def = base_definition();
        def.username = Some("cc-plugin-user".to_string());
        def.envs = Some(vec![env("KEY", "val")]);
        let script = create_service_file("Test Plugin", "cc-plugin-test-plugin", &def);
        assert!(script
            .contains("supervise_daemon_args=\"-u 'cc-plugin-user' --no-new-privs -e 'KEY=val'\""));
    }

    #[test]
    fn service_file_omits_daemon_args_for_empty_envs() {
        // An empty envs list with no username must not produce
        // supervise_daemon_args.
        let mut def = base_definition();
        def.envs = Some(vec![]);
        let script = create_service_file("Test Plugin", "cc-plugin-test-plugin", &def);
        assert!(script.contains("supervise_daemon_args").not());
    }
}
