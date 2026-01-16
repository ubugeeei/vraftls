//! Cluster membership management

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use vraftls_core::{NodeId, RaftGroupId, Timestamp};

/// Node status in the cluster
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum NodeStatus {
    /// Node is healthy and responding
    Healthy,
    /// Node is suspected to be down
    Suspect,
    /// Node is confirmed down
    Down,
    /// Node is joining the cluster
    Joining,
    /// Node is leaving the cluster
    Leaving,
}

/// Information about a cluster node
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ClusterNode {
    pub id: NodeId,
    pub addr: SocketAddr,
    pub status: NodeStatus,
    pub raft_groups: Vec<RaftGroupId>,
    pub last_heartbeat: Timestamp,
}

/// Cluster membership registry
pub struct ClusterMembership {
    /// Known nodes
    nodes: DashMap<NodeId, ClusterNode>,

    /// Local node ID
    local_node_id: NodeId,
}

impl ClusterMembership {
    pub fn new(local_node_id: NodeId) -> Self {
        Self {
            nodes: DashMap::new(),
            local_node_id,
        }
    }

    /// Add or update a node
    pub fn upsert_node(&self, node: ClusterNode) {
        self.nodes.insert(node.id, node);
    }

    /// Get a node by ID
    pub fn get_node(&self, id: NodeId) -> Option<ClusterNode> {
        self.nodes.get(&id).map(|n| n.clone())
    }

    /// Get all healthy nodes
    pub fn healthy_nodes(&self) -> Vec<ClusterNode> {
        self.nodes
            .iter()
            .filter(|n| n.status == NodeStatus::Healthy)
            .map(|n| n.clone())
            .collect()
    }

    /// Mark a node as suspect
    pub fn mark_suspect(&self, id: NodeId) {
        if let Some(mut node) = self.nodes.get_mut(&id) {
            node.status = NodeStatus::Suspect;
        }
    }

    /// Mark a node as down
    pub fn mark_down(&self, id: NodeId) {
        if let Some(mut node) = self.nodes.get_mut(&id) {
            node.status = NodeStatus::Down;
        }
    }

    /// Update heartbeat for a node
    pub fn update_heartbeat(&self, id: NodeId) {
        if let Some(mut node) = self.nodes.get_mut(&id) {
            node.last_heartbeat = Timestamp::now();
            if node.status == NodeStatus::Suspect {
                node.status = NodeStatus::Healthy;
            }
        }
    }

    /// Remove a node
    pub fn remove_node(&self, id: NodeId) {
        self.nodes.remove(&id);
    }

    /// Get node count
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Get healthy node count
    pub fn healthy_node_count(&self) -> usize {
        self.nodes
            .iter()
            .filter(|n| n.status == NodeStatus::Healthy)
            .count()
    }
}
