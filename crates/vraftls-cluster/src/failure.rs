//! Failure detection

use std::time::Duration;
use tokio::time::Instant;
use vraftls_core::NodeId;

/// Failure detector using heartbeats
pub struct FailureDetector {
    /// Timeout before marking node as suspect
    suspicion_timeout: Duration,

    /// Timeout before marking node as failed
    failure_timeout: Duration,
}

impl FailureDetector {
    pub fn new(suspicion_timeout: Duration, failure_timeout: Duration) -> Self {
        Self {
            suspicion_timeout,
            failure_timeout,
        }
    }

    /// Check if a node should be marked as suspect
    pub fn is_suspect(&self, last_heartbeat: Instant) -> bool {
        last_heartbeat.elapsed() > self.suspicion_timeout
    }

    /// Check if a node should be marked as failed
    pub fn is_failed(&self, last_heartbeat: Instant) -> bool {
        last_heartbeat.elapsed() > self.failure_timeout
    }
}

impl Default for FailureDetector {
    fn default() -> Self {
        Self::new(Duration::from_secs(5), Duration::from_secs(10))
    }
}
