//! Virtual File System implementation

use crate::commands::{VfsCommand, VfsCommandError, VfsResponse};
use crate::file::{FileChangeEvent, FileChangeType, VfsFile};
use crate::path::VfsPath;
use dashmap::DashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::broadcast;
use vraftls_core::{FileId, RaftGroupId, Result, Timestamp, VRaftError};

/// In-memory Virtual File System
pub struct Vfs {
    /// Files indexed by ID
    files: DashMap<FileId, VfsFile>,

    /// Path to file ID mapping
    path_index: DashMap<VfsPath, FileId>,

    /// Next file ID counter
    next_file_id: AtomicU64,

    /// Raft group this VFS belongs to
    group_id: RaftGroupId,

    /// File change event broadcaster
    change_tx: broadcast::Sender<FileChangeEvent>,
}

impl Vfs {
    /// Create a new VFS for a Raft group
    pub fn new(group_id: RaftGroupId) -> Self {
        let (change_tx, _) = broadcast::channel(1024);
        Self {
            files: DashMap::new(),
            path_index: DashMap::new(),
            next_file_id: AtomicU64::new(1),
            group_id,
            change_tx,
        }
    }

    /// Subscribe to file change events
    pub fn subscribe(&self) -> broadcast::Receiver<FileChangeEvent> {
        self.change_tx.subscribe()
    }

    /// Apply a VFS command (used by Raft state machine)
    pub fn apply(&self, command: VfsCommand) -> VfsResponse {
        match command {
            VfsCommand::CreateFile { path, content } => self.create_file(path, content),
            VfsCommand::UpdateFile {
                file_id,
                content,
                expected_version,
            } => self.update_file(file_id, content, expected_version),
            VfsCommand::DeleteFile { file_id } => self.delete_file(file_id),
            VfsCommand::RenameFile { file_id, new_path } => self.rename_file(file_id, new_path),
            VfsCommand::BatchWrite { operations } => self.batch_write(operations),
            VfsCommand::InvalidateCache { file_ids } => {
                // Cache invalidation is handled externally
                VfsResponse::Ok(None)
            }
        }
    }

    /// Create a new file
    fn create_file(&self, path: VfsPath, content: String) -> VfsResponse {
        // Check if file already exists
        if self.path_index.contains_key(&path) {
            return VfsResponse::Error(VfsCommandError::FileAlreadyExists(path.to_string()));
        }

        let file_id = FileId::new(self.next_file_id.fetch_add(1, Ordering::SeqCst));
        let file = VfsFile::new(file_id, path.clone(), content, self.group_id);

        self.files.insert(file_id, file);
        self.path_index.insert(path.clone(), file_id);

        // Emit change event
        let _ = self.change_tx.send(FileChangeEvent {
            change_type: FileChangeType::Created,
            file_id,
            path,
            version: vraftls_core::FileVersion::initial(),
            timestamp: Timestamp::now(),
        });

        VfsResponse::Created(file_id)
    }

    /// Update an existing file
    fn update_file(
        &self,
        file_id: FileId,
        content: String,
        expected_version: Option<u64>,
    ) -> VfsResponse {
        let mut file = match self.files.get_mut(&file_id) {
            Some(f) => f,
            None => return VfsResponse::Error(VfsCommandError::FileNotFound(file_id)),
        };

        // Check version if specified
        if let Some(expected) = expected_version {
            if file.version.0 != expected {
                return VfsResponse::Error(VfsCommandError::VersionMismatch {
                    expected,
                    actual: file.version.0,
                });
            }
        }

        let path = file.path.clone();
        file.update_content(content);
        let version = file.version;

        // Emit change event
        let _ = self.change_tx.send(FileChangeEvent {
            change_type: FileChangeType::Modified,
            file_id,
            path,
            version,
            timestamp: Timestamp::now(),
        });

        VfsResponse::Ok(Some(file_id))
    }

    /// Delete a file
    fn delete_file(&self, file_id: FileId) -> VfsResponse {
        let file = match self.files.remove(&file_id) {
            Some((_, f)) => f,
            None => return VfsResponse::Error(VfsCommandError::FileNotFound(file_id)),
        };

        self.path_index.remove(&file.path);

        // Emit change event
        let _ = self.change_tx.send(FileChangeEvent {
            change_type: FileChangeType::Deleted,
            file_id,
            path: file.path,
            version: file.version,
            timestamp: Timestamp::now(),
        });

        VfsResponse::Ok(None)
    }

    /// Rename a file
    fn rename_file(&self, file_id: FileId, new_path: VfsPath) -> VfsResponse {
        // Check if new path already exists
        if self.path_index.contains_key(&new_path) {
            return VfsResponse::Error(VfsCommandError::FileAlreadyExists(new_path.to_string()));
        }

        let mut file = match self.files.get_mut(&file_id) {
            Some(f) => f,
            None => return VfsResponse::Error(VfsCommandError::FileNotFound(file_id)),
        };

        let old_path = file.path.clone();
        self.path_index.remove(&old_path);

        file.path = new_path.clone();
        file.last_modified = Timestamp::now();

        self.path_index.insert(new_path.clone(), file_id);

        // Emit change event
        let _ = self.change_tx.send(FileChangeEvent {
            change_type: FileChangeType::Renamed,
            file_id,
            path: new_path,
            version: file.version,
            timestamp: Timestamp::now(),
        });

        VfsResponse::Ok(Some(file_id))
    }

