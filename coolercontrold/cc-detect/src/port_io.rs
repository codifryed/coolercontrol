/*
 * CoolerControl - monitor and control your cooling and other devices
 * Copyright (c) 2026  Guy Boldon, megadjc and contributors
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

//! I/O port access abstraction for Super-I/O chip probing.
//!
//! Provides the [`PortIo`] trait for reading/writing x86 I/O ports,
//! with a real implementation using `/dev/port` and a mock for testing.

use std::fmt;
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};

/// Error type for port I/O operations.
#[derive(Debug)]
pub enum PortIoError {
    /// The `/dev/port` device could not be opened (permission denied, not found, etc.)
    OpenFailed(std::io::Error),
    /// A seek/read/write operation on the port failed.
    IoFailed(std::io::Error),
}

impl fmt::Display for PortIoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OpenFailed(e) => write!(f, "failed to open /dev/port: {e}"),
            Self::IoFailed(e) => write!(f, "port I/O failed: {e}"),
        }
    }
}

impl std::error::Error for PortIoError {}

/// Trait for I/O port access - enables mocking in tests.
pub trait PortIo {
    /// Read a byte from the given I/O port.
    ///
    /// # Errors
    ///
    /// * [`PortIoError::OpenFailed`]: The `/dev/port` device could not be opened.
    /// * [`PortIoError::IoFailed`]: A seek/read/write operation on the port failed.
    fn inb(&mut self, port: u16) -> Result<u8, PortIoError>;

    /// Write a byte to the given I/O port.
    ///
    /// # Errors
    ///
    /// * [`PortIoError::IoFailed`]: A seek/read/write operation on the port failed.
    fn outb(&mut self, port: u16, value: u8) -> Result<(), PortIoError>;
}

/// Real implementation using `/dev/port` for raw x86 I/O port access.
#[cfg(target_arch = "x86_64")]
pub struct DevPort {
    file: File,
}

#[cfg(target_arch = "x86_64")]
impl DevPort {
    /// Open `/dev/port` for reading and writing.
    ///
    /// # Errors
    ///
    /// * [`PortIoError::OpenFailed`]: The `/dev/port` device could not be opened.
    pub fn open() -> Result<Self, PortIoError> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open("/dev/port")
            .map_err(PortIoError::OpenFailed)?;
        Ok(Self { file })
    }
}

#[cfg(target_arch = "x86_64")]
impl PortIo for DevPort {
    fn inb(&mut self, port: u16) -> Result<u8, PortIoError> {
        self.file
            .seek(SeekFrom::Start(u64::from(port)))
            .map_err(PortIoError::IoFailed)?;
        let mut buf = [0u8; 1];
        self.file
            .read_exact(&mut buf)
            .map_err(PortIoError::IoFailed)?;
        Ok(buf[0])
    }

    fn outb(&mut self, port: u16, value: u8) -> Result<(), PortIoError> {
        self.file
            .seek(SeekFrom::Start(u64::from(port)))
            .map_err(PortIoError::IoFailed)?;
        self.file.write_all(&[value]).map_err(PortIoError::IoFailed)
    }
}

/// Mock implementation for testing - records writes and replays pre-configured reads.
#[cfg(test)]
pub struct MockPortIo {
    /// Sequence of values to return for each successive `inb` call.
    read_sequence: Vec<u8>,
    read_index: usize,
    /// Records all writes: (port, value).
    pub writes: Vec<(u16, u8)>,
}

#[cfg(test)]
impl MockPortIo {
    /// Create a new mock with a sequence of read values returned for each `inb` call.
    #[must_use]
    pub fn new(read_sequence: Vec<u8>) -> Self {
        Self {
            read_sequence,
            read_index: 0,
            writes: Vec::new(),
        }
    }
}

#[cfg(test)]
impl PortIo for MockPortIo {
    fn inb(&mut self, _port: u16) -> Result<u8, PortIoError> {
        if self.read_index < self.read_sequence.len() {
            let value = self.read_sequence[self.read_index];
            self.read_index += 1;
            return Ok(value);
        }
        Ok(0xFF)
    }

    fn outb(&mut self, port: u16, value: u8) -> Result<(), PortIoError> {
        self.writes.push((port, value));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_port_io_sequence() {
        let mut mock = MockPortIo::new(vec![0x87, 0x12]);
        assert_eq!(mock.inb(0x2E).unwrap(), 0x87);
        assert_eq!(mock.inb(0x2F).unwrap(), 0x12);
        // Past end of sequence returns 0xFF
        assert_eq!(mock.inb(0x2E).unwrap(), 0xFF);
    }

    #[test]
    fn test_mock_port_io_writes() {
        let mut mock = MockPortIo::new(vec![]);
        mock.outb(0x2E, 0x20).unwrap();
        mock.outb(0x2E, 0x07).unwrap();
        assert_eq!(mock.writes, vec![(0x2E, 0x20), (0x2E, 0x07)]);
    }

    #[cfg(target_arch = "x86_64")]
    #[test]
    fn test_dev_port_error_handling() {
        // Without root permissions, opening /dev/port should fail.
        // This test may pass or fail depending on the test environment.
        let result = DevPort::open();
        // We just verify it doesn't panic - the result depends on permissions
        let _ = result;
    }
}
