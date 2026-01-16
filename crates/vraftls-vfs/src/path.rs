//! Virtual file path handling

use serde::{Deserialize, Serialize};
use std::path::{Component, Path, PathBuf};
use vraftls_core::{ClientId, LanguageId, PartitionKey};

/// A normalized virtual file path
#[derive(Clone, Debug, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct VfsPath {
    /// Client that owns this path (for multi-client scenarios)
    client_id: Option<ClientId>,

    /// Normalized path components
    components: Vec<String>,

    /// Original path string
    original: String,
}

impl VfsPath {
    /// Create a new VfsPath from a string
    pub fn new(path: impl AsRef<str>) -> Self {
        let path_str = path.as_ref();
        let normalized = Self::normalize(path_str);
        Self {
            client_id: None,
            components: normalized,
            original: path_str.to_string(),
        }
    }

    /// Create a VfsPath with a client ID
    pub fn with_client(path: impl AsRef<str>, client_id: ClientId) -> Self {
        let mut vfs_path = Self::new(path);
        vfs_path.client_id = Some(client_id);
        vfs_path
    }

    /// Normalize a path string into components
    fn normalize(path: &str) -> Vec<String> {
        let path = Path::new(path);
        let mut components = Vec::new();

        for component in path.components() {
            match component {
                Component::Normal(c) => {
                    if let Some(s) = c.to_str() {
                        components.push(s.to_string());
                    }
                }
                Component::RootDir => {
                    // Keep track of absolute paths by adding empty string
                    components.clear();
                }
                Component::ParentDir => {
                    // Handle ..
                    components.pop();
                }
                Component::CurDir => {
                    // Ignore .
                }
                Component::Prefix(_) => {
                    // Windows prefix, ignore for now
                }
            }
        }

        components
    }

    /// Get the path components
    pub fn components(&self) -> &[String] {
        &self.components
    }

    /// Get the original path string
    pub fn as_str(&self) -> &str {
        &self.original
    }

    /// Get the client ID if set
    pub fn client_id(&self) -> Option<ClientId> {
        self.client_id
    }

    /// Get the file name (last component)
    pub fn file_name(&self) -> Option<&str> {
        self.components.last().map(|s| s.as_str())
    }

    /// Get the file extension
    pub fn extension(&self) -> Option<&str> {
        self.file_name()
            .and_then(|name| name.rsplit_once('.'))
            .map(|(_, ext)| ext)
    }

    /// Get the parent path
    pub fn parent(&self) -> Option<VfsPath> {
        if self.components.len() <= 1 {
            return None;
        }
        let parent_components = &self.components[..self.components.len() - 1];
        Some(VfsPath {
            client_id: self.client_id,
            components: parent_components.to_vec(),
            original: parent_components.join("/"),
        })
    }

    /// Join with another path
    pub fn join(&self, other: impl AsRef<str>) -> VfsPath {
        let other_path = Self::new(other);
        let mut new_components = self.components.clone();
        new_components.extend(other_path.components.into_iter());

        VfsPath {
            client_id: self.client_id,
            components: new_components.clone(),
            original: new_components.join("/"),
        }
    }

    /// Convert to a PathBuf
    pub fn to_path_buf(&self) -> PathBuf {
        if self.original.starts_with('/') {
            PathBuf::from("/").join(self.components.join("/"))
        } else {
            PathBuf::from(self.components.join("/"))
        }
    }

    /// Get the language ID based on file extension
    pub fn language_id(&self) -> Option<LanguageId> {
        self.extension().map(LanguageId::from_extension)
    }

    /// Compute partition key for consistent hashing
    pub fn partition_key(&self) -> PartitionKey {
        PartitionKey::from_path(&self.original)
    }

    /// Check if this path is a child of another path
    pub fn starts_with(&self, other: &VfsPath) -> bool {
        if self.components.len() < other.components.len() {
            return false;
        }
        self.components
            .iter()
            .zip(other.components.iter())
            .all(|(a, b)| a == b)
    }
}

impl std::fmt::Display for VfsPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.original)
    }
}

impl From<&str> for VfsPath {
    fn from(s: &str) -> Self {
        Self::new(s)
    }
}

impl From<String> for VfsPath {
    fn from(s: String) -> Self {
        Self::new(s)
    }
}

impl From<&Path> for VfsPath {
    fn from(p: &Path) -> Self {
        Self::new(p.to_string_lossy())
    }
}

impl From<PathBuf> for VfsPath {
    fn from(p: PathBuf) -> Self {
        Self::new(p.to_string_lossy())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_path() {
        let path = VfsPath::new("/foo/bar/../baz");
        assert_eq!(path.components(), &["foo", "baz"]);
    }

    #[test]
    fn test_file_name() {
        let path = VfsPath::new("/project/src/main.rs");
        assert_eq!(path.file_name(), Some("main.rs"));
    }

    #[test]
    fn test_extension() {
        let path = VfsPath::new("/project/src/main.rs");
        assert_eq!(path.extension(), Some("rs"));
    }

    #[test]
    fn test_language_id() {
        let path = VfsPath::new("/project/src/main.rs");
        assert_eq!(path.language_id(), Some(LanguageId::Rust));

        let path = VfsPath::new("/project/src/index.ts");
        assert_eq!(path.language_id(), Some(LanguageId::TypeScript));
    }

    #[test]
    fn test_join() {
        let base = VfsPath::new("/project");
        let joined = base.join("src/main.rs");
        assert_eq!(joined.components(), &["project", "src", "main.rs"]);
    }
}
