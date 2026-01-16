//! Cache invalidation

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::RwLock;
use vraftls_core::FileId;

/// Dependency graph for cache invalidation
pub struct DependencyGraph {
    /// File -> files that depend on it
    dependents: RwLock<HashMap<FileId, HashSet<FileId>>>,

    /// File -> files it depends on
    dependencies: RwLock<HashMap<FileId, HashSet<FileId>>>,
}

impl DependencyGraph {
    pub fn new() -> Self {
        Self {
            dependents: RwLock::new(HashMap::new()),
            dependencies: RwLock::new(HashMap::new()),
        }
    }

    /// Add a dependency: `dependent` depends on `dependency`
    pub async fn add_dependency(&self, dependent: FileId, dependency: FileId) {
        {
            let mut deps = self.dependents.write().await;
            deps.entry(dependency)
                .or_insert_with(HashSet::new)
                .insert(dependent);
        }
        {
            let mut deps = self.dependencies.write().await;
            deps.entry(dependent)
                .or_insert_with(HashSet::new)
                .insert(dependency);
        }
    }

    /// Get all files that depend on the given file (transitively)
    pub async fn transitive_dependents(&self, file_id: FileId) -> HashSet<FileId> {
        let mut result = HashSet::new();
        let mut to_visit = vec![file_id];

        let dependents = self.dependents.read().await;

        while let Some(current) = to_visit.pop() {
            if let Some(deps) = dependents.get(&current) {
                for dep in deps {
                    if result.insert(*dep) {
                        to_visit.push(*dep);
                    }
                }
            }
        }

        result
    }

    /// Remove all dependencies for a file
    pub async fn remove_file(&self, file_id: FileId) {
        // Remove as dependent
        {
            let deps_snapshot = {
                let deps = self.dependencies.read().await;
                deps.get(&file_id).cloned()
            };

            if let Some(deps) = deps_snapshot {
                let mut dependents = self.dependents.write().await;
                for dep in deps {
                    if let Some(set) = dependents.get_mut(&dep) {
                        set.remove(&file_id);
                    }
                }
            }
        }

        // Remove as dependency
        {
            let mut dependents = self.dependents.write().await;
            dependents.remove(&file_id);
        }

        {
            let mut dependencies = self.dependencies.write().await;
            dependencies.remove(&file_id);
        }
    }
}

impl Default for DependencyGraph {
    fn default() -> Self {
        Self::new()
    }
}
