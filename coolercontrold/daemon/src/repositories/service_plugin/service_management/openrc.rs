// SPDX-FileCopyrightText: 2025 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::{ensure_plugin_user, find_on_path, ServiceId, ServiceIdExt};
use crate::cc_fs;
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
use std::path::PathBuf;
use std::time::Duration;

const RC_SERVICE: &str = "rc-service";
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
        let dir_path = service_dir_path();
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
        let service_path = service_dir_path().join(service_id.to_service_name());
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

#[inline]
fn service_dir_path() -> PathBuf {
    PathBuf::from("/etc/init.d")
}

fn create_service_file(
    description: &str,
    provide: &str,
    service_definition: &ServiceDefinition,
) -> String {
    let mut script = String::new();
    warn_about_unrepresentable_args(service_definition);
    let args = service_definition
        .args
        .iter()
        .map(|arg| escape_shell_dquoted(arg))
        .collect::<Vec<_>>()
        .join(" ");
    let program_path = escape_shell_dquoted(&service_definition.executable.to_string_lossy());
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
        parts.push(format!("-u {username}"));
        // Same rule as the systemd unit: harden the unprivileged plugin, leave a plugin the
        // user deliberately gave root alone. `--no-new-privs` is a bare flag.
        parts.push("--no-new-privs".to_string());
    }
    if let Some(envs) = &service_definition.envs {
        for (var, val) in envs {
            parts.push(format!("-e {var}={}", escape_shell_dquoted(val)));
        }
    }
    parts.join(" ")
}

/// Neutralises the characters that keep their meaning inside a double-quoted shell word.
///
/// Everything this generator writes lands inside `name="..."` in a POSIX shell script, so
/// a value carrying `$`, a backtick, `"` or `\` would be expanded or would close the
/// quoting and let the rest of the value execute. Only these four matter inside double
/// quotes; the rest of the shell's metacharacters do not.
///
/// Note that this protects the script, not the argument's shape: see
/// [`warn_about_unrepresentable_args`].
fn escape_shell_dquoted(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        if matches!(character, '\\' | '"' | '$' | '`') {
            escaped.push('\\');
        }
        escaped.push(character);
    }
    escaped
}

