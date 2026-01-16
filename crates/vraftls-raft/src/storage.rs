//! Raft log storage implementation using RocksDB
//!
//! OpenRaft requires two storage traits:
//! - RaftLogStorage: for storing log entries
//! - RaftStateMachine: for applying committed entries (see state_machine.rs)

use crate::types::{RaftNodeId, VRaftNode, VRaftTypeConfig, VfsRequest};
use openraft::storage::{LogFlushed, LogState, RaftLogStorage};
use openraft::{
    Entry, EntryPayload, LogId, OptionalSend, RaftLogReader, SnapshotMeta, StorageError,
    StoredMembership, Vote,
};
use rocksdb::{ColumnFamily, ColumnFamilyDescriptor, Options, DB};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt::Debug;
use std::ops::RangeBounds;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Column family names
const CF_LOGS: &str = "logs";
const CF_META: &str = "meta";

/// Metadata keys
const KEY_VOTE: &[u8] = b"vote";
const KEY_COMMITTED: &[u8] = b"committed";
const KEY_LAST_PURGED: &[u8] = b"last_purged";

/// RocksDB-backed log storage
pub struct RocksDbLogStorage {
    /// RocksDB instance
    db: Arc<DB>,

    /// In-memory cache for recent logs (for performance)
    log_cache: RwLock<BTreeMap<u64, Entry<VRaftTypeConfig>>>,

    /// Current vote
    vote: RwLock<Option<Vote<RaftNodeId>>>,

    /// Last committed log id
    committed: RwLock<Option<LogId<RaftNodeId>>>,

    /// Last purged log id
    last_purged: RwLock<Option<LogId<RaftNodeId>>>,
}

impl RocksDbLogStorage {
    /// Create a new RocksDB-backed log storage
    pub fn new(data_dir: impl AsRef<Path>) -> Result<Self, StorageError<RaftNodeId>> {
        let path = data_dir.as_ref().join("raft-log");

        let mut opts = Options::default();
        opts.create_if_missing(true);
        opts.create_missing_column_families(true);

        let cf_opts = Options::default();
        let cf_descriptors = vec![
            ColumnFamilyDescriptor::new(CF_LOGS, cf_opts.clone()),
            ColumnFamilyDescriptor::new(CF_META, cf_opts),
        ];

        let db = DB::open_cf_descriptors(&opts, &path, cf_descriptors)
            .map_err(|e| StorageError::from_io_error(openraft::ErrorSubject::Store, openraft::ErrorVerb::Read, e.into()))?;

        let storage = Self {
            db: Arc::new(db),
            log_cache: RwLock::new(BTreeMap::new()),
            vote: RwLock::new(None),
            committed: RwLock::new(None),
            last_purged: RwLock::new(None),
        };

        // Load metadata from disk
        storage.load_metadata()?;

        Ok(storage)
    }

    /// Load metadata from RocksDB
    fn load_metadata(&self) -> Result<(), StorageError<RaftNodeId>> {
        let cf = self.db.cf_handle(CF_META).ok_or_else(|| {
            StorageError::from_io_error(
                openraft::ErrorSubject::Store,
                openraft::ErrorVerb::Read,
                std::io::Error::new(std::io::ErrorKind::NotFound, "meta cf not found"),
            )
        })?;

        // Load vote
        if let Some(data) = self.db.get_cf(cf, KEY_VOTE).map_err(|e| {
            StorageError::from_io_error(openraft::ErrorSubject::Vote, openraft::ErrorVerb::Read, e.into())
        })? {
            let vote: Vote<RaftNodeId> = serde_json::from_slice(&data).map_err(|e| {
                StorageError::from_io_error(openraft::ErrorSubject::Vote, openraft::ErrorVerb::Read, e.into())
            })?;
            *self.vote.blocking_write() = Some(vote);
        }

        // Load committed
        if let Some(data) = self.db.get_cf(cf, KEY_COMMITTED).map_err(|e| {
            StorageError::from_io_error(openraft::ErrorSubject::Store, openraft::ErrorVerb::Read, e.into())
        })? {
            let committed: LogId<RaftNodeId> = serde_json::from_slice(&data).map_err(|e| {
                StorageError::from_io_error(openraft::ErrorSubject::Store, openraft::ErrorVerb::Read, e.into())
            })?;
            *self.committed.blocking_write() = Some(committed);
        }

        // Load last_purged
        if let Some(data) = self.db.get_cf(cf, KEY_LAST_PURGED).map_err(|e| {
            StorageError::from_io_error(openraft::ErrorSubject::Store, openraft::ErrorVerb::Read, e.into())
        })? {
            let last_purged: LogId<RaftNodeId> = serde_json::from_slice(&data).map_err(|e| {
                StorageError::from_io_error(openraft::ErrorSubject::Store, openraft::ErrorVerb::Read, e.into())
            })?;
            *self.last_purged.blocking_write() = Some(last_purged);
        }

        Ok(())
    }

