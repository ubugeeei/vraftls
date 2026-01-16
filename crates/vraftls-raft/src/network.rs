//! HTTP-based Raft network implementation
//!
//! Handles node-to-node communication for Raft consensus.

use crate::types::{RaftNodeId, VRaftNode, VRaftTypeConfig, VfsRequest};
use openraft::error::{InstallSnapshotError, RPCError, RaftError, RemoteError};
use openraft::network::{RPCOption, RaftNetwork, RaftNetworkFactory};
use openraft::raft::{
    AppendEntriesRequest, AppendEntriesResponse, InstallSnapshotRequest, InstallSnapshotResponse,
    VoteRequest, VoteResponse,
};
use openraft::OptionalSend;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;

/// HTTP network factory
pub struct HttpRaftNetworkFactory {
    /// HTTP client
    client: Client,
}

impl HttpRaftNetworkFactory {
    pub fn new() -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .expect("Failed to create HTTP client");

        Self { client }
    }
}

impl Default for HttpRaftNetworkFactory {
    fn default() -> Self {
        Self::new()
    }
}

impl RaftNetworkFactory<VRaftTypeConfig> for HttpRaftNetworkFactory {
    type Network = HttpRaftNetwork;

    async fn new_client(&mut self, target: RaftNodeId, node: &VRaftNode) -> Self::Network {
        HttpRaftNetwork {
            client: self.client.clone(),
            target,
            target_addr: node.addr.clone(),
        }
    }
}

/// HTTP-based Raft network
pub struct HttpRaftNetwork {
    /// HTTP client
    client: Client,

    /// Target node ID
    target: RaftNodeId,

    /// Target node address
    target_addr: String,
}

impl HttpRaftNetwork {
    /// Build URL for an endpoint
    fn url(&self, endpoint: &str) -> String {
        format!("http://{}/raft/{}", self.target_addr, endpoint)
    }

    /// Send a POST request
    async fn post<Req, Resp>(&self, endpoint: &str, request: &Req) -> Result<Resp, RPCError<RaftNodeId, VRaftNode, RaftError<RaftNodeId>>>
    where
        Req: Serialize + Send + Sync,
        Resp: for<'de> Deserialize<'de>,
    {
        let url = self.url(endpoint);

        let response = self
            .client
            .post(&url)
            .json(request)
            .send()
            .await
            .map_err(|e| RPCError::Network(openraft::error::NetworkError::new(&e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(RPCError::Network(openraft::error::NetworkError::new(&std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("HTTP {}: {}", status, text),
            ))));
        }

        response
            .json()
            .await
            .map_err(|e| RPCError::Network(openraft::error::NetworkError::new(&e)))
    }
}

impl RaftNetwork<VRaftTypeConfig> for HttpRaftNetwork {
    async fn append_entries(
        &mut self,
        request: AppendEntriesRequest<VRaftTypeConfig>,
        _option: RPCOption,
    ) -> Result<AppendEntriesResponse<RaftNodeId>, RPCError<RaftNodeId, VRaftNode, RaftError<RaftNodeId>>> {
        self.post("append_entries", &request).await
    }

    async fn install_snapshot(
        &mut self,
        request: InstallSnapshotRequest<VRaftTypeConfig>,
        _option: RPCOption,
    ) -> Result<InstallSnapshotResponse<RaftNodeId>, RPCError<RaftNodeId, VRaftNode, RaftError<RaftNodeId, InstallSnapshotError>>> {
        self.post("install_snapshot", &request).await
    }

    async fn vote(
        &mut self,
        request: VoteRequest<RaftNodeId>,
        _option: RPCOption,
    ) -> Result<VoteResponse<RaftNodeId>, RPCError<RaftNodeId, VRaftNode, RaftError<RaftNodeId>>> {
        self.post("vote", &request).await
    }
}

/// HTTP request/response types for Raft RPC

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RaftRpcRequest<T> {
    pub body: T,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RaftRpcResponse<T> {
    pub body: T,
}

/// Wrapper for serialization
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AppendEntriesRequestWrapper {
    pub vote: openraft::Vote<RaftNodeId>,
    pub prev_log_id: Option<openraft::LogId<RaftNodeId>>,
    pub entries: Vec<openraft::Entry<VRaftTypeConfig>>,
    pub leader_commit: Option<openraft::LogId<RaftNodeId>>,
}

impl From<AppendEntriesRequest<VRaftTypeConfig>> for AppendEntriesRequestWrapper {
    fn from(req: AppendEntriesRequest<VRaftTypeConfig>) -> Self {
        Self {
            vote: req.vote,
            prev_log_id: req.prev_log_id,
            entries: req.entries,
            leader_commit: req.leader_commit,
        }
    }
}

impl From<AppendEntriesRequestWrapper> for AppendEntriesRequest<VRaftTypeConfig> {
    fn from(wrapper: AppendEntriesRequestWrapper) -> Self {
        Self {
            vote: wrapper.vote,
            prev_log_id: wrapper.prev_log_id,
            entries: wrapper.entries,
            leader_commit: wrapper.leader_commit,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_network_factory() {
        let factory = HttpRaftNetworkFactory::new();
        assert!(true);
    }
}
