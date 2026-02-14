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
 *
 * MokaSessionStore based on tower-sessions-moka-store by Max Countryman
 * Copyright (c) 2024 Max Countryman
 * Licensed under the MIT License
 * https://github.com/maxcountryman/tower-sessions-stores/tree/main/moka-store
 *
 * FileSessionStore based on tower-sessions-file-store by Silas
 * Copyright (c) 2024 Silas
 * Licensed under the MIT License
 * https://github.com/nyabinary/tower-sessions-file-store
 */

use std::fs::Permissions;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::str::FromStr;
use std::time::{Duration as StdDuration, Instant as StdInstant};

use crate::cc_fs;
use async_trait::async_trait;
use moka::future::Cache;
use moka::policy::EvictionPolicy;
use moka::Expiry;
use time::OffsetDateTime;
use tower_sessions::session::{Id, Record};
use tower_sessions::session_store;
use tower_sessions::{ExpiredDeletion, SessionStore};

const SESSION_DIR_PERMISSIONS: u32 = 0o700;
const SESSION_FILE_PERMISSIONS: u32 = 0o600;

/// A custom session store wrapping moka's async cache.
///
/// This is similar to the official `MokaStore` from `tower-sessions-moka-store`,
/// except that `delete()` calls `invalidate_all()` instead of invalidating only
/// the target session. Since `CoolerControl` is a single-user system, deleting any
/// session (e.g., on password change) should invalidate all cached sessions.
#[derive(Debug, Clone)]
pub struct MokaSessionStore {
    cache: Cache<Id, Record>,
}

impl MokaSessionStore {
    pub fn new(max_capacity: Option<u64>) -> Self {
        let builder = match max_capacity {
            Some(cap) => Cache::builder().max_capacity(cap),
            None => Cache::builder(),
        }
        .eviction_policy(EvictionPolicy::lru())
        .expire_after(SessionExpiry);
        Self {
            cache: builder.build(),
        }
    }
}

#[async_trait]
impl SessionStore for MokaSessionStore {
    async fn create(&self, record: &mut Record) -> session_store::Result<()> {
        while self.cache.contains_key(&record.id) {
            record.id = Id::default();
        }
        self.cache.insert(record.id, record.clone()).await;
        Ok(())
    }

    async fn save(&self, record: &Record) -> session_store::Result<()> {
        self.cache.insert(record.id, record.clone()).await;
        Ok(())
    }

    async fn load(&self, session_id: &Id) -> session_store::Result<Option<Record>> {
        Ok(self.cache.get(session_id).await)
    }

    async fn delete(&self, _session_id: &Id) -> session_store::Result<()> {
        // Since CoolerControl has only a single user, deleting any session
        // invalidates all cached sessions. This ensures that on password change,
        // all sessions are cleared from the cache layer.
        self.cache.invalidate_all();
        Ok(())
    }
}

/// Per-entry expiry policy based on each session record's `expiry_date`.
struct SessionExpiry;

impl SessionExpiry {
    fn expiry_date_to_duration(expiry_date: OffsetDateTime) -> StdDuration {
        let now = OffsetDateTime::now_utc();
        if expiry_date > now {
            (expiry_date - now).try_into().unwrap_or(StdDuration::ZERO)
        } else {
            StdDuration::ZERO
        }
    }
}

impl Expiry<Id, Record> for SessionExpiry {
    fn expire_after_create(
        &self,
        _key: &Id,
        value: &Record,
        _current_time: StdInstant,
    ) -> Option<StdDuration> {
        Some(Self::expiry_date_to_duration(value.expiry_date))
    }

    fn expire_after_update(
        &self,
        _key: &Id,
        value: &Record,
        _current_time: StdInstant,
        _current_duration: Option<StdDuration>,
    ) -> Option<StdDuration> {
        Some(Self::expiry_date_to_duration(value.expiry_date))
    }
}

/// A custom file-based session store that persists sessions as JSON files.
///
/// Similar to the upstream `tower-sessions-file-store`, but simplified for our
/// single-user use case:
/// - `delete()` removes all session files (not just the target session)
/// - No `minimum_expiry_date` filtering in `delete_expired()`
/// - No `Cow` path wrapper
#[derive(Debug, Clone)]
pub struct FileSessionStore {
    folder: PathBuf,
}

impl FileSessionStore {
    pub fn new(folder: PathBuf) -> Self {
        Self { folder }
    }

    /// Deletes all session files from the sessions directory.
    async fn delete_all_files(&self) {
        if let Ok(entries) = cc_fs::read_dir(&self.folder) {
            for entry in entries.flatten() {
                let _ = cc_fs::remove_file(entry.path()).await;
            }
        }
    }
}

