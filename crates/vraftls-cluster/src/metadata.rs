//! Metadata Raft group management

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use vraftls_core::{NodeId, PartitionKey, RaftGroupId};

/// Routing table entry
#[derive(Clone, Debug)]
pub struct RoutingEntry {
    pub group_id: RaftGroupId,
    pub leader: Option<NodeId>,
    pub replicas: Vec<NodeId>,
}

/// Cluster metadata (stored in Raft group 0)
pub struct ClusterMetadata {
    /// Partition key to Raft group mapping
    routing_table: RwLock<HashMap<PartitionKey, RoutingEntry>>,

    /// Raft group to node mapping
    group_nodes: RwLock<HashMap<RaftGroupId, Vec<NodeId>>>,

    /// Next Raft group ID
    next_group_id: RwLock<u64>,
}

impl ClusterMetadata {
    pub fn new() -> Self {
        Self {
            routing_table: RwLock::new(HashMap::new()),
            group_nodes: RwLock::new(HashMap::new()),
            next_group_id: RwLock::new(1), // 0 is reserved for metadata group
        }
    }

    /// Lookup the Raft group for a partition key
    pub async fn lookup(&self, key: &PartitionKey) -> Option<RoutingEntry> {
        let table = self.routing_table.read().await;
        table.get(key).cloned()
    }

    /// Update routing entry
    pub async fn update_routing(&self, key: PartitionKey, entry: RoutingEntry) {
        let mut table = self.routing_table.write().await;
        table.insert(key, entry);
    }

    /// Get nodes for a Raft group
    pub async fn get_group_nodes(&self, group_id: RaftGroupId) -> Vec<NodeId> {
        let groups = self.group_nodes.read().await;
        groups.get(&group_id).cloned().unwrap_or_default()
    }

    /// Allocate a new Raft group ID
    pub async fn allocate_group_id(&self) -> RaftGroupId {
        let mut next_id = self.next_group_id.write().await;
        let id = *next_id;
        *next_id += 1;
        RaftGroupId::new(id)
    }
}

impl Default for ClusterMetadata {
    fn default() -> Self {
        Self::new()
    }
}
