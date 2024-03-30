/*
 * CoolerControl - monitor and control your cooling and other devices
 * Copyright (c) 2021-2024  Guy Boldon, Eren Simsek and contributors
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

use std::path::Path;

use crate::config::DEFAULT_CONFIG_DIR;
use anyhow::Result;
use const_format::concatcp;
use log::error;
use sha2::{Digest, Sha512};
use std::fs::Permissions;
use std::os::unix::fs::PermissionsExt;

const PASSWD_FILE_PATH: &str = concatcp!(DEFAULT_CONFIG_DIR, "/.passwd");
const DEFAULT_PASS: &str = "coolAdmin";
const DEFAULT_PERMISSIONS: u32 = 0o600;

pub async fn passwd_matches(passwd: &str) -> bool {
    match load_passwd().await {
        Ok(stored_passwd) => stored_passwd == hash_passwd(passwd.as_bytes()).unwrap_or_default(),
        Err(err) => {
            error!("Error loading password: {}", err);
            false
        }
    }
}

pub async fn load_passwd() -> Result<String> {
    let passwd_path = Path::new(PASSWD_FILE_PATH);
    if passwd_path.exists() {
        if let Ok(contents) = tokio::fs::read_to_string(passwd_path).await {
            tokio::fs::set_permissions(passwd_path, Permissions::from_mode(DEFAULT_PERMISSIONS))
                .await?;
            return Ok(contents.trim().to_owned());
        };
    }
    let passwd = hash_passwd(DEFAULT_PASS.as_bytes())?;
    let _ = tokio::fs::remove_file(passwd_path).await;
    tokio::fs::write(passwd_path, passwd.clone()).await?;
    tokio::fs::set_permissions(passwd_path, Permissions::from_mode(DEFAULT_PERMISSIONS)).await?;
    Ok(passwd)
}

pub async fn save_passwd(password: &str) -> Result<()> {
    let passwd_path = Path::new(PASSWD_FILE_PATH);
    let passwd = hash_passwd(password.as_bytes())?;
    let _ = tokio::fs::remove_file(passwd_path).await;
    tokio::fs::write(passwd_path, passwd).await?;
    tokio::fs::set_permissions(passwd_path, Permissions::from_mode(DEFAULT_PERMISSIONS)).await?;
    Ok(())
}

pub async fn reset_passwd() -> Result<()> {
    let passwd_path = Path::new(PASSWD_FILE_PATH);
    let passwd = hash_passwd(DEFAULT_PASS.as_bytes())?;
    let _ = tokio::fs::remove_file(passwd_path).await;
    tokio::fs::write(passwd_path, passwd).await?;
    tokio::fs::set_permissions(passwd_path, Permissions::from_mode(DEFAULT_PERMISSIONS)).await?;
    Ok(())
}

fn hash_passwd(passwd: &[u8]) -> Result<String> {
    let mut hasher = Sha512::new();
    hasher.update(String::from_utf8(passwd.to_owned())?);
    Ok(format!("{:x}", hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;

    // Returns a hashed password string for valid input.
    #[test]
    fn test_valid_input() {
        let input = b"password";
        let expected = "b109f3bbbc244eb82441917ed06d618b9008dd09b3befd1b5e07394c706a8bb980b1d7785e5976ec049b46df5f1326af5a2ea6d103fd07c95385ffab0cacbc86";
        let result = hash_passwd(input).unwrap();
        assert_eq!(result, expected);
    }

    // Returns a different hashed password string for different input.
    #[test]
    fn test_different_input() {
        let input1 = b"password1";
        let input2 = b"password2";
        let result1 = hash_passwd(input1).unwrap();
        let result2 = hash_passwd(input2).unwrap();
        assert_ne!(result1, result2);
    }

    // Handles empty input by returning an empty string.
    #[test]
    fn test_empty_input() {
        let input = b"";
        let expected = "cf83e1357eefb8bdf1542850d66d8007d620e4050b5715dc83f4a921d36ce9ce47d0d13c5d85f2b0ff8318d2877eec2f63b931bd47417a81a538327af927da3e";
        let result = hash_passwd(input).unwrap();
        assert_eq!(result, expected);
    }
}