#[async_trait]
impl SessionStore for FileSessionStore {
    async fn create(&self, record: &mut Record) -> session_store::Result<()> {
        cc_fs::create_dir_all(&self.folder)
            .await
            .map_err(from_anyhow_to_backend)?;
        cc_fs::set_permissions(
            &self.folder,
            Permissions::from_mode(SESSION_DIR_PERMISSIONS),
        )
        .await
        .map_err(from_anyhow_to_backend)?;
        let data = serde_json::to_vec(&record).map_err(from_serde_to_backend)?;
        let file_path = self.folder.join(record.id.to_string());
        cc_fs::write(&file_path, data)
            .await
            .map_err(from_anyhow_to_backend)?;
        cc_fs::set_permissions(&file_path, Permissions::from_mode(SESSION_FILE_PERMISSIONS))
            .await
            .map_err(from_anyhow_to_backend)?;
        Ok(())
    }

    async fn save(&self, record: &Record) -> session_store::Result<()> {
        let data = serde_json::to_vec(record).map_err(from_serde_to_backend)?;
        let file_path = self.folder.join(record.id.to_string());
        cc_fs::write(&file_path, data)
            .await
            .map_err(from_anyhow_to_backend)?;
        cc_fs::set_permissions(&file_path, Permissions::from_mode(SESSION_FILE_PERMISSIONS))
            .await
            .map_err(from_anyhow_to_backend)?;
        Ok(())
    }

    async fn load(&self, session_id: &Id) -> session_store::Result<Option<Record>> {
        let path = self.folder.join(session_id.to_string());
        match cc_fs::read_txt(&path).await {
            Ok(data) => {
                let record = serde_json::from_str(&data).map_err(from_serde_to_backend)?;
                Ok(record)
            }
            Err(e)
                if e.downcast_ref::<std::io::Error>()
                    .is_some_and(|io| io.kind() == std::io::ErrorKind::NotFound) =>
            {
                Ok(None)
            }
            Err(e) => Err(from_anyhow_to_backend(e)),
        }
    }

    async fn delete(&self, _session_id: &Id) -> session_store::Result<()> {
        // Since CoolerControl has only a single user, deleting any session
        // removes all persisted session files. This ensures that on password
        // change, all sessions are cleared from the file-store layer.
        self.delete_all_files().await;
        Ok(())
    }
}

#[async_trait]
impl ExpiredDeletion for FileSessionStore {
    async fn delete_expired(&self) -> session_store::Result<()> {
        let mut entries = tokio::fs::read_dir(&self.folder)
            .await
            .map_err(from_std_to_backend)?;
        while let Some(entry) = entries.next_entry().await.map_err(from_std_to_backend)? {
            let Some(session_id) = entry
                .file_name()
                .to_str()
                .and_then(|s| Id::from_str(s).ok())
            else {
                continue;
            };
            let Some(record) = self.load(&session_id).await? else {
                continue;
            };
            if OffsetDateTime::now_utc() > record.expiry_date {
                let _ = cc_fs::remove_file(entry.path()).await;
            }
        }
        Ok(())
    }
}

#[allow(clippy::needless_pass_by_value)]
fn from_std_to_backend(err: std::io::Error) -> session_store::Error {
    session_store::Error::Backend(err.to_string())
}

#[allow(clippy::needless_pass_by_value)]
fn from_serde_to_backend(err: serde_json::Error) -> session_store::Error {
    session_store::Error::Backend(err.to_string())
}

