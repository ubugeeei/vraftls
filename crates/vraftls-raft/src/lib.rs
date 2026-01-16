//! VRaftLS Raft - Raft consensus implementation using OpenRaft
//!
//! This crate provides the Raft consensus layer for VRaftLS:
//!
//! - `types`: Type definitions for OpenRaft integration
//! - `storage`: RocksDB-backed log storage
//! - `state_machine`: VFS state machine that applies committed entries
//! - `network`: HTTP-based inter-node communication

pub mod network;
pub mod state_machine;
pub mod storage;
pub mod types;

pub use network::{HttpRaftNetwork, HttpRaftNetworkFactory};
pub use state_machine::{VfsSnapshot, VfsSnapshotState, VfsStateMachine};
pub use storage::RocksDbLogStorage;
pub use types::*;

use openraft::Raft;
use std::sync::Arc;

/// The Raft instance type for VRaftLS
pub type VRaftRaft = Raft<VRaftTypeConfig>;

/// Create a new Raft instance
pub async fn create_raft(
    node_id: RaftNodeId,
    config: openraft::Config,
    network: HttpRaftNetworkFactory,
    log_storage: Arc<RocksDbLogStorage>,
    state_machine: Arc<VfsStateMachine>,
) -> Result<VRaftRaft, openraft::error::Fatal<RaftNodeId>> {
    Raft::new(node_id, Arc::new(config), network, log_storage, state_machine).await
}
