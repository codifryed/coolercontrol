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
 * FileSessionStore based on tower-sessions-file-store by Silas
 * Copyright (c) 2024 Silas
 * Licensed under the MIT License
 * https://github.com/nyabinary/tower-sessions-file-store
 */

use std::fs::Permissions;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Mutex;

use crate::{admin, cc_fs};
use async_trait::async_trait;
use indexmap::IndexMap;
use time::OffsetDateTime;
use tower_sessions::session::{Id, Record};
use tower_sessions::session_store;
use tower_sessions::{ExpiredDeletion, SessionStore};

const SESSION_DIR_PERMISSIONS: u32 = 0o700;
const SESSION_FILE_PERMISSIONS: u32 = 0o600;

/// In-memory LRU session cache with expiry-based eviction.
///
/// Uses an `IndexMap` to maintain insertion/access order — the back is the most-recently-used
/// entry, the front is eviction candidates. Expired entries are lazily pruned on `load`.
///
/// `delete()` clears all entries because `CoolerControl` is a single-user
/// system; any session deletion (e.g., password change) must invalidate
/// every cached session.
#[derive(Debug, Clone)]
pub struct MemorySessionStore {
    // Mutex satisfies the Send + Sync requirement of SessionStore.
    // The API servers run on a single-threaded tokio runtime, so there
    // is no actual lock contention.
    entries: std::sync::Arc<Mutex<IndexMap<Id, Record>>>,
    max_capacity: usize,
}

impl MemorySessionStore {
    pub fn new(max_capacity: usize) -> Self {
        assert!(max_capacity > 0, "Session cache capacity must be > 0.");
        Self {
            entries: std::sync::Arc::new(Mutex::new(IndexMap::with_capacity(max_capacity))),
            max_capacity,
        }
    }

    /// Moves `id` to the back (most-recently-used position) if present.
    fn touch(map: &mut IndexMap<Id, Record>, id: &Id) {
        if let Some(index) = map.get_index_of(id) {
            map.move_index(index, map.len().saturating_sub(1));
        }
    }

    /// Evicts the least-recently-used entry (front) when at capacity.
    fn evict_lru_if_full(map: &mut IndexMap<Id, Record>, max_capacity: usize) {
        debug_assert!(max_capacity > 0);
        while map.len() > max_capacity {
            map.shift_remove_index(0);
        }
    }
}

#[async_trait]
impl SessionStore for MemorySessionStore {
    async fn create(&self, record: &mut Record) -> session_store::Result<()> {
        let mut map = self.entries.lock().expect("session cache lock poisoned");
        // Avoid ID collision by generating a new ID until unique.
        while map.contains_key(&record.id) {
            record.id = Id::default();
        }
        map.insert(record.id, record.clone());
        Self::evict_lru_if_full(&mut map, self.max_capacity);
        Ok(())
    }

    async fn save(&self, record: &Record) -> session_store::Result<()> {
        let mut map = self.entries.lock().expect("session cache lock poisoned");
        map.insert(record.id, record.clone());
        // Move to back so the updated entry is treated as most-recently-used.
        Self::touch(&mut map, &record.id);
        Self::evict_lru_if_full(&mut map, self.max_capacity);
        Ok(())
    }

    async fn load(&self, session_id: &Id) -> session_store::Result<Option<Record>> {
        let mut map = self.entries.lock().expect("session cache lock poisoned");
        let Some(record) = map.get(session_id).cloned() else {
            return Ok(None);
        };
        // Lazily evict expired entries on access.
        if record.expiry_date <= OffsetDateTime::now_utc() {
            map.shift_remove(session_id);
            return Ok(None);
        }
        Self::touch(&mut map, session_id);
        Ok(Some(record))
    }

