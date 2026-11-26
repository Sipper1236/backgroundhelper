//! Job specifications, types, and state machine.

use crate::validation::{Validatable, ValidationError};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Unique identifier for a job.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct JobId(pub Uuid);

impl JobId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for JobId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for JobId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Unique identifier for a worker.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct WorkerId(pub Uuid);

impl WorkerId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl std::fmt::Display for WorkerId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Supported workload types for v0.1 (deterministic CPU-only).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WorkloadType {
    /// Sum a range of integers: sum(start..end)
    RangeSum { start: u64, end: u64 },
    /// Compute a tile of a matrix multiplication: C = A * B
    MatrixTile {
        /// Matrix dimension (N x N)
        size: usize,
        /// Tile coordinates (row, col) and tile size
        tile: TileSpec,
    },
    /// Count primes in a range
    PrimeCount { start: u64, end: u64 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TileSpec {
    pub row: usize,
    pub col: usize,
    pub tile_size: usize,
}

impl WorkloadType {
    /// Estimated CPU cycles for this workload (rough heuristic for scheduling).
    pub fn estimated_cycles(&self) -> u64 {
        match self {
            WorkloadType::RangeSum { start, end } => {
                let range = if *end > *start { end - start } else { 1 };
                range.max(1)
            }
            WorkloadType::MatrixTile { size, tile } => {
                (size / tile.tile_size).max(1) as u64 * (size / tile.tile_size).max(1) as u64 * 1000
            }
            WorkloadType::PrimeCount { start, end } => {
                let range = if *end > *start { end - start } else { 1 };
                range.max(1) * 100
            }
        }
    }

    /// Estimated memory in bytes for this workload.
    pub fn estimated_memory_bytes(&self) -> u64 {
        match self {
            WorkloadType::RangeSum { .. } => 1024, // Minimal
            WorkloadType::MatrixTile { size, .. } => {
                (size * size * 8) as u64 * 3 // Three matrices (A, B, C tile)
            }
            WorkloadType::PrimeCount { .. } => 1024,
        }
    }
}

/// Job priority levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum JobPriority {
    Low = 0,
    Normal = 1,
    High = 2,
    Critical = 3,
}

impl Default for JobPriority {
    fn default() -> Self {
        Self::Normal
    }
}

/// Job state machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum JobState {
    Queued,
    Offered,
    Approved,
    Running,
    Completed,
    Failed,
    Cancelled,
    Expired,
}

impl JobState {
    /// Valid state transitions.
    pub fn can_transition_to(&self, next: JobState) -> bool {
        use JobState::*;
        match (self, next) {
            (Queued, Offered) | (Queued, Cancelled) | (Queued, Expired) => true,
            (Offered, Approved) | (Offered, Cancelled) | (Offered, Expired) => true,
            (Approved, Running) | (Approved, Cancelled) => true,
            (Running, Completed) | (Running, Failed) | (Running, Cancelled) => true,
            (Completed, _) => false, // Terminal
            (Failed, _) => false,    // Terminal
            (Cancelled, _) => false, // Terminal
            (Expired, _) => false,   // Terminal
            _ => false,              // Exhaustive - all other transitions invalid
        }
    }

    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            JobState::Completed | JobState::Failed | JobState::Cancelled | JobState::Expired
        )
    }

    pub fn is_active(&self) -> bool {
        matches!(
            self,
            JobState::Offered | JobState::Approved | JobState::Running
        )
    }
}

/// Complete job specification submitted by the coordinator.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobSpec {
    pub id: JobId,
    pub workload: WorkloadType,
    pub priority: JobPriority,
    pub max_duration_secs: u32,
    pub estimated_cycles: u64,
    pub estimated_memory_bytes: u64,
    pub created_at: DateTime<Utc>,
    pub metadata: HashMap<String, String>,
}

impl JobSpec {
    pub fn new(workload: WorkloadType, priority: JobPriority, max_duration_secs: u32) -> Self {
        let estimated_cycles = workload.estimated_cycles();
        let estimated_memory_bytes = workload.estimated_memory_bytes();
        Self {
            id: JobId::new(),
            workload,
            priority,
            max_duration_secs,
            estimated_cycles,
            estimated_memory_bytes,
            created_at: Utc::now(),
            metadata: HashMap::new(),
        }
    }
}