    /// Batch write operations
    fn batch_write(&self, operations: Vec<crate::commands::BatchWriteOp>) -> VfsResponse {
        use crate::commands::{BatchWriteOp, VfsBatchResult};

        let results: Vec<VfsBatchResult> = operations
            .into_iter()
            .map(|op| match op {
                BatchWriteOp::Create { path, content } => match self.create_file(path, content) {
                    VfsResponse::Created(id) => VfsBatchResult::Success(Some(id)),
                    VfsResponse::Error(e) => VfsBatchResult::Error(e),
                    _ => VfsBatchResult::Success(None),
                },
                BatchWriteOp::Update { file_id, content } => {
                    match self.update_file(file_id, content, None) {
                        VfsResponse::Ok(id) => VfsBatchResult::Success(id),
                        VfsResponse::Error(e) => VfsBatchResult::Error(e),
                        _ => VfsBatchResult::Success(None),
                    }
                }
                BatchWriteOp::Delete { file_id } => match self.delete_file(file_id) {
                    VfsResponse::Ok(_) => VfsBatchResult::Success(None),
                    VfsResponse::Error(e) => VfsBatchResult::Error(e),
                    _ => VfsBatchResult::Success(None),
                },
            })
            .collect();

        VfsResponse::BatchResults(results)
    }

    // Query methods (not replicated)

    /// Get a file by ID
    pub fn get_file(&self, file_id: FileId) -> Option<VfsFile> {
        self.files.get(&file_id).map(|f| f.clone())
    }

    /// Get a file by path
    pub fn get_file_by_path(&self, path: &VfsPath) -> Option<VfsFile> {
        self.path_index
            .get(path)
            .and_then(|id| self.files.get(&id).map(|f| f.clone()))
    }

    /// Get file content
    pub fn get_content(&self, file_id: FileId) -> Result<String> {
        let file = self
            .files
            .get(&file_id)
            .ok_or(VRaftError::FileNotFound(file_id))?;

        file.content_str()
            .map(|s| s.to_string())
            .ok_or_else(|| VRaftError::Internal("content not loaded".to_string()))
    }

    /// List all files in a directory
    pub fn list_directory(&self, path: &VfsPath) -> Vec<VfsFile> {
        self.files
            .iter()
            .filter(|entry| entry.path.starts_with(path))
            .map(|entry| entry.value().clone())
            .collect()
    }

    /// Find files matching a pattern (simple glob)
    pub fn find_files(&self, pattern: &str) -> Vec<VfsFile> {
        // Simple pattern matching (just contains for now)
        self.files
            .iter()
            .filter(|entry| entry.path.as_str().contains(pattern))
            .map(|entry| entry.value().clone())
            .collect()
    }

    /// Get total file count
    pub fn file_count(&self) -> usize {
        self.files.len()
    }

    /// Get all file IDs
    pub fn all_file_ids(&self) -> Vec<FileId> {
        self.files.iter().map(|entry| *entry.key()).collect()
    }
}

/// Thread-safe VFS handle
pub type VfsHandle = Arc<Vfs>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_and_get_file() {
        let vfs = Vfs::new(RaftGroupId::new(1));

        let path = VfsPath::new("/test.rs");
        let content = "fn main() {}".to_string();

        let response = vfs.apply(VfsCommand::CreateFile {
            path: path.clone(),
            content: content.clone(),
        });

        let file_id = match response {
            VfsResponse::Created(id) => id,
            _ => panic!("expected Created response"),
        };

        let file = vfs.get_file(file_id).expect("file should exist");
        assert_eq!(file.content_str(), Some(content.as_str()));
    }

    #[test]
    fn test_update_file() {
        let vfs = Vfs::new(RaftGroupId::new(1));

        let path = VfsPath::new("/test.rs");
        let file_id = match vfs.apply(VfsCommand::CreateFile {
            path: path.clone(),
            content: "fn main() {}".to_string(),
        }) {
            VfsResponse::Created(id) => id,
            _ => panic!("expected Created"),
        };

        let new_content = "fn main() { println!(\"hello\"); }".to_string();
        vfs.apply(VfsCommand::UpdateFile {
            file_id,
            content: new_content.clone(),
            expected_version: Some(0),
        });

        let file = vfs.get_file(file_id).unwrap();
        assert_eq!(file.content_str(), Some(new_content.as_str()));
        assert_eq!(file.version.0, 1);
    }

    #[test]
    fn test_delete_file() {
        let vfs = Vfs::new(RaftGroupId::new(1));

        let path = VfsPath::new("/test.rs");
        let file_id = match vfs.apply(VfsCommand::CreateFile {
            path,
            content: "test".to_string(),
        }) {
            VfsResponse::Created(id) => id,
            _ => panic!("expected Created"),
        };

        vfs.apply(VfsCommand::DeleteFile { file_id });

        assert!(vfs.get_file(file_id).is_none());
    }
}
