//! Multi-level cache hierarchy

use moka::future::Cache;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use vraftls_core::{FileId, FileVersion};

/// Cache key
#[derive(Clone, Debug, Hash, Eq, PartialEq)]
pub struct CacheKey {
    pub file_id: FileId,
    pub file_version: FileVersion,
    pub cache_type: CacheType,
}

/// Type of cached data
#[derive(Clone, Debug, Hash, Eq, PartialEq)]
pub enum CacheType {
    Ast,
    Types,
    Symbols,
    Diagnostics,
    Completions,
}

/// Cached entry
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum CacheEntry {
    Ast(Vec<u8>),
    Types(Vec<u8>),
    Symbols(Vec<u8>),
    Diagnostics(Vec<u8>),
    Completions(Vec<u8>),
}

/// Multi-level cache
pub struct CacheHierarchy {
    /// L1: In-memory hot cache
    l1: Cache<CacheKey, CacheEntry>,
}

impl CacheHierarchy {
    pub fn new(max_entries: u64) -> Self {
        Self {
            l1: Cache::new(max_entries),
        }
    }

    pub async fn get(&self, key: &CacheKey) -> Option<CacheEntry> {
        self.l1.get(key).await
    }

    pub async fn insert(&self, key: CacheKey, entry: CacheEntry) {
        self.l1.insert(key, entry).await;
    }

    pub async fn invalidate(&self, key: &CacheKey) {
        self.l1.invalidate(key).await;
    }

    pub async fn clear(&self) {
        self.l1.invalidate_all();
    }
}