    /// Get the logs column family
    fn cf_logs(&self) -> &ColumnFamily {
        self.db.cf_handle(CF_LOGS).expect("logs cf must exist")
    }

    /// Get the meta column family
    fn cf_meta(&self) -> &ColumnFamily {
        self.db.cf_handle(CF_META).expect("meta cf must exist")
    }

    /// Convert log index to RocksDB key
    fn log_key(index: u64) -> [u8; 8] {
        index.to_be_bytes()
    }

    /// Save an entry to RocksDB
    fn save_entry(&self, entry: &Entry<VRaftTypeConfig>) -> Result<(), StorageError<RaftNodeId>> {
        let key = Self::log_key(entry.log_id.index);
        let value = serde_json::to_vec(entry).map_err(|e| {
            StorageError::from_io_error(openraft::ErrorSubject::Logs, openraft::ErrorVerb::Write, e.into())
        })?;

        self.db.put_cf(self.cf_logs(), key, value).map_err(|e| {
            StorageError::from_io_error(openraft::ErrorSubject::Logs, openraft::ErrorVerb::Write, e.into())
        })?;

        Ok(())
    }

    /// Load an entry from RocksDB
    fn load_entry(&self, index: u64) -> Result<Option<Entry<VRaftTypeConfig>>, StorageError<RaftNodeId>> {
        let key = Self::log_key(index);

        match self.db.get_cf(self.cf_logs(), key) {
            Ok(Some(data)) => {
                let entry: Entry<VRaftTypeConfig> = serde_json::from_slice(&data).map_err(|e| {
                    StorageError::from_io_error(openraft::ErrorSubject::Logs, openraft::ErrorVerb::Read, e.into())
                })?;
                Ok(Some(entry))
            }
            Ok(None) => Ok(None),
            Err(e) => Err(StorageError::from_io_error(
                openraft::ErrorSubject::Logs,
                openraft::ErrorVerb::Read,
                e.into(),
            )),
        }
    }

    /// Delete entries up to (exclusive) the given index
    fn delete_entries_before(&self, before_index: u64) -> Result<(), StorageError<RaftNodeId>> {
        let cf = self.cf_logs();
        let start_key = Self::log_key(0);
        let end_key = Self::log_key(before_index);

        self.db.delete_range_cf(cf, start_key, end_key).map_err(|e| {
            StorageError::from_io_error(openraft::ErrorSubject::Logs, openraft::ErrorVerb::Write, e.into())
        })?;

        Ok(())
    }
}

impl RaftLogReader<VRaftTypeConfig> for Arc<RocksDbLogStorage> {
    async fn try_get_log_entries<RB: RangeBounds<u64> + Clone + Debug + OptionalSend>(
        &mut self,
        range: RB,
    ) -> Result<Vec<Entry<VRaftTypeConfig>>, StorageError<RaftNodeId>> {
        let start = match range.start_bound() {
            std::ops::Bound::Included(&n) => n,
            std::ops::Bound::Excluded(&n) => n + 1,
            std::ops::Bound::Unbounded => 0,
        };

        let end = match range.end_bound() {
            std::ops::Bound::Included(&n) => n + 1,
            std::ops::Bound::Excluded(&n) => n,
            std::ops::Bound::Unbounded => u64::MAX,
        };

        // First check cache
        let cache = self.log_cache.read().await;
        let mut entries = Vec::new();

        for idx in start..end {
            if let Some(entry) = cache.get(&idx) {
                entries.push(entry.clone());
            } else {
                // Load from disk
                drop(cache);
                if let Some(entry) = self.load_entry(idx)? {
                    entries.push(entry);
                } else {
                    break;
                }
                // Re-acquire cache
                let cache = self.log_cache.read().await;
                continue;
            }
        }

        Ok(entries)
    }
}

impl RaftLogStorage<VRaftTypeConfig> for Arc<RocksDbLogStorage> {
    type LogReader = Self;

