// ============================================================
// Compatibility facade — Task World HTTP moved to `crate::modules::task::api`
// ============================================================
//
// Consumed by `crate::app::router`; kept so `crate::api::task_world::*` keeps
// resolving while call sites migrate.

pub use crate::modules::task::api::*;
