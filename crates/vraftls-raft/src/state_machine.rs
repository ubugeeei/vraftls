//! Raft state machine for VFS
//!
//! The state machine applies committed log entries to the VFS.
//! This is where the actual file operations happen.

use crate::types::{RaftNodeId, VRaftNode, VRaftTypeConfig, VfsRequest, VfsStateMachineResponse};
use openraft::storage::{RaftSnapshotBuilder, RaftStateMachine, Snapshot};
use openraft::{
    Entry, EntryPayload, LogId, OptionalSend, SnapshotMeta, StorageError, StoredMembership,
};
use serde::{Deserialize, Serialize};
use std::io::Cursor;
use std::sync::Arc;
use tokio::sync::RwLock;
use vraftls_core::RaftGroupId;
use vraftls_vfs::{Vfs, VfsCommand, VfsHandle, VfsResponse};

/// VFS State Machine
///
/// Applies Raft log entries to the VFS
pub struct VfsStateMachine {
    /// The Virtual File System
    vfs: VfsHandle,

    /// Last applied log id
    last_applied_log: RwLock<Option<LogId<RaftNodeId>>>,

    /// Current membership configuration
    membership: RwLock<StoredMembership<VRaftTypeConfig>>,

    /// Raft group ID
    group_id: RaftGroupId,
}

impl VfsStateMachine {
    /// Create a new VFS state machine
    pub fn new(group_id: RaftGroupId) -> Self {
        Self {
            vfs: Arc::new(Vfs::new(group_id)),
            last_applied_log: RwLock::new(None),
            membership: RwLock::new(StoredMembership::default()),
            group_id,
        }
    }

    /// Create with an existing VFS
    pub fn with_vfs(group_id: RaftGroupId, vfs: VfsHandle) -> Self {
        Self {
            vfs,
            last_applied_log: RwLock::new(None),
            membership: RwLock::new(StoredMembership::default()),
            group_id,
        }
    }

    /// Get a reference to the VFS
    pub fn vfs(&self) -> &VfsHandle {
        &self.vfs
    }
}

/// Snapshot data structure
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VfsSnapshot {
    /// Last applied log id when snapshot was taken
    pub last_applied_log: Option<LogId<RaftNodeId>>,

    /// Membership configuration
    pub membership: StoredMembership<VRaftTypeConfig>,

    /// VFS state (serialized files)
    pub vfs_state: VfsSnapshotState,
}

/// VFS state in snapshot
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VfsSnapshotState {
    /// All files in the VFS
    pub files: Vec<vraftls_vfs::VfsFile>,
}

impl RaftSnapshotBuilder<VRaftTypeConfig> for Arc<VfsStateMachine> {
    async fn build_snapshot(&mut self) -> Result<Snapshot<VRaftTypeConfig>, StorageError<RaftNodeId>> {
        // Get current state
        let last_applied_log = self.last_applied_log.read().await.clone();
        let membership = self.membership.read().await.clone();

        // Collect all files
        let file_ids = self.vfs.all_file_ids();
        let files: Vec<_> = file_ids
            .into_iter()
            .filter_map(|id| self.vfs.get_file(id))
            .collect();

        let vfs_state = VfsSnapshotState { files };

        let snapshot = VfsSnapshot {
            last_applied_log,
            membership: membership.clone(),
            vfs_state,
        };

        // Serialize snapshot
        let data = serde_json::to_vec(&snapshot).map_err(|e| {
            StorageError::from_io_error(
                openraft::ErrorSubject::StateMachine,
                openraft::ErrorVerb::Write,
                e.into(),
            )
        })?;

        let snapshot_id = format!(
            "{}-{}",
            last_applied_log.map(|l| l.index).unwrap_or(0),
            chrono::Utc::now().timestamp()
        );

        Ok(Snapshot {
            meta: SnapshotMeta {
                last_log_id: last_applied_log,
                last_membership: membership,
                snapshot_id,
            },
            snapshot: Box::new(Cursor::new(data)),
        })
    }
}

impl RaftStateMachine<VRaftTypeConfig> for Arc<VfsStateMachine> {
    type SnapshotBuilder = Self;

    async fn applied_state(
        &mut self,
    ) -> Result<(Option<LogId<RaftNodeId>>, StoredMembership<VRaftTypeConfig>), StorageError<RaftNodeId>> {
        let last_applied = self.last_applied_log.read().await.clone();
        let membership = self.membership.read().await.clone();
        Ok((last_applied, membership))
    }

    async fn apply<I>(&mut self, entries: I) -> Result<Vec<VfsStateMachineResponse>, StorageError<RaftNodeId>>
    where
        I: IntoIterator<Item = Entry<VRaftTypeConfig>> + OptionalSend,
        I::IntoIter: OptionalSend,
    {
        let mut responses = Vec::new();

        for entry in entries {
            // Update last applied log
            *self.last_applied_log.write().await = Some(entry.log_id);

            match entry.payload {
                EntryPayload::Blank => {
                    responses.push(VfsStateMachineResponse {
                        response: VfsResponse::Ok(None),
                    });
                }
                EntryPayload::Normal(request) => {
                    // Apply the VFS command
                    let vfs_response = self.vfs.apply(request.command);
                    responses.push(VfsStateMachineResponse {
                        response: vfs_response,
                    });
                }
                EntryPayload::Membership(membership) => {
                    *self.membership.write().await = StoredMembership::new(
                        Some(entry.log_id),
                        membership,
                    );
                    responses.push(VfsStateMachineResponse {
                        response: VfsResponse::Ok(None),
                    });
                }
            }
        }

        Ok(responses)
    }

    async fn get_snapshot_builder(&mut self) -> Self::SnapshotBuilder {
        self.clone()
    }

    async fn begin_receiving_snapshot(&mut self) -> Result<Box<Cursor<Vec<u8>>>, StorageError<RaftNodeId>> {
        Ok(Box::new(Cursor::new(Vec::new())))
    }

    async fn install_snapshot(
        &mut self,
        meta: &SnapshotMeta<VRaftTypeConfig>,
        snapshot: Box<Cursor<Vec<u8>>>,
    ) -> Result<(), StorageError<RaftNodeId>> {
        // Deserialize snapshot
        let data = snapshot.into_inner();
        let vfs_snapshot: VfsSnapshot = serde_json::from_slice(&data).map_err(|e| {
            StorageError::from_io_error(
                openraft::ErrorSubject::StateMachine,
                openraft::ErrorVerb::Read,
                e.into(),
            )
        })?;

        // Update state
        *self.last_applied_log.write().await = vfs_snapshot.last_applied_log;
        *self.membership.write().await = vfs_snapshot.membership;

        // Restore VFS state
        // First, we need to recreate the VFS with the snapshot data
        // For now, we'll just apply all the files
        for file in vfs_snapshot.vfs_state.files {
            self.vfs.apply(VfsCommand::CreateFile {
                path: file.path.clone(),
                content: file.content.as_str().map(|s| s.to_string()).unwrap_or_default(),
            });
        }

        Ok(())
    }

    async fn get_current_snapshot(
        &mut self,
    ) -> Result<Option<Snapshot<VRaftTypeConfig>>, StorageError<RaftNodeId>> {
        // Build a snapshot of current state
        let mut builder = self.clone();
        let snapshot = builder.build_snapshot().await?;
        Ok(Some(snapshot))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_state_machine() {
        let sm = VfsStateMachine::new(RaftGroupId::new(1));
        assert_eq!(sm.vfs.file_count(), 0);
    }
}
