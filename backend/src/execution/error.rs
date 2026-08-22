// ============================================================
// Execution error — safe, non-leaking error surface.
//
// Errors carry only caller-supplied descriptions. They never wrap raw
// database errors, filesystem paths, or secrets directly; the future runtime
// is responsible for mapping lower-level errors into these safe variants.
// ============================================================

use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ExecutionError {
    #[error("invalid execution context: {0}")]
    InvalidContext(String),
    #[error("execution not found: {0}")]
    NotFound(String),
    #[error("permission denied: {0}")]
    PermissionDenied(String),
    #[error("internal execution error")]
    Internal,
}
