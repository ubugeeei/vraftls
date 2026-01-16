//! Core types for VRaftLS

use serde::{Deserialize, Serialize};
use std::fmt;

/// Opaque file identifier (stable across renames)
#[derive(Clone, Copy, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct FileId(pub u64);

impl FileId {
    pub fn new(id: u64) -> Self {
        Self(id)
    }
}

impl fmt::Debug for FileId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "FileId({})", self.0)
    }
}

impl fmt::Display for FileId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Node identifier in the cluster
#[derive(Clone, Copy, Hash, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub struct NodeId(pub u64);

impl NodeId {
    pub fn new(id: u64) -> Self {
        Self(id)
    }
}

impl fmt::Debug for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "NodeId({})", self.0)
    }
}

impl fmt::Display for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Raft group identifier
#[derive(Clone, Copy, Hash, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub struct RaftGroupId(pub u64);

impl RaftGroupId {
    pub fn new(id: u64) -> Self {
        Self(id)
    }

    /// Metadata Raft group (always group 0)
    pub const METADATA: Self = Self(0);
}

impl fmt::Debug for RaftGroupId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "RaftGroupId({})", self.0)
    }
}

impl fmt::Display for RaftGroupId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Partition key for consistent hashing
#[derive(Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct PartitionKey(pub Vec<u8>);

impl PartitionKey {
    pub fn from_bytes(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }

    pub fn from_path(path: &str) -> Self {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        path.hash(&mut hasher);
        Self(hasher.finish().to_le_bytes().to_vec())
    }
}

impl fmt::Debug for PartitionKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PartitionKey({:?})", hex::encode(&self.0))
    }
}

/// Client (editor) identifier
#[derive(Clone, Copy, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct ClientId(pub u64);

impl ClientId {
    pub fn new(id: u64) -> Self {
        Self(id)
    }
}

impl fmt::Debug for ClientId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ClientId({})", self.0)
    }
}

/// Language identifier for language servers
#[derive(Clone, Debug, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub enum LanguageId {
    Rust,
    TypeScript,
    JavaScript,
    Go,
    Python,
    Other(String),
}

impl LanguageId {
    pub fn from_extension(ext: &str) -> Self {
        match ext {
            "rs" => Self::Rust,
            "ts" | "tsx" => Self::TypeScript,
            "js" | "jsx" | "mjs" | "cjs" => Self::JavaScript,
            "go" => Self::Go,
            "py" | "pyi" => Self::Python,
            other => Self::Other(other.to_string()),
        }
    }

    pub fn language_server_command(&self) -> Option<&'static str> {
        match self {
            Self::Rust => Some("rust-analyzer"),
            Self::TypeScript | Self::JavaScript => Some("typescript-language-server"),
            Self::Go => Some("gopls"),
            Self::Python => Some("pyright-langserver"),
            Self::Other(_) => None,
        }
    }
}

/// Timestamp in milliseconds since Unix epoch
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub struct Timestamp(pub u64);

impl Timestamp {
    pub fn now() -> Self {
        use std::time::{SystemTime, UNIX_EPOCH};
        let duration = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards");
        Self(duration.as_millis() as u64)
    }
}

/// File version (incremented on each change)
#[derive(Clone, Copy, Debug, Hash, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub struct FileVersion(pub u64);

impl FileVersion {
    pub fn new(version: u64) -> Self {
        Self(version)
    }

    pub fn initial() -> Self {
        Self(0)
    }

    pub fn next(&self) -> Self {
        Self(self.0 + 1)
    }
}

// Hex encoding helper (inline since it's a simple implementation)
mod hex {
    pub fn encode(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{:02x}", b)).collect()
    }
}