    async fn delete(&self, _session_id: &Id) -> session_store::Result<()> {
        // Since CoolerControl has only a single user, deleting any session
        // invalidates all cached sessions. This ensures that on password change,
        // all sessions are cleared from the cache layer.
        let mut map = self.entries.lock().expect("session cache lock poisoned");
        map.clear();
        Ok(())
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
        admin::clear_session_files(&self.folder).await;
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

    // MemorySessionStore tests

    #[tokio::test]
    async fn test_memory_create_and_load() {
        // Goal: verify that a created session can be loaded back.
        let store = MemorySessionStore::new(100);
        let id = Id::default();
        let mut record = create_test_record(id);
        store.create(&mut record).await.unwrap();

        let loaded = store.load(&record.id).await.unwrap();
        assert!(loaded.is_some());
        assert_eq!(loaded.unwrap().id, record.id);
    }

    #[tokio::test]
    async fn test_memory_save_and_load() {
        // Goal: verify that save inserts an entry that can be loaded.
        let store = MemorySessionStore::new(100);
        let id = Id::default();
        let record = create_test_record(id);
        store.save(&record).await.unwrap();

        let loaded = store.load(&id).await.unwrap();
        assert!(loaded.is_some());
    }

    #[tokio::test]
    async fn test_memory_load_nonexistent() {
        // Goal: verify that loading a missing ID returns None.
        let store = MemorySessionStore::new(100);
        let loaded = store.load(&Id::default()).await.unwrap();
        assert!(loaded.is_none());
    }

    #[tokio::test]
    async fn test_memory_delete_clears_all_entries() {
        // Goal: verify that delete() clears all entries, not just the
        // target session — matches single-user semantics.
        let store = MemorySessionStore::new(100);

        let mut record1 = create_test_record(Id::default());
        let mut record2 = create_test_record(Id::default());
        store.create(&mut record1).await.unwrap();
        store.create(&mut record2).await.unwrap();

        store.delete(&record1.id).await.unwrap();

        let loaded1 = store.load(&record1.id).await.unwrap();
        let loaded2 = store.load(&record2.id).await.unwrap();
        assert!(loaded1.is_none(), "Session 1 should be cleared.");
        assert!(loaded2.is_none(), "Session 2 should also be cleared.");
    }

    #[tokio::test]
    async fn test_memory_create_avoids_id_collision() {
        // Goal: verify that create() generates a new ID when the
        // original ID is already in use.
        let store = MemorySessionStore::new(100);
        let id = Id::default();
        let mut record1 = create_test_record(id);
        store.create(&mut record1).await.unwrap();

        let mut record2 = create_test_record(id);
        store.create(&mut record2).await.unwrap();

        assert_ne!(record2.id, id, "Collision should trigger a new ID.");
    }

    #[tokio::test]
    async fn test_memory_lru_eviction() {
        // Goal: verify that when max_capacity is reached, the least
        // recently used entry (front of IndexMap) is evicted.
        let store = MemorySessionStore::new(2);

        let mut r1 = create_test_record(Id::default());
        let mut r2 = create_test_record(Id::default());
        let mut r3 = create_test_record(Id::default());
        store.create(&mut r1).await.unwrap();
        store.create(&mut r2).await.unwrap();
        // r1 is LRU, should be evicted when r3 is inserted.
        store.create(&mut r3).await.unwrap();

        assert!(
            store.load(&r1.id).await.unwrap().is_none(),
            "LRU entry r1 should be evicted."
        );
        assert!(store.load(&r2.id).await.unwrap().is_some());
        assert!(store.load(&r3.id).await.unwrap().is_some());
    }

    #[tokio::test]
    async fn test_memory_lru_touch_on_load() {
        // Goal: verify that loading an entry moves it to the back,
        // protecting it from LRU eviction.
        let store = MemorySessionStore::new(2);

        let mut r1 = create_test_record(Id::default());
        let mut r2 = create_test_record(Id::default());
        store.create(&mut r1).await.unwrap();
        store.create(&mut r2).await.unwrap();

        // Touch r1, making r2 the new LRU.
        store.load(&r1.id).await.unwrap();

        let mut r3 = create_test_record(Id::default());
        store.create(&mut r3).await.unwrap();

        assert!(
            store.load(&r1.id).await.unwrap().is_some(),
            "r1 was touched, should survive eviction."
        );
        assert!(
            store.load(&r2.id).await.unwrap().is_none(),
            "r2 was LRU after r1 was touched, should be evicted."
        );
    }

    #[tokio::test]
    async fn test_memory_lru_touch_on_save() {
        // Goal: verify that save() moves the entry to the back,
        // protecting it from LRU eviction.
        let store = MemorySessionStore::new(2);

        let mut r1 = create_test_record(Id::default());
        let mut r2 = create_test_record(Id::default());
        store.create(&mut r1).await.unwrap();
        store.create(&mut r2).await.unwrap();

        // Save r1 again, making r2 the LRU.
        store.save(&r1).await.unwrap();

        let mut r3 = create_test_record(Id::default());
        store.create(&mut r3).await.unwrap();

        assert!(
            store.load(&r1.id).await.unwrap().is_some(),
            "r1 was saved, should survive eviction."
        );
        assert!(
            store.load(&r2.id).await.unwrap().is_none(),
            "r2 was LRU, should be evicted."
        );
    }

    #[tokio::test]
    async fn test_memory_expired_entry_not_loaded() {
        // Goal: verify that expired entries are lazily pruned on load.
        let store = MemorySessionStore::new(100);
        let id = Id::default();
        let mut record = create_expired_record(id);
        store.create(&mut record).await.unwrap();

        let loaded = store.load(&record.id).await.unwrap();
        assert!(loaded.is_none(), "Expired entry should return None.");

        // Also verify the entry was removed from the map.
        let map = store.entries.lock().unwrap();
        assert!(
            !map.contains_key(&record.id),
            "Expired entry should be removed from the map."
        );
    }

    #[tokio::test]
    async fn test_memory_save_overwrites_existing() {
        // Goal: verify that save() updates an existing entry's data.
        let store = MemorySessionStore::new(100);
        let id = Id::default();
        let mut record = create_test_record(id);
        store.create(&mut record).await.unwrap();

        // Modify the record and save.
        record
            .data
            .insert("key".to_string(), serde_json::json!("value"));
        store.save(&record).await.unwrap();

        let loaded = store.load(&id).await.unwrap().unwrap();
        assert_eq!(
            loaded.data.get("key"),
            Some(&serde_json::json!("value")),
            "Save should overwrite existing entry data."
        );
    }

    #[test]
    #[should_panic(expected = "Session cache capacity must be > 0")]
    fn test_memory_zero_capacity_panics() {
        // Goal: verify the TigerStyle assertion rejects zero capacity.
        let _ = MemorySessionStore::new(0);
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
