// SPDX-FileCopyrightText: 2025 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use crate::cc_fs;
use crate::repositories::service_plugin::service_management::manager::{
    ServiceDefinition, ServiceManager, ServiceStatus,
};
use crate::repositories::service_plugin::service_management::{
    ensure_plugin_user, find_on_path, ServiceId, ServiceIdExt,
};
use crate::repositories::service_plugin::service_plugin_repo::CC_PLUGIN_USER;
use crate::repositories::utils::DirectCommand;
use crate::rt::sleep;
use anyhow::{anyhow, Result};
use std::fs::Permissions;
use std::ops::Not;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::time::Duration;
use strum::Display;

const SYSTEMCTL: &str = "systemctl";
const SYSTEMCTL_TIMEOUT: Duration = Duration::from_secs(10);
const SERVICE_FILE_PERMISSIONS: u32 = 0o644;
/// `systemctl stop` waits for the unit, but a unit that ignores its stop signal outlives it.
/// These bound the wait for the unit to actually leave.
const STOP_VERIFY_INTERVAL: Duration = Duration::from_millis(250);
const STOP_VERIFY_ATTEMPTS: u8 = 20;

#[derive(Clone, Debug)]
pub struct SystemdConfig {
    /// interval in seconds to limit number of `burst` starts
    pub start_limit_interval_sec: Option<u32>,
    /// number of starts allowed in `interval`
    pub start_limit_burst: Option<u32>,
    /// restart type (on-failure, always, etc.)
    pub restart: SystemdServiceRestartType,
    /// number of seconds to wait between stopping and starting service
    pub restart_sec: Option<u32>,
    /// number of seconds to wait for service to exit on it own, before sending SIGTERM
    pub timeout_stop_sec: Option<u32>,
}

