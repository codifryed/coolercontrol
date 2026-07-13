/*
 * CoolerControl - monitor and control your cooling and other devices
 * Copyright (c) 2021-2025  Guy Boldon, Eren Simsek and contributors
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation, either version 3 of the License, or
 * (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

//! Validates the daemon's config files by parsing each into its owning module's
//! type. Shared by the `check` command (live config) and by restore (the backup
//! being applied), so a hand-edited or restored file that would brick startup is
//! caught before it can.
//!
//! Two files have no daemon-side struct (`config-ui.json`'s schema lives in the
//! UI, `detect.toml`'s parser lives in `cc-detect` and swallows errors), and
//! neither can fail daemon startup, so they get a well-formedness check only.

use std::path::Path;

use crate::cc_fs;

/// Config files a check knows how to validate, in display order.
pub const CONFIG_FILE_NAMES: [&str; 7] = [
    "config.toml",
    "config-ui.json",
    "alerts.json",
    "modes.json",
    "calibrations.json",
    "overrides.toml",
    "detect.toml",
];

/// The validation outcome for a single file.
pub struct FileReport {
    pub name: String,
    /// `None` when valid; the failure message otherwise.
    pub error: Option<String>,
}

/// Validates every known config file present in `dir`. Missing files are skipped
/// (absence is valid: the feature simply has no config). Returns one report per
/// present file, in `CONFIG_FILE_NAMES` order.
pub async fn check_dir(dir: &Path) -> Vec<FileReport> {
    let mut reports = Vec::with_capacity(CONFIG_FILE_NAMES.len());
    for name in CONFIG_FILE_NAMES {
        let path = dir.join(name);
        if !cc_fs::exists(&path) {
            continue;
        }
        let error = match cc_fs::read_txt(&path).await {
            Ok(contents) => validate_named(name, &contents)
                .await
                .err()
                .map(|err| format!("{err:#}")),
            Err(err) => Some(format!("unreadable: {err:#}")),
        };
        reports.push(FileReport {
            name: name.to_string(),
            error,
        });
    }
    reports
}

/// True when no present file failed validation.
pub fn all_valid(reports: &[FileReport]) -> bool {
    reports.iter().all(|report| report.error.is_none())
}

/// Dispatches a file to its owning module's validator by name.
async fn validate_named(name: &str, contents: &str) -> anyhow::Result<()> {
    match name {
        "config.toml" => crate::config::Config::validate_str(contents).await,
        "alerts.json" => crate::alerts::validate(contents),
        "modes.json" => crate::modes::validate(contents),
        "calibrations.json" => crate::calibration::validate(contents),
        "overrides.toml" => crate::overrides::validate(contents),
        // No daemon struct; a syntax check is the strongest bar available.
        "config-ui.json" => validate_well_formed_json(contents),
        "detect.toml" => validate_well_formed_toml(contents),
        _ => Ok(()),
    }
}

fn validate_well_formed_json(contents: &str) -> anyhow::Result<()> {
    serde_json::from_str::<serde_json::Value>(contents)
        .map(|_| ())
        .map_err(|err| anyhow::anyhow!("Not well-formed JSON: {err}"))
}

fn validate_well_formed_toml(contents: &str) -> anyhow::Result<()> {
    contents
        .parse::<toml_edit::DocumentMut>()
        .map(|_| ())
        .map_err(|err| anyhow::anyhow!("Not well-formed TOML: {err}"))
}

/// Validates the live config directory, printing one line per file. Returns
/// whether every present file is valid (the process exit status keys off this).
pub async fn run_check() -> bool {
    let reports = check_dir(crate::paths::config_dir()).await;
    if reports.is_empty() {
        println!("No configuration files found to check.");
        return true;
    }
    for report in &reports {
        match &report.error {
            None => println!("  ok      {}", report.name),
            Some(msg) => println!("  INVALID {} : {msg}", report.name),
        }
    }
    all_valid(&reports)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(dir: &Path, name: &str, contents: &str) {
        std::fs::write(dir.join(name), contents).unwrap();
    }

    /// Goal: a directory of valid files reports no errors, and each kind of
    /// corruption (bad struct data, malformed JSON, malformed TOML) is flagged on
    /// exactly the offending file while others stay ok. Method: write a mix of
    /// valid and invalid files and assert per-file outcomes.
    #[test]
    fn check_dir_flags_only_invalid_files() {
        crate::rt::test_runtime(async {
            let tmp = tempfile::tempdir().unwrap();
            let dir = tmp.path();
            // Valid files.
            write(dir, "alerts.json", "{\"alerts\":[]}");
            write(dir, "modes.json", "{\"modes\":[],\"order\":[]}");
            write(dir, "overrides.toml", "");
            write(dir, "config-ui.json", "{\"any\":true}");
            // Invalid files.
            write(dir, "calibrations.json", "{ not json");
            write(dir, "detect.toml", "this is = = not toml");

            let reports = check_dir(dir).await;
            let by_name = |n: &str| reports.iter().find(|r| r.name == n).unwrap();

            assert!(by_name("alerts.json").error.is_none());
            assert!(by_name("modes.json").error.is_none());
            assert!(by_name("overrides.toml").error.is_none());
            assert!(by_name("config-ui.json").error.is_none());
            assert!(by_name("calibrations.json").error.is_some());
            assert!(by_name("detect.toml").error.is_some());
            assert!(!all_valid(&reports));
        });
    }

    /// Goal: files absent from the directory are simply not reported (absence is
    /// valid). Method: check an empty directory and assert no reports, all valid.
    #[test]
    fn check_dir_skips_missing_files() {
        crate::rt::test_runtime(async {
            let tmp = tempfile::tempdir().unwrap();
            let reports = check_dir(tmp.path()).await;
            assert!(reports.is_empty());
            assert!(all_valid(&reports));
        });
    }
}