/// Result of a completed job.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobResult {
    pub job_id: JobId,
    pub worker_id: WorkerId,
    pub output: JobOutput,
    pub started_at: DateTime<Utc>,
    pub completed_at: DateTime<Utc>,
    pub cpu_cycles_used: u64,
    pub peak_memory_bytes: u64,
}

/// Job output data (typed per workload).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum JobOutput {
    RangeSum {
        sum: u128,
    },
    MatrixTile {
        tile_data: Vec<f64>,
        rows: usize,
        cols: usize,
    },
    PrimeCount {
        count: u64,
    },
}

impl JobOutput {
    /// Verify the result against expected output (for testing/simulation).
    pub fn verify(&self, expected: &JobOutput) -> bool {
        std::mem::discriminant(self) == std::mem::discriminant(expected) && self == expected
    }
}

/// Full job object tracked by the coordinator.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job {
    pub spec: JobSpec,
    pub state: JobState,
    pub assigned_worker: Option<WorkerId>,
    pub result: Option<JobResult>,
    pub offered_at: Option<DateTime<Utc>>,
    pub approved_at: Option<DateTime<Utc>>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub error_message: Option<String>,
    pub retry_count: u32,
}

impl Job {
    pub fn new(spec: JobSpec) -> Self {
        Self {
            spec,
            state: JobState::Queued,
            assigned_worker: None,
            result: None,
            offered_at: None,
            approved_at: None,
            started_at: None,
            completed_at: None,
            error_message: None,
            retry_count: 0,
        }
    }

    pub fn transition_to(&mut self, new_state: JobState) -> Result<(), ValidationError> {
        if !self.state.can_transition_to(new_state) {
            return Err(ValidationError::InvalidStateTransition {
                from: format!("{:?}", self.state),
                to: format!("{:?}", new_state),
            });
        }
        self.state = new_state;
        let now = Utc::now();
        match new_state {
            JobState::Offered => self.offered_at = Some(now),
            JobState::Approved => self.approved_at = Some(now),
            JobState::Running => self.started_at = Some(now),
            JobState::Completed | JobState::Failed | JobState::Cancelled | JobState::Expired => {
                self.completed_at = Some(now);
            }
            _ => {}
        }
        Ok(())
    }
}

impl Validatable for JobSpec {
    fn validate(&self) -> Result<(), ValidationError> {
        if self.max_duration_secs == 0 {
            return Err(ValidationError::InvalidField(
                "max_duration_secs must be > 0".into(),
            ));
        }
        if self.estimated_cycles == 0 {
            return Err(ValidationError::InvalidField(
                "estimated_cycles must be > 0".into(),
            ));
        }
        // Validate workload-specific constraints
        match &self.workload {
            WorkloadType::RangeSum { start, end } => {
                if start >= end {
                    return Err(ValidationError::InvalidField(
                        "RangeSum: start must be < end".into(),
                    ));
                }
                if end - start > 1_000_000_000 {
                    return Err(ValidationError::InvalidField(
                        "RangeSum: range too large (max 1B)".into(),
                    ));
                }
            }
            WorkloadType::MatrixTile { size, tile } => {
                if *size == 0 || *size > 10_000 {
                    return Err(ValidationError::InvalidField(
                        "MatrixTile: size must be 1..10000".into(),
                    ));
                }
                if tile.tile_size == 0 || tile.tile_size > *size {
                    return Err(ValidationError::InvalidField(
                        "MatrixTile: invalid tile_size".into(),
                    ));
                }
                if tile.row >= *size || tile.col >= *size {
                    return Err(ValidationError::InvalidField(
                        "MatrixTile: tile coordinates out of bounds".into(),
                    ));
                }
            }
            WorkloadType::PrimeCount { start, end } => {
                if start >= end {
                    return Err(ValidationError::InvalidField(
                        "PrimeCount: start must be < end".into(),
                    ));
                }
                if end - start > 10_000_000 {
                    return Err(ValidationError::InvalidField(
                        "PrimeCount: range too large (max 10M)".into(),
                    ));
                }
            }
        }
        Ok(())
    }
}
