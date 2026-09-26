// ============================================================
// Compatibility facade — Workflow HTTP moved to `crate::modules::workflow::api`
// ============================================================
//
// Consumed by `crate::app::router` and `crate::api::task_world`; kept so
// `crate::api::workflow_runtime::*` keeps resolving while call sites migrate.

pub use crate::modules::workflow::api::*;
