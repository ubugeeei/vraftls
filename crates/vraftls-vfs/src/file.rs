//! Virtual file representation

use crate::path::VfsPath;
use serde::{Deserialize, Serialize};
use vraftls_core::{FileId, FileVersion, NodeId, RaftGroupId, Timestamp};

/// Content storage mode
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum FileContent {
    /// Full content stored in memory
    Loaded(String),

    /// Content stored on disk, path to file
    OnDisk(String),

    /// Content available on a remote node
    Remote {
        node_id: NodeId,
        offset: u64,
        length: u64,
    },

    /// Content not yet loaded
    NotLoaded,
}

impl FileContent {
    /// Check if content is loaded in memory
    pub fn is_loaded(&self) -> bool {
        matches!(self, Self::Loaded(_))
    }

    /// Get content if loaded
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::Loaded(s) => Some(s),
            _ => None,
        }
    }

    /// Get content length if known
    pub fn len(&self) -> Option<usize> {
        match self {
            Self::Loaded(s) => Some(s.len()),
            Self::Remote { length, .. } => Some(*length as usize),
            _ => None,
        }
    }

    /// Check if content is empty (only valid for loaded content)
    pub fn is_empty(&self) -> bool {
        match self {
            Self::Loaded(s) => s.is_empty(),
            _ => false,
        }
    }
}

/// Checksum for file content verification
#[derive(Clone, Copy, Debug, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct Checksum(pub u64);

impl Checksum {
    /// Compute checksum from content
    pub fn compute(content: &str) -> Self {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        content.hash(&mut hasher);
        Self(hasher.finish())
    }

    /// Verify content matches this checksum
    pub fn verify(&self, content: &str) -> bool {
        Self::compute(content) == *self
    }
}

/// A file in the virtual file system
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VfsFile {
    /// Unique file identifier
    pub id: FileId,

    /// Virtual path
    pub path: VfsPath,

    /// Current version
    pub version: FileVersion,

    /// File content
    pub content: FileContent,

    /// Content checksum
    pub checksum: Checksum,

    /// Last modification timestamp
    pub last_modified: Timestamp,

    /// Raft group that owns this file
    pub owning_group: RaftGroupId,

    /// File metadata
    pub metadata: FileMetadata,
}

impl VfsFile {
    /// Create a new file with content
    pub fn new(
        id: FileId,
        path: VfsPath,
        content: String,
        owning_group: RaftGroupId,
    ) -> Self {
        let checksum = Checksum::compute(&content);
        Self {
            id,
            path,
            version: FileVersion::initial(),
            content: FileContent::Loaded(content),
            checksum,
            last_modified: Timestamp::now(),
            owning_group,
            metadata: FileMetadata::default(),
        }
    }

    /// Update file content
    pub fn update_content(&mut self, content: String) {
        self.checksum = Checksum::compute(&content);
        self.content = FileContent::Loaded(content);
        self.version = self.version.next();
        self.last_modified = Timestamp::now();
    }

    /// Get content as string if loaded
    pub fn content_str(&self) -> Option<&str> {
        self.content.as_str()
    }

    /// Check if file is loaded
    pub fn is_loaded(&self) -> bool {
        self.content.is_loaded()
    }
}

/// File metadata
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct FileMetadata {
    /// File is read-only
    pub read_only: bool,

    /// File encoding (default UTF-8)
    pub encoding: Option<String>,

    /// Line ending style
    pub line_ending: LineEnding,

    /// Custom attributes
    pub attributes: std::collections::HashMap<String, String>,
}

/// Line ending style
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub enum LineEnding {
    #[default]
    Lf,
    CrLf,
    Cr,
}

impl LineEnding {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Lf => "\n",
            Self::CrLf => "\r\n",
            Self::Cr => "\r",
        }
    }

    pub fn detect(content: &str) -> Self {
        if content.contains("\r\n") {
            Self::CrLf
        } else if content.contains('\r') {
            Self::Cr
        } else {
            Self::Lf
        }
    }
}

/// File change event
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FileChangeEvent {
    /// Type of change
    pub change_type: FileChangeType,

    /// File that changed
    pub file_id: FileId,

    /// Path of the file
    pub path: VfsPath,

    /// New version after change
    pub version: FileVersion,

    /// Timestamp of change
    pub timestamp: Timestamp,
}

/// Type of file change
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum FileChangeType {
    Created,
    Modified,
    Deleted,
    Renamed,
}
