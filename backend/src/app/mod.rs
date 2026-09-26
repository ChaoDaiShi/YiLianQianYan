// ============================================================
// Application composition root
// ============================================================
//
// Owns the long-lived application state, the assembly sequence that builds it,
// and the serving lifecycle. Transport and domain modules may depend on
// `app::state`; this module must never depend on them in reverse.

pub mod bootstrap;
pub mod lifecycle;
pub mod router;
pub mod state;
