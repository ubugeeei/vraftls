//! VFS commands for Raft log entries

use crate::path::VfsPath;
use serde::{Deserialize, Serialize};
use vraftls_core::FileId;

/// Commands that can be applied to the VFS state machine
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum VfsCommand {
    /// Create a new file
    CreateFile {
        path: VfsPath,
        content: String,
    },

    /// Update an existing file
    UpdateFile {
        file_id: FileId,
        content: String,
        expected_version: Option<u64>,
    },

    /// Delete a file
    DeleteFile {
        file_id: FileId,
    },

    /// Rename/move a file
    RenameFile {
        file_id: FileId,
        new_path: VfsPath,
    },

    /// Batch create/update files
    BatchWrite {
        operations: Vec<BatchWriteOp>,
    },

    /// Invalidate cached analysis for files
    InvalidateCache {
        file_ids: Vec<FileId>,
    },
}

/// Operation in a batch write
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum BatchWriteOp {
    Create { path: VfsPath, content: String },
    Update { file_id: FileId, content: String },
    Delete { file_id: FileId },
}

/// Response from VFS command execution
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum VfsResponse {
    /// Success with optional file ID
    Ok(Option<FileId>),

    /// Created file with ID
    Created(FileId),

    /// Batch operation results
    BatchResults(Vec<VfsBatchResult>),

    /// Error occurred
    Error(VfsCommandError),
}

/// Result of a single batch operation
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum VfsBatchResult {
    Success(Option<FileId>),
    Error(VfsCommandError),
}

/// Error from VFS command execution
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum VfsCommandError {
    FileNotFound(FileId),
    FileAlreadyExists(String),
    VersionMismatch { expected: u64, actual: u64 },
    InvalidPath(String),
    StorageError(String),
}

impl std::fmt::Display for VfsCommandError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::FileNotFound(id) => write!(f, "file not found: {:?}", id),
            Self::FileAlreadyExists(path) => write!(f, "file already exists: {}", path),
            Self::VersionMismatch { expected, actual } => {
                write!(f, "version mismatch: expected {}, got {}", expected, actual)
            }
            Self::InvalidPath(path) => write!(f, "invalid path: {}", path),
            Self::StorageError(msg) => write!(f, "storage error: {}", msg),
        }
    }
}

impl std::error::Error for VfsCommandError {}

/// Query for reading VFS state (not replicated)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum VfsQuery {
    /// Get file by ID
    GetFile(FileId),

    /// Get file by path
    GetFileByPath(VfsPath),

    /// List files in directory
    ListDirectory(VfsPath),

    /// Find files matching pattern
    FindFiles(String),

    /// Get file content
    GetContent(FileId),
}

/// Response from VFS query
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum VfsQueryResponse {
    /// Single file
    File(Option<crate::file::VfsFile>),

    /// List of files
    Files(Vec<crate::file::VfsFile>),

    /// File content
    Content(Option<String>),

    /// Error
    Error(String),
}
