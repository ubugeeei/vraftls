//! Configuration types for VRaftLS

use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

/// Main configuration for a VRaftLS node
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NodeConfig {
    /// Unique node identifier
    pub node_id: u64,

    /// Address for node-to-node communication
    pub cluster_addr: SocketAddr,

    /// Data directory for persistent storage
    pub data_dir: PathBuf,

    /// Raft configuration
    pub raft: RaftConfig,

    /// VFS configuration
    pub vfs: VfsConfig,

    /// Cache configuration
    pub cache: CacheConfig,
}

impl Default for NodeConfig {
    fn default() -> Self {
        Self {
            node_id: 1,
            cluster_addr: "127.0.0.1:8080".parse().unwrap(),
            data_dir: PathBuf::from("./data"),
            raft: RaftConfig::default(),
            vfs: VfsConfig::default(),
            cache: CacheConfig::default(),
        }
    }
}

/// Raft consensus configuration
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RaftConfig {
    /// Heartbeat interval
    #[serde(with = "duration_millis")]
    pub heartbeat_interval: Duration,

    /// Minimum election timeout
    #[serde(with = "duration_millis")]
    pub election_timeout_min: Duration,

    /// Maximum election timeout
    #[serde(with = "duration_millis")]
    pub election_timeout_max: Duration,

    /// Maximum entries per AppendEntries RPC
    pub max_append_entries: u64,

    /// Snapshot chunk size for transfer
    pub snapshot_chunk_size: u64,

    /// Maximum log entries before triggering snapshot
    pub max_log_entries: u64,

    /// Maximum log bytes before triggering snapshot
    pub max_log_bytes: u64,
}

impl Default for RaftConfig {
    fn default() -> Self {
        Self {
            heartbeat_interval: Duration::from_millis(100),
            election_timeout_min: Duration::from_millis(300),
            election_timeout_max: Duration::from_millis(500),
            max_append_entries: 100,
            snapshot_chunk_size: 1024 * 1024, // 1MB
            max_log_entries: 10000,
            max_log_bytes: 100 * 1024 * 1024, // 100MB
        }
    }
}

/// Virtual File System configuration
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VfsConfig {
    /// Maximum file size in bytes
    pub max_file_size: u64,

    /// Maximum files per Raft group
    pub max_files_per_group: u64,

    /// Enable file content compression
    pub enable_compression: bool,
}

impl Default for VfsConfig {
    fn default() -> Self {
        Self {
            max_file_size: 10 * 1024 * 1024,   // 10MB
            max_files_per_group: 200,
            enable_compression: true,
        }
    }
}

/// Cache configuration
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CacheConfig {
    /// L1 (in-memory) cache maximum entries
    pub l1_max_entries: u64,

    /// L2 (disk) cache maximum bytes
    pub l2_max_bytes: u64,

    /// Cache entry TTL
    #[serde(with = "duration_secs")]
    pub ttl: Duration,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            l1_max_entries: 10000,
            l2_max_bytes: 1024 * 1024 * 1024, // 1GB
            ttl: Duration::from_secs(3600),   // 1 hour
        }
    }
}

/// Gateway configuration
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GatewayConfig {
    /// Cluster nodes to connect to
    pub cluster_nodes: Vec<SocketAddr>,

    /// Request timeout
    #[serde(with = "duration_secs")]
    pub request_timeout: Duration,

    /// Connection pool size per node
    pub pool_size: u32,
}

impl Default for GatewayConfig {
    fn default() -> Self {
        Self {
            cluster_nodes: vec!["127.0.0.1:8080".parse().unwrap()],
            request_timeout: Duration::from_secs(30),
            pool_size: 10,
        }
    }
}

// Serde helpers for Duration
mod duration_millis {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::time::Duration;

    pub fn serialize<S>(duration: &Duration, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        duration.as_millis().serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Duration, D::Error>
    where
        D: Deserializer<'de>,
    {
        let millis = u64::deserialize(deserializer)?;
        Ok(Duration::from_millis(millis))
    }
}

mod duration_secs {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::time::Duration;

    pub fn serialize<S>(duration: &Duration, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        duration.as_secs().serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Duration, D::Error>
    where
        D: Deserializer<'de>,
    {
        let secs = u64::deserialize(deserializer)?;
        Ok(Duration::from_secs(secs))
    }
}