/// Warns about arguments `OpenRC` cannot pass through intact.
///
/// `command_args` is one shell string that `OpenRC` word-splits, so an argument holding
/// whitespace arrives at the plugin as several arguments and there is no quoting that
/// prevents it. Saying so beats corrupting it silently, which is how the old allowlist
/// behaved. systemd has no such limit, so this is not worth rejecting in the manifest.
fn warn_about_unrepresentable_args(service_definition: &ServiceDefinition) {
    for arg in &service_definition.args {
        if arg.chars().any(char::is_whitespace) {
            warn!(
                "Plugin '{}' has the argument {arg:?}, which contains whitespace. OpenRC \
                 passes arguments as one word-split string, so the plugin will receive this \
                 as several arguments.",
                service_definition.service_id
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    fn command_args_of(args: Vec<String>) -> String {
        let mut definition = base_definition();
        definition.args = args;
        let script = create_service_file("Test", "test", &definition);
        script
            .lines()
            .find(|line| line.starts_with("command_args="))
            .expect("command_args is present")
            .to_string()
    }

    /// Goal: the four characters that keep their meaning inside a double-quoted shell
    /// word are neutralised. Everything here lands inside `name="..."` in a POSIX shell
    /// script, so an unescaped one of these would be expanded, or would close the quoting
    /// and let the remainder of the value execute.
    #[test]
    fn command_args_neutralises_shell_metacharacters() {
        assert_eq!(
            command_args_of(vec!["$HOME".into()]),
            r#"command_args="\$HOME""#
        );
        assert_eq!(
            command_args_of(vec!["`id`".into()]),
            r#"command_args="\`id\`""#
        );
        assert_eq!(
            command_args_of(vec![r#"a";id;""#.into()]),
            r#"command_args="a\";id;\"""#
        );
        assert_eq!(
            command_args_of(vec![r"C:\tmp".into()]),
            r#"command_args="C:\\tmp""#
        );
    }

    /// Goal: characters the old allowlist deleted now reach the plugin. None of these has
    /// any meaning inside double quotes, so none of them needs touching.
    #[test]
    fn command_args_preserves_previously_stripped_characters() {
        for arg in [
            "50%",
            "x~",
            "--listen=[::1]:8080",
            "~/.config/foo.toml",
            "*",
            ";",
            "p@ss!word",
            "https://ex.com/a?b=1&c=2",
        ] {
            let line = command_args_of(vec![arg.to_string()]);
            assert_eq!(line, format!("command_args=\"{arg}\""), "for {arg}");
        }
    }

    /// Goal: the program path is escaped too, since it is written into the same kind of
    /// shell assignment as the arguments.
    #[test]
    fn command_escapes_the_program_path() {
        let mut definition = base_definition();
        definition.executable = PathBuf::from("/opt/plugin/run$x");
        let script = create_service_file("Test", "test", &definition);
        assert!(
            script.contains(r#"command="/opt/plugin/run\$x""#),
            "{script}"
        );
    }

    /// Goal: an environment value goes through the same escaping, because it is joined
    /// into `supervise_daemon_args` and written into the same shell assignment.
    #[test]
    fn supervise_daemon_args_escapes_environment_values() {
        let mut definition = base_definition();
        definition.username = Some("cc-plugin-user".to_string());
        definition.envs = Some(vec![("LITERAL".into(), "$HOME".into())]);
        let args = build_supervise_daemon_args(&definition);
        assert!(args.contains(r"-e LITERAL=\$HOME"), "{args}");
    }

    /// Goal: an argument containing whitespace is reported rather than silently reshaped.
    /// `command_args` is one shell string that OpenRC word-splits, so no quoting makes it
    /// arrive as a single argument; saying so is the honest option.
    #[test]
    fn whitespace_in_an_argument_is_reported_not_hidden() {
        let mut definition = base_definition();
        definition.args = vec!["--name".into(), "My Device".into()];
        // The value is still written out unaltered: it splits, but nothing is deleted.
        let line = command_args_of(definition.args.clone());
        assert_eq!(line, r#"command_args="--name My Device""#);
        // And the warning names it, which is what `create_service_file` emits.
        warn_about_unrepresentable_args(&definition);
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

    #[test]
    fn service_file_contains_required_directives() {
        // A basic service file must contain the shebang, supervisor,
        // command, logger directives, and depend block.
        let script =
            create_service_file("Test Plugin", "cc-plugin-test-plugin", &base_definition());
        assert!(script.starts_with("#!/sbin/openrc-run"));
        assert!(script.contains("description=\"Test Plugin\""));
        assert!(script.contains("supervisor=\"supervise-daemon\""));
        assert!(script.contains("command=\"/usr/bin/test-plugin\""));
        assert!(script.contains("command_args=\"--port 8080\""));
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
        assert!(script.contains("supervise_daemon_args=\"-u cc-plugin-user --no-new-privs\""));
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
        def.envs = Some(vec![("MY_VAR".to_string(), "value".to_string())]);
        let script = create_service_file("Test Plugin", "cc-plugin-test-plugin", &def);
        assert!(script.contains("supervise_daemon_args=\"-e MY_VAR=value\""));
    }

    #[test]
    fn service_file_combines_user_and_env_vars() {
        // When both username and env vars are set, they must appear
        // together in supervise_daemon_args.
        let mut def = base_definition();
        def.username = Some("cc-plugin-user".to_string());
        def.envs = Some(vec![("KEY".to_string(), "val".to_string())]);
        let script = create_service_file("Test Plugin", "cc-plugin-test-plugin", &def);
        assert!(script
            .contains("supervise_daemon_args=\"-u cc-plugin-user --no-new-privs -e KEY=val\""));
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