#[allow(clippy::needless_pass_by_value)]
fn from_anyhow_to_backend(err: anyhow::Error) -> session_store::Error {
    session_store::Error::Backend(err.to_string())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use time::Duration;

    fn create_test_record(id: Id) -> Record {
        Record {
            id,
            data: HashMap::default(),
            expiry_date: OffsetDateTime::now_utc() + Duration::hours(1),
        }
    }

    fn create_expired_record(id: Id) -> Record {
        Record {
            id,
            data: HashMap::default(),
            expiry_date: OffsetDateTime::now_utc() - Duration::hours(1),
        }
    }

    // MokaSessionStore tests

    #[tokio::test]
    async fn test_moka_create_and_load() {
        let store = MokaSessionStore::new(Some(100));
        let id = Id::default();
        let mut record = create_test_record(id);
        store.create(&mut record).await.unwrap();

        let loaded = store.load(&record.id).await.unwrap();
        assert!(loaded.is_some());
    }

    #[tokio::test]
    async fn test_moka_save_and_load() {
        let store = MokaSessionStore::new(Some(100));
        let id = Id::default();
        let record = create_test_record(id);
        store.save(&record).await.unwrap();

        let loaded = store.load(&id).await.unwrap();
        assert!(loaded.is_some());
    }

    #[tokio::test]
    async fn test_moka_load_nonexistent() {
        let store = MokaSessionStore::new(Some(100));
        let loaded = store.load(&Id::default()).await.unwrap();
        assert!(loaded.is_none());
    }

    #[tokio::test]
    async fn test_moka_delete_clears_all_entries() {
        let store = MokaSessionStore::new(Some(100));

        let id1 = Id::default();
        let id2 = Id::default();
        let mut record1 = create_test_record(id1);
        let mut record2 = create_test_record(id2);
        store.create(&mut record1).await.unwrap();
        store.create(&mut record2).await.unwrap();

        store.delete(&record1.id).await.unwrap();

        // run_pending ensures invalidation takes effect
        store.cache.run_pending_tasks().await;

        let loaded1 = store.load(&record1.id).await.unwrap();
        let loaded2 = store.load(&record2.id).await.unwrap();
        assert!(loaded1.is_none(), "Session 1 should be invalidated");
        assert!(loaded2.is_none(), "Session 2 should be invalidated");
    }

    #[tokio::test]
    async fn test_moka_create_avoids_id_collision() {
        let store = MokaSessionStore::new(Some(100));
        let id = Id::default();
        let mut record1 = create_test_record(id);
        store.create(&mut record1).await.unwrap();

        let mut record2 = create_test_record(id);
        store.create(&mut record2).await.unwrap();

        assert_ne!(record2.id, id);
    }

    // FileSessionStore tests

    #[tokio::test]
    async fn test_file_create_and_load() {
        let dir = tempfile::tempdir().unwrap();
        let store = FileSessionStore::new(dir.path().to_path_buf());
        let id = Id::default();
        let mut record = create_test_record(id);
        store.create(&mut record).await.unwrap();

        let loaded = store.load(&record.id).await.unwrap();
        assert!(loaded.is_some());
        assert_eq!(loaded.unwrap().id, record.id);
    }

    #[tokio::test]
    async fn test_file_save_and_load() {
        let dir = tempfile::tempdir().unwrap();
        let store = FileSessionStore::new(dir.path().to_path_buf());
        let id = Id::default();
        let mut record = create_test_record(id);
        store.create(&mut record).await.unwrap();

        // Update the record via save
        store.save(&record).await.unwrap();

        let loaded = store.load(&id).await.unwrap();
        assert!(loaded.is_some());
    }

    #[tokio::test]
    async fn test_file_delete_clears_all_files() {
        let dir = tempfile::tempdir().unwrap();
        let store = FileSessionStore::new(dir.path().to_path_buf());

        let mut record1 = create_test_record(Id::default());
        let mut record2 = create_test_record(Id::default());
        store.create(&mut record1).await.unwrap();
        store.create(&mut record2).await.unwrap();

        // Delete using only one session ID — should clear all files
        store.delete(&record1.id).await.unwrap();

        let loaded1 = store.load(&record1.id).await.unwrap();
        let loaded2 = store.load(&record2.id).await.unwrap();
        assert!(loaded1.is_none(), "Session 1 file should be deleted");
        assert!(loaded2.is_none(), "Session 2 file should be deleted");
    }

    #[tokio::test]
    async fn test_file_load_nonexistent() {
        let dir = tempfile::tempdir().unwrap();
        let store = FileSessionStore::new(dir.path().to_path_buf());
        let loaded = store.load(&Id::default()).await.unwrap();
        assert!(loaded.is_none());
    }

    #[tokio::test]
    async fn test_file_delete_expired() {
        let dir = tempfile::tempdir().unwrap();
        let store = FileSessionStore::new(dir.path().to_path_buf());

        // Create one expired and one valid session
        let mut expired = create_expired_record(Id::default());
        let mut valid = create_test_record(Id::default());
        store.create(&mut expired).await.unwrap();
        store.create(&mut valid).await.unwrap();

        store.delete_expired().await.unwrap();

        let loaded_expired = store.load(&expired.id).await.unwrap();
        let loaded_valid = store.load(&valid.id).await.unwrap();
        assert!(
            loaded_expired.is_none(),
            "Expired session should be removed"
        );
        assert!(loaded_valid.is_some(), "Valid session should remain");
    }

    #[tokio::test]
    async fn test_file_permissions() {
        let dir = tempfile::tempdir().unwrap();
        let session_dir = dir.path().join("sessions");
        let store = FileSessionStore::new(session_dir.clone());

        let mut record = create_test_record(Id::default());
        store.create(&mut record).await.unwrap();

        // Directory should have 700 permissions
        let dir_perms = std::fs::metadata(&session_dir).unwrap().permissions();
        assert_eq!(
            dir_perms.mode() & 0o777,
            SESSION_DIR_PERMISSIONS,
            "Session directory should have 700 permissions"
        );

        // File should have 600 permissions
        let file_path = session_dir.join(record.id.to_string());
        let file_perms = std::fs::metadata(&file_path).unwrap().permissions();
        assert_eq!(
            file_perms.mode() & 0o777,
            SESSION_FILE_PERMISSIONS,
            "Session file should have 600 permissions"
        );

        // Permissions should be maintained after save
        store.save(&record).await.unwrap();
        let file_perms = std::fs::metadata(&file_path).unwrap().permissions();
        assert_eq!(
            file_perms.mode() & 0o777,
            SESSION_FILE_PERMISSIONS,
            "Session file should retain 600 permissions after save"
        );
    }
}
