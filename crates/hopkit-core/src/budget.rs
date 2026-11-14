//! Shared admission interfaces for future batched probes. Existing protocol throughput is unaffected.
use crate::ToolError;
use serde::Serialize;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
pub const MAX_ACTIVE_TASKS: usize = 4;
pub const MAX_SESSION_TASKS: usize = 40;
pub const MAX_PREVIEW_EVENTS: usize = 150;
#[derive(Clone, Copy, Debug, Serialize)]
pub struct Limits {
    pub default_connections: usize,
    pub task_connections: usize,
    pub global_connections: usize,
    pub default_rate: u32,
    pub global_rate: u32,
    pub max_candidates: usize,
    pub max_combinations: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            default_connections: 32,
            task_connections: 128,
            global_connections: 128,
            default_rate: 100,
            global_rate: 200,
            max_candidates: 4096,
            max_combinations: 65536,
        }
    }
}
impl Limits {
    pub fn validate_batch(
        &self,
        candidates: usize,
        ports: usize,
        concurrency: usize,
    ) -> Result<(), ToolError> {
        let combinations = candidates.checked_mul(ports).ok_or_else(invalid)?;
        if candidates == 0
            || ports == 0
            || candidates > self.max_candidates
            || combinations > self.max_combinations
            || concurrency == 0
            || concurrency > self.task_connections
            || concurrency > self.global_connections
        {
            return Err(invalid());
        }
        Ok(())
    }
}
fn invalid() -> ToolError {
    ToolError::new(
        "INVALID_ARGUMENT",
        "Probe budget exceeded / 超过探测资源预算",
    )
}
#[derive(Clone)]
pub struct ConnectionBudget {
    active: Arc<AtomicUsize>,
    limit: usize,
}
pub struct Permit {
    active: Arc<AtomicUsize>,
}
impl ConnectionBudget {
    pub fn new(limit: usize) -> Self {
        Self {
            active: Arc::new(AtomicUsize::new(0)),
            limit,
        }
    }
    /// Nonblocking: schedulers wait with their cancellation/deadline policy and retry.
    pub fn try_acquire(&self) -> Option<Permit> {
        self.active
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
                if n < self.limit { Some(n + 1) } else { None }
            })
            .ok()?;
        Some(Permit {
            active: self.active.clone(),
        })
    }
}
impl Drop for Permit {
    fn drop(&mut self) {
        self.active.fetch_sub(1, Ordering::AcqRel);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn admission_is_shared_and_released_on_drop() {
        let a = ConnectionBudget::new(1);
        let b = a.clone();
        let permit = a.try_acquire().unwrap();
        assert!(b.try_acquire().is_none());
        drop(permit);
        assert!(b.try_acquire().is_some());
    }
    #[test]
    fn rejects_overflow_and_oversized_batches_before_allocation() {
        let limits = Limits::default();
        assert!(limits.validate_batch(256, 256, 32).is_ok());
        for (hosts, ports, workers) in [
            (4097, 1, 1),
            (4096, 17, 32),
            (1, 1, 129),
            (1, 1, 0),
            (usize::MAX, 2, 1),
        ] {
            assert!(limits.validate_batch(hosts, ports, workers).is_err());
        }
    }
}