impl Default for SystemdConfig {
    fn default() -> Self {
        Self {
            start_limit_interval_sec: Some(60),
            start_limit_burst: Some(10),
            restart: SystemdServiceRestartType::OnFailure,
            restart_sec: Some(1),
            timeout_stop_sec: Some(3),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct SystemdManager {
    pub config: SystemdConfig,
}

impl SystemdManager {
    pub fn detected() -> bool {
        find_on_path(SYSTEMCTL).is_some()
    }

    /// Returns `(exit_code, stdout, stderr)`. `Err` only on spawn failure or timeout.
    async fn systemctl(cmd: &str, service_id: &ServiceId) -> Result<(i32, String, String)> {
        DirectCommand::new(SYSTEMCTL, SYSTEMCTL_TIMEOUT)
            .arg(cmd)
            .arg(service_id.to_service_name())
            .run_with_code()
            .await
    }

    /// Runs a `systemctl` subcommand that takes no unit argument.
    async fn systemctl_global(cmd: &str) -> Result<(i32, String, String)> {
        DirectCommand::new(SYSTEMCTL, SYSTEMCTL_TIMEOUT)
            .arg(cmd)
            .run_with_code()
            .await
    }

    /// Waits until the unit is no longer running, so no caller acts on a stop that has
    /// been reported but has not finished.
    async fn await_stopped(&self, service_id: &ServiceId) -> Result<()> {
        for _ in 0..STOP_VERIFY_ATTEMPTS {
            // Any status but running means there is nothing left to wait for: stopped, or
            // the unit file is already gone. A status that cannot be read is not proof that
            // the unit went down, so it keeps waiting.
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

impl ServiceManager for SystemdManager {
    async fn add(&self, service_definition: ServiceDefinition) -> Result<()> {
        let dir_path = systemd_global_dir_path();
        cc_fs::create_dir_all(&dir_path).await?;
        let service_name = service_definition.service_id.to_service_name();
        let service_path = dir_path.join(format!("{service_name}.service"));
        let service_description = service_definition.service_id.to_description();
        if service_definition.username.is_some() {
            ensure_plugin_user(CC_PLUGIN_USER).await;
        }
        let unit_file = create_unit_file(&self.config, &service_description, service_definition)?;
        cc_fs::write_string(&service_path, unit_file).await?;
        cc_fs::set_permissions(
            &service_path,
            Permissions::from_mode(SERVICE_FILE_PERMISSIONS),
        )
        .await?;
        // The definition on disk just changed. systemd serves the copy it already has
        // until it is told to re-read them, so without this the unit keeps running under
        // the old definition and the write above has no effect.
        let (code, _, stderr) = Self::systemctl_global("daemon-reload").await?;
        if code != 0 {
            return Err(anyhow!("systemctl daemon-reload failed: {stderr}"));
        }
        Ok(())
    }

    async fn remove(&self, service_id: &ServiceId) -> Result<()> {
        // The stop has to be confirmed before the unit file goes, for the same reason it
        // does under OpenRC: removing the definition of a unit that is still up leaves a
        // process behind that no later service command can address.
        if let ServiceStatus::Unmanaged = self.status(service_id).await? {
            // Never installed, or already removed: there is nothing to stop or unlink.
            return Ok(());
        }
        self.stop(service_id).await?;
        let dir_path = systemd_global_dir_path();
        let service_name = service_id.to_service_name();
        let service_path = dir_path.join(format!("{service_name}.service"));
        cc_fs::remove_file(service_path).await
    }

    async fn start(&self, service_id: &ServiceId) -> Result<()> {
        let (code, _, stderr) = Self::systemctl("start", service_id).await?;
        if code != 0 {
            Err(anyhow!(
                "systemctl start {} failed: {stderr}",
                service_id.to_service_name()
            ))
        } else {
            Ok(())
        }
    }

    async fn stop(&self, service_id: &ServiceId) -> Result<()> {
        let (code, _, stderr) = Self::systemctl("stop", service_id).await?;
        if code != 0 {
            return Err(anyhow!(
                "systemctl stop {} failed: {stderr}",
                service_id.to_service_name()
            ));
        }
        self.await_stopped(service_id).await
    }

    async fn restart(&self, service_id: &ServiceId) -> Result<()> {
        let (code, _, stderr) = Self::systemctl("restart", service_id).await?;
        if code != 0 {
            Err(anyhow!(
                "systemctl restart {} failed: {stderr}",
                service_id.to_service_name()
            ))
        } else {
            Ok(())
        }
    }

    /// See: `https://www.freedesktop.org/software/systemd/man/latest/systemctl.html#Exit%20status`
    async fn status(&self, service_id: &ServiceId) -> Result<ServiceStatus> {
        let (code, _, _) = Self::systemctl("status", service_id).await?;
        match code {
            4 => Ok(ServiceStatus::Unmanaged),
            3 => Ok(ServiceStatus::Stopped(None)),
            0 => Ok(ServiceStatus::Running),
            _ => Err(anyhow!("Unexpected systemctl status exit code: {code}")),
        }
    }
}

#[inline]
fn systemd_global_dir_path() -> PathBuf {
    PathBuf::from("/etc/systemd/system")
}

fn create_unit_file(
    config: &SystemdConfig,
    description: &String,
    service_definition: ServiceDefinition,
) -> Result<String> {
    use std::fmt::Write;
    let mut service = String::new();
    writeln!(service, "[Unit]")?;
    writeln!(service, "Description={description}")?;
    if let Some(start_limit_interval) = config.start_limit_interval_sec {
        writeln!(service, "StartLimitIntervalSec={start_limit_interval}")?;
    }
    if let Some(start_limit_burst) = config.start_limit_burst {
        writeln!(service, "StartLimitBurst={start_limit_burst}")?;
    }
    writeln!(service, "[Service]")?;
    writeln!(service, "Type=simple")?;
    if let Some(username) = service_definition.username {
        writeln!(service, "User={username}")?;
        writeln!(service, "Group={username}")?;
        // Stops the plugin user escalating through a setuid binary or a file capability, which
        // is the whole point of running it unprivileged. Deliberately not applied to a
        // `privileged = true` plugin: that one already runs as root, so this would add nothing
        // while breaking any setuid or capability helper it legitimately calls.
        writeln!(service, "NoNewPrivileges=true")?;
    }
    if let Some(working_directory) = service_definition.wrk_dir {
        writeln!(
            service,
            "WorkingDirectory={}",
            working_directory.to_string_lossy()
        )?;
    }
    if let Some(env_vars) = service_definition.envs {
        for (var, val) in env_vars {
            let _ = writeln!(service, "Environment={}", escape_environment(&var, &val));
        }
    }
    // The program is quoted like any other word: a path containing a space would
    // otherwise be split into two arguments, and one beginning with '-' would be read as
    // an ExecStart prefix.
    let program = escape_exec_word(&service_definition.executable.to_string_lossy());
    let args = service_definition
        .args
        .iter()
        .map(|arg| escape_exec_word(arg))
        .collect::<Vec<_>>()
        .join(" ");
    writeln!(service, "ExecStart={program} {args}")?;
    if service_definition.disable_restart_on_failure.not() {
        if config.restart != SystemdServiceRestartType::No {
            writeln!(service, "Restart={}", config.restart)?;
        }
        if let Some(restart_secs) = config.restart_sec {
            writeln!(service, "RestartSec={restart_secs}")?;
        }
    }
    if let Some(timeout_stop_sec) = config.timeout_stop_sec {
        writeln!(service, "TimeoutStopSec={timeout_stop_sec}")?;
    }
    Ok(service.trim().to_string())
}

/// One `ExecStart=` word, quoted so it reaches the program as a single argument.
///
/// Three layers read a command line, and all three are this generator's problem rather
/// than the manifest parser's, since only here is the syntax known:
///
/// - specifier expansion, applied to every unit setting: a literal `%` is written `%%`
///   (`systemd.unit(5)`)
/// - variable expansion, applied to command lines only: a literal `$` is written `$$`
///   (`systemd.service(5)`)
/// - item quoting: wrapping in `"` keeps whitespace inside one argument, and within those
///   quotes a backslash and a double quote must be escaped (`systemd.syntax(7)`)
///
/// The layers act on disjoint characters, so the substitutions cannot interfere.
fn escape_exec_word(word: &str) -> String {
    let mut escaped = String::with_capacity(word.len() + 2);
    escaped.push('"');
    for character in word.chars() {
        match character {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            '%' => escaped.push_str("%%"),
            '$' => escaped.push_str("$$"),
            _ => escaped.push(character),
        }
    }
    escaped.push('"');
    escaped
}

/// One `Environment=` assignment, quoted as a whole so a value may hold whitespace or an
/// equals sign.
///
/// `$` is deliberately left alone: `systemd.exec(5)` states that no variable expansion
/// happens here and the character has no special meaning, so doubling it would put a
/// literal `$$` into the plugin's environment. Specifier expansion still applies, so `%`
/// is still doubled.
fn escape_environment(name: &str, value: &str) -> String {
    let mut escaped = String::with_capacity(name.len() + value.len() + 4);
    escaped.push('"');
    escaped.push_str(name);
    escaped.push('=');
    for character in value.chars() {
        match character {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            '%' => escaped.push_str("%%"),
            _ => escaped.push(character),
        }
    }
    escaped.push('"');
    escaped
}

#[derive(Copy, Clone, Display, Debug, Default, PartialEq, Eq)]
// Variant names map onto systemd's Restart= values: OnSuccess -> on-success.
#[strum(serialize_all = "kebab-case")]
#[allow(dead_code)]
pub enum SystemdServiceRestartType {
    #[default]
    No,
    Always,
    OnSuccess,
    OnFailure,
    OnAbnormal,
    OnAbort,
    OnWatch,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_definition() -> ServiceDefinition {
        ServiceDefinition {
            service_id: "test-plugin".to_string(),
            executable: PathBuf::from("/usr/bin/test-plugin"),
            args: Vec::new(),
            username: Some(CC_PLUGIN_USER.to_string()),
            wrk_dir: None,
            envs: None,
            disable_restart_on_failure: false,
        }
    }

    fn exec_start_of(args: Vec<String>, envs: Option<Vec<(String, String)>>) -> String {
        let mut definition = base_definition();
        definition.args = args;
        definition.envs = envs;
        let unit = create_unit_file(&SystemdConfig::default(), &"Test".to_string(), definition)
            .expect("the unit file is written");
        unit.lines()
            .find(|line| line.starts_with("ExecStart="))
            .expect("ExecStart is present")
            .to_string()
    }

    /// Goal: the escaping contract for `ExecStart=`, taken from `systemd.unit(5)`,
    /// `systemd.service(5)` and `systemd.syntax(7)`. Each argument is one quoted item, a
    /// literal percent is doubled so specifier expansion leaves it alone, and a literal
    /// dollar is doubled so variable expansion does. Getting any of these wrong is how
    /// `--rate 50%` reached the plugin as `--rate 50`.
    #[test]
    fn exec_start_escapes_each_argument_as_one_item() {
        assert_eq!(
            exec_start_of(vec!["--rate".into(), "50%".into()], None),
            r#"ExecStart="/usr/bin/test-plugin" "--rate" "50%%""#
        );
        assert_eq!(
            exec_start_of(vec!["--name".into(), "My Device".into()], None),
            r#"ExecStart="/usr/bin/test-plugin" "--name" "My Device""#
        );
        assert_eq!(
            exec_start_of(vec!["--home".into(), "$HOME".into()], None),
            r#"ExecStart="/usr/bin/test-plugin" "--home" "$$HOME""#
        );
        assert_eq!(
            exec_start_of(vec![r#"--say "hi""#.into()], None),
            r#"ExecStart="/usr/bin/test-plugin" "--say \"hi\"""#
        );
        assert_eq!(
            exec_start_of(vec![r"--path=C:\tmp".into()], None),
            r#"ExecStart="/usr/bin/test-plugin" "--path=C:\\tmp""#
        );
    }

    /// Goal: characters the old allowlist deleted now survive into the unit file. These
    /// are the exact values from the bug report, and none of them may be altered.
    #[test]
    fn exec_start_preserves_previously_stripped_characters() {
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
            let exec = exec_start_of(vec![arg.to_string()], None);
            // Percent is the one character that is legitimately rewritten, as `%%`.
            let expected = arg.replace('%', "%%");
            assert!(
                exec.contains(&format!("\"{expected}\"")),
                "{arg} should survive, got {exec}"
            );
        }
    }

    /// Goal: the executable is quoted like any other word. A path with a space would
    /// otherwise become two arguments, and one starting with '-' would be read as an
    /// ExecStart prefix rather than a program.
    #[test]
    fn exec_start_quotes_the_program_path() {
        let mut definition = base_definition();
        definition.executable = PathBuf::from("/opt/my plugin/run");
        let unit =
            create_unit_file(&SystemdConfig::default(), &"Test".to_string(), definition).unwrap();
        assert!(unit.contains(r#"ExecStart="/opt/my plugin/run""#), "{unit}");
    }

    /// Goal: `Environment=` follows different rules from a command line.
    /// `systemd.exec(5)` states that no variable expansion happens there and `$` has no
    /// special meaning, so doubling it would put a literal `$$` in the plugin's
    /// environment. Specifier expansion still applies, so `%` is still doubled.
    #[test]
    fn environment_doubles_percent_but_leaves_dollar_alone() {
        let unit = create_unit_file(&SystemdConfig::default(), &"Test".to_string(), {
            let mut definition = base_definition();
            definition.envs = Some(vec![
                ("FMT".into(), "%Y-%m-%d".into()),
                ("GREETING".into(), "hello world".into()),
                ("LITERAL".into(), "$NOT_EXPANDED".into()),
            ]);
            definition
        })
        .unwrap();
        assert!(unit.contains(r#"Environment="FMT=%%Y-%%m-%%d""#), "{unit}");
        assert!(
            unit.contains(r#"Environment="GREETING=hello world""#),
            "{unit}"
        );
        assert!(
            unit.contains(r#"Environment="LITERAL=$NOT_EXPANDED""#),
            "{unit}"
        );
    }

    /// Goal: a quote in an argument must not end the item and let the rest of the value
    /// be read as further arguments, which is the injection the old allowlist existed to
    /// stop. Quoting closes it without deleting anything.
    #[test]
    fn exec_start_cannot_be_escaped_by_a_quote() {
        let exec = exec_start_of(vec![r#"a" "ExecStart=/bin/evil"#.into()], None);
        // The payload stays inside one item: its quotes are escaped, so it cannot close
        // the item and be read as a second command.
        assert!(exec.contains(r#""a\" \"ExecStart=/bin/evil""#), "{exec}");
        assert_eq!(exec.matches("ExecStart=").count(), 2, "{exec}");
        // Every unescaped quote pairs up, so the item cannot be broken open.
        let unescaped = exec
            .char_indices()
            .filter(|&(index, c)| c == '"' && (index == 0 || exec.as_bytes()[index - 1] != b'\\'))
            .count();
        assert_eq!(unescaped % 2, 0, "{exec}");
    }

    /// Goal: an unprivileged plugin must not be able to climb back out through a setuid binary
    /// or a file capability, since running it as its own user is the only thing containing it.
    /// Methodology: the default definition carries a username, which is the unprivileged case.
    #[test]
    fn unit_file_blocks_privilege_escalation_for_an_unprivileged_plugin() {
        let unit = create_unit_file(
            &SystemdConfig::default(),
            &"Test".to_string(),
            base_definition(),
        )
        .expect("unit must render");

        assert!(unit.contains("NoNewPrivileges=true"), "{unit}");
    }

    /// Goal: a plugin the user deliberately gave root must keep working. The directive would add
    /// nothing there (it is already root) but would break a setuid or capability helper it calls.
    /// Methodology: a privileged plugin is expressed by the absence of a username, which is also
    /// what makes systemd default the unit to root.
    #[test]
    fn unit_file_does_not_restrict_a_privileged_plugin() {
        let mut definition = base_definition();
        definition.username = None;

        let unit = create_unit_file(&SystemdConfig::default(), &"Test".to_string(), definition)
            .expect("unit must render");

        assert!(unit.contains("NoNewPrivileges").not(), "{unit}");
        assert!(
            unit.contains("User=").not(),
            "a privileged plugin runs as root: {unit}"
        );
    }

    /// Goal: pin the exact strings written to `Restart=` in a generated unit
    /// file. systemd rejects the unit outright if these drift, and the Display
    /// impl is derived, so nothing else would catch a rename.
    #[test]
    fn restart_type_renders_systemd_values() {
        let cases = [
            (SystemdServiceRestartType::No, "no"),
            (SystemdServiceRestartType::Always, "always"),
            (SystemdServiceRestartType::OnSuccess, "on-success"),
            (SystemdServiceRestartType::OnFailure, "on-failure"),
            (SystemdServiceRestartType::OnAbnormal, "on-abnormal"),
            (SystemdServiceRestartType::OnAbort, "on-abort"),
            (SystemdServiceRestartType::OnWatch, "on-watch"),
        ];
        for (restart_type, expected) in cases {
            assert_eq!(restart_type.to_string(), expected);
        }
    }

    #[test]
    fn restart_type_defaults_to_no() {
        assert_eq!(SystemdServiceRestartType::default().to_string(), "no");
    }
}
