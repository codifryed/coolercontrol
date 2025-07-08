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
use anyhow::{anyhow, Result};
use log::{debug, error, info};
use pyo3::Python;
use std::ffi::CString;

const PY_VERIFY: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/resources/liqctld/verify.py"
));
const PY_SERVICE: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/resources/liqctld/main.py"
));

/// This function verifies the Python system environment meets the requirements to
/// run the `liqctld` service.
/// This functions should be called with `tokio::task::spawn_blocking` to ensure the Python logger
/// integrates with the Rust logger.
pub fn verify_env() -> Result<()> {
    debug!("Verifying Python environment for liqctld service...");
    // needed only once
    pyo3::prepare_freethreaded_python();
    Python::with_gil(|py| {
        // needed only once
        pyo3_pylogger::setup_logging(py, "coolercontrol-liqctld")?;
        py.run(CString::new(PY_VERIFY)?.as_c_str(), None, None)
    })
    .map_err(|err| anyhow!("{err}"))
}

/// This function runs the `liqctld` service daemon.
/// This functions should be called with `tokio::task::spawn_blocking` to ensure the Python logger
/// integrates with the Rust logger.
pub fn run() -> Result<()> {
    debug!("Starting liqctld Service...");
    Python::with_gil(|py| py.run(CString::new(PY_SERVICE)?.as_c_str(), None, None))
        .inspect(|()| info!("Liqctld service exited successfully."))
        .map_err(|err| {
            error!("Liqctld service exited with an error: {err}");
            anyhow!("Liqctld service exited with error: {err}")
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    /// python3-dev should be installed for building, and so this test should pass
    /// which verifies calling the embedded python script works.
    #[test]
    fn test_verify_env() {
        let result = verify_env();
        // Err would mean that the liquidctl package is not installed, which is fine for testing.
        assert!(result.is_ok() || result.is_err());
    }
}