    async fn get_log_state(&mut self) -> Result<LogState<VRaftTypeConfig>, StorageError<RaftNodeId>> {
        let last_purged = self.last_purged.read().await.clone();

        // Find the last log entry
        let cache = self.log_cache.read().await;
        let last_log_id = if let Some((&idx, entry)) = cache.iter().next_back() {
            Some(entry.log_id)
        } else {
            // Check RocksDB for the last entry
            let cf = self.cf_logs();
            let mut iter = self.db.raw_iterator_cf(cf);
            iter.seek_to_last();
            if iter.valid() {
                if let Some(value) = iter.value() {
                    let entry: Entry<VRaftTypeConfig> = serde_json::from_slice(value).map_err(|e| {
                        StorageError::from_io_error(openraft::ErrorSubject::Logs, openraft::ErrorVerb::Read, e.into())
                    })?;
                    Some(entry.log_id)
                } else {
                    last_purged
                }
            } else {
                last_purged
            }
        };

        Ok(LogState {
            last_purged_log_id: last_purged,
            last_log_id,
        })
    }

    async fn save_committed(&mut self, committed: Option<LogId<RaftNodeId>>) -> Result<(), StorageError<RaftNodeId>> {
        *self.committed.write().await = committed;

        if let Some(ref c) = committed {
            let data = serde_json::to_vec(c).map_err(|e| {
                StorageError::from_io_error(openraft::ErrorSubject::Store, openraft::ErrorVerb::Write, e.into())
            })?;
            self.db.put_cf(self.cf_meta(), KEY_COMMITTED, data).map_err(|e| {
                StorageError::from_io_error(openraft::ErrorSubject::Store, openraft::ErrorVerb::Write, e.into())
            })?;
        }

        Ok(())
    }

    async fn read_committed(&mut self) -> Result<Option<LogId<RaftNodeId>>, StorageError<RaftNodeId>> {
        Ok(self.committed.read().await.clone())
    }

    async fn save_vote(&mut self, vote: &Vote<RaftNodeId>) -> Result<(), StorageError<RaftNodeId>> {
        *self.vote.write().await = Some(*vote);

        let data = serde_json::to_vec(vote).map_err(|e| {
            StorageError::from_io_error(openraft::ErrorSubject::Vote, openraft::ErrorVerb::Write, e.into())
        })?;
        self.db.put_cf(self.cf_meta(), KEY_VOTE, data).map_err(|e| {
            StorageError::from_io_error(openraft::ErrorSubject::Vote, openraft::ErrorVerb::Write, e.into())
        })?;

        Ok(())
    }

    async fn append<I>(&mut self, entries: I, callback: LogFlushed<VRaftTypeConfig>) -> Result<(), StorageError<RaftNodeId>>
    where
        I: IntoIterator<Item = Entry<VRaftTypeConfig>> + OptionalSend,
        I::IntoIter: OptionalSend,
    {
        let mut cache = self.log_cache.write().await;

        for entry in entries {
            // Save to RocksDB
            self.save_entry(&entry)?;

            // Update cache
            cache.insert(entry.log_id.index, entry);
        }

        // Notify that logs are flushed
        callback.log_io_completed(Ok(()));

        Ok(())
    }

    async fn truncate(&mut self, log_id: LogId<RaftNodeId>) -> Result<(), StorageError<RaftNodeId>> {
        let mut cache = self.log_cache.write().await;

        // Remove from cache
        cache.split_off(&log_id.index);

        // Remove from RocksDB
        let cf = self.cf_logs();
        let start_key = Self::log_key(log_id.index);
        let end_key = Self::log_key(u64::MAX);

        self.db.delete_range_cf(cf, start_key, end_key).map_err(|e| {
            StorageError::from_io_error(openraft::ErrorSubject::Logs, openraft::ErrorVerb::Write, e.into())
        })?;

        Ok(())
    }

    async fn purge(&mut self, log_id: LogId<RaftNodeId>) -> Result<(), StorageError<RaftNodeId>> {
        *self.last_purged.write().await = Some(log_id);

        // Save last_purged to RocksDB
        let data = serde_json::to_vec(&log_id).map_err(|e| {
            StorageError::from_io_error(openraft::ErrorSubject::Store, openraft::ErrorVerb::Write, e.into())
        })?;
        self.db.put_cf(self.cf_meta(), KEY_LAST_PURGED, data).map_err(|e| {
            StorageError::from_io_error(openraft::ErrorSubject::Store, openraft::ErrorVerb::Write, e.into())
        })?;

        // Remove from cache
        let mut cache = self.log_cache.write().await;
        cache = cache.split_off(&(log_id.index + 1));

        // Remove from RocksDB
        self.delete_entries_before(log_id.index + 1)?;

        Ok(())
    }

    async fn get_log_reader(&mut self) -> Self::LogReader {
        self.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_create_storage() {
        let temp_dir = TempDir::new().unwrap();
        let storage = RocksDbLogStorage::new(temp_dir.path()).unwrap();
        assert!(storage.vote.read().await.is_none());
    }
}
