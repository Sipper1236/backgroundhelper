//! Validation traits and errors.

use thiserror::Error;
use serde::{Deserialize, Serialize};

/// Validation error types.
#[derive(Debug, Clone, Error, Serialize, Deserialize)]
pub enum ValidationError {
    #[error("Invalid field: {0}")]
    InvalidField(String),
    #[error("Invalid state transition: {from} -> {to}")]
    InvalidStateTransition { from: String, to: String },
    #[error("Missing required field: {0}")]
    MissingField(String),
    #[error("Value out of range: {0}")]
    OutOfRange(String),
    #[error("Schema validation failed: {0}")]
    SchemaError(String),
    #[error("Protocol version mismatch: expected {expected}, got {actual}")]
    VersionMismatch { expected: String, actual: String },
    #[error("Message validation failed: {0}")]
    MessageError(String),
}

/// Trait for types that can be validated.
pub trait Validatable {
    fn validate(&self) -> Result<(), ValidationError>;
}

/// Validate a JSON value against a schema (placeholder for future JSON Schema validation).
pub fn validate_json_schema<T: Validatable + ?Sized>(value: &T) -> Result<(), ValidationError> {
    value.validate()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::job::{JobSpec, WorkloadType, JobPriority};
    use crate::resource::ResourceLimits;

    #[test]
    fn test_job_spec_validation() {
        let spec = JobSpec::new(WorkloadType::RangeSum { start: 0, end: 100 }, JobPriority::Normal, 300);
        assert!(spec.validate().is_ok());

        let spec = JobSpec::new(WorkloadType::RangeSum { start: 100, end: 0 }, JobPriority::Normal, 300);
        assert!(spec.validate().is_err());

        let spec = JobSpec::new(WorkloadType::RangeSum { start: 0, end: 2_000_000_000 }, JobPriority::Normal, 300);
        assert!(spec.validate().is_err());
    }

    #[test]
    fn test_resource_limits_validation() {
        let limits = ResourceLimits::default();
        assert!(limits.validate().is_ok());

        let mut limits = ResourceLimits::default();
        limits.cpu_max_utilization_pct = Some(0);
        assert!(limits.validate().is_err());

        limits.cpu_max_utilization_pct = Some(101);
        assert!(limits.validate().is_err());

        limits.cpu_max_utilization_pct = Some(50);
        limits.gpu_max_memory_pct = Some(150);
        assert!(limits.validate().is_err());
    }
}