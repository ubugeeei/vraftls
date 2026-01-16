//! Error types for VRaftLS

use crate::types::{FileId, NodeId, RaftGroupId};
use thiserror::Error;

/// Main error type for VRaftLS
#[derive(Error, Debug)]
pub enum VRaftError {
    // Raft errors
    #[error("not leader, current leader is {leader:?}")]
    NotLeader { leader: Option<NodeId> },

    #[error("raft group not found: {0}")]
    GroupNotFound(RaftGroupId),

    #[error("raft consensus failed: {0}")]
    RaftConsensus(String),

    #[error("raft log error: {0}")]
    RaftLog(String),

    // VFS errors
    #[error("file not found: {0:?}")]
    FileNotFound(FileId),

    #[error("file already exists: {0}")]
    FileExists(String),

    #[error("invalid path: {0}")]
    InvalidPath(String),

    #[error("path not in workspace: {0}")]
    PathNotInWorkspace(String),

    // Network errors
    #[error("node unreachable: {0}")]
    NodeUnreachable(NodeId),

    #[error("network timeout")]
    Timeout,

    #[error("connection failed: {0}")]
    ConnectionFailed(String),

    // LSP errors
    #[error("language server error: {0}")]
    LanguageServer(String),

    #[error("unsupported language: {0}")]
    UnsupportedLanguage(String),

    #[error("language server not running")]
    LanguageServerNotRunning,

    #[error("invalid LSP request: {0}")]
    InvalidLspRequest(String),

    // Storage errors
    #[error("storage error: {0}")]
    Storage(String),

    #[error("serialization error: {0}")]
    Serialization(String),

    // Transaction errors
    #[error("transaction aborted: {0}")]
    TransactionAborted(String),

    #[error("transaction timeout")]
    TransactionTimeout,

    // Internal errors
    #[error("internal error: {0}")]
    Internal(String),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

impl VRaftError {
    pub fn is_retriable(&self) -> bool {
        matches!(
            self,
            Self::NotLeader { .. }
                | Self::NodeUnreachable(_)
                | Self::Timeout
                | Self::TransactionTimeout
        )
    }

    pub fn is_not_leader(&self) -> bool {
        matches!(self, Self::NotLeader { .. })
    }
}

/// Result type alias for VRaftLS
pub type Result<T> = std::result::Result<T, VRaftError>;
