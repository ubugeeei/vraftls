//! Service discovery

use std::net::SocketAddr;
use vraftls_core::{NodeId, Result};

/// Service discovery mechanism
pub trait ServiceDiscovery: Send + Sync {
    /// Discover cluster nodes
    fn discover(&self) -> impl std::future::Future<Output = Result<Vec<(NodeId, SocketAddr)>>> + Send;

    /// Register this node
    fn register(&self, node_id: NodeId, addr: SocketAddr) -> impl std::future::Future<Output = Result<()>> + Send;

    /// Deregister this node
    fn deregister(&self, node_id: NodeId) -> impl std::future::Future<Output = Result<()>> + Send;
}

/// Static list discovery (for development/testing)
pub struct StaticDiscovery {
    nodes: Vec<(NodeId, SocketAddr)>,
}

impl StaticDiscovery {
    pub fn new(nodes: Vec<(NodeId, SocketAddr)>) -> Self {
        Self { nodes }
    }
}

impl ServiceDiscovery for StaticDiscovery {
    async fn discover(&self) -> Result<Vec<(NodeId, SocketAddr)>> {
        Ok(self.nodes.clone())
    }

    async fn register(&self, _node_id: NodeId, _addr: SocketAddr) -> Result<()> {
        // No-op for static discovery
        Ok(())
    }

    async fn deregister(&self, _node_id: NodeId) -> Result<()> {
        // No-op for static discovery
        Ok(())
    }
}
