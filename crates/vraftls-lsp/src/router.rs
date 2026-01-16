//! LSP Request Router

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use vraftls_core::{NodeId, PartitionKey, RaftGroupId};
use vraftls_vfs::VfsPath;

/// Decision on how to route an LSP request
#[derive(Clone, Debug)]
pub enum RouteDecision {
    /// Route to a single node
    Single(NodeId),

    /// Route to local node only
    LocalOnly,

    /// Fan out to multiple nodes and aggregate results
    ScatterGather(Vec<RaftGroupId>),

    /// Two-phase commit across multiple groups
    TwoPhaseCommit(Vec<RaftGroupId>),
}

/// Router for LSP requests
pub struct LspRouter {
    /// File to node mapping cache
    file_cache: RwLock<HashMap<VfsPath, NodeId>>,

    /// Raft group to leader node mapping
    group_leaders: RwLock<HashMap<RaftGroupId, NodeId>>,

    /// Local node ID
    local_node_id: RwLock<Option<NodeId>>,
}

impl LspRouter {
    pub fn new() -> Self {
        Self {
            file_cache: RwLock::new(HashMap::new()),
            group_leaders: RwLock::new(HashMap::new()),
            local_node_id: RwLock::new(None),
        }
    }

    /// Set the local node ID
    pub async fn set_local_node(&self, node_id: NodeId) {
        let mut local = self.local_node_id.write().await;
        *local = Some(node_id);
    }

    /// Update the leader for a Raft group
    pub async fn update_leader(&self, group_id: RaftGroupId, leader: NodeId) {
        let mut leaders = self.group_leaders.write().await;
        leaders.insert(group_id, leader);
    }

    /// Route a file-based request
    pub async fn route_for_file(&self, path: &VfsPath) -> RouteDecision {
        // Check cache first
        let cache = self.file_cache.read().await;
        if let Some(node_id) = cache.get(path) {
            return RouteDecision::Single(*node_id);
        }
        drop(cache);

        // For now, return local only (single node mode)
        RouteDecision::LocalOnly
    }

    /// Route a workspace-wide request (scatter-gather)
    pub async fn route_workspace(&self) -> RouteDecision {
        let leaders = self.group_leaders.read().await;
        if leaders.is_empty() {
            return RouteDecision::LocalOnly;
        }

        let groups: Vec<RaftGroupId> = leaders.keys().copied().collect();
        RouteDecision::ScatterGather(groups)
    }

    /// Get the leader node for a Raft group
    pub async fn get_leader(&self, group_id: RaftGroupId) -> Option<NodeId> {
        let leaders = self.group_leaders.read().await;
        leaders.get(&group_id).copied()
    }

    /// Cache a file's owner node
    pub async fn cache_file_owner(&self, path: VfsPath, node_id: NodeId) {
        let mut cache = self.file_cache.write().await;
        cache.insert(path, node_id);
    }

    /// Invalidate cache for a file
    pub async fn invalidate_file(&self, path: &VfsPath) {
        let mut cache = self.file_cache.write().await;
        cache.remove(path);
    }

    /// Clear all cached state
    pub async fn clear(&self) {
        let mut cache = self.file_cache.write().await;
        cache.clear();
        drop(cache);

        let mut leaders = self.group_leaders.write().await;
        leaders.clear();
    }
}

impl Default for LspRouter {
    fn default() -> Self {
        Self::new()
    }
}

/// Result aggregator for scatter-gather operations
pub struct ResponseAggregator<T> {
    responses: Vec<T>,
    errors: Vec<String>,
}

impl<T> ResponseAggregator<T> {
    pub fn new() -> Self {
        Self {
            responses: Vec::new(),
            errors: Vec::new(),
        }
    }

    pub fn add_response(&mut self, response: T) {
        self.responses.push(response);
    }

    pub fn add_error(&mut self, error: String) {
        self.errors.push(error);
    }

    pub fn into_results(self) -> (Vec<T>, Vec<String>) {
        (self.responses, self.errors)
    }

    pub fn responses(&self) -> &[T] {
        &self.responses
    }

    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }
}

impl<T> Default for ResponseAggregator<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Aggregator for completion responses
impl ResponseAggregator<tower_lsp::lsp_types::CompletionItem> {
    pub fn into_completion_response(
        self,
    ) -> Option<tower_lsp::lsp_types::CompletionResponse> {
        if self.responses.is_empty() {
            None
        } else {
            Some(tower_lsp::lsp_types::CompletionResponse::Array(
                self.responses,
            ))
        }
    }
}

/// Aggregator for location responses (references, definitions)
impl ResponseAggregator<tower_lsp::lsp_types::Location> {
    pub fn into_locations(self) -> Option<Vec<tower_lsp::lsp_types::Location>> {
        if self.responses.is_empty() {
            None
        } else {
            Some(self.responses)
        }
    }
}
