// ============================================================
// Compatibility facade — settings moved to `crate::modules::settings`
// ============================================================
//
// Handlers are consumed by `crate::app::router`; the secret-source helpers are
// consumed by `crate::api::secrets`. Both keep resolving through this path
// while call sites migrate.

pub use crate::modules::settings::api::routes::{
    get_handler, update_handler, verify_provider_handler,
};

pub(crate) use crate::modules::settings::application::secret_lifecycle::{
    chat_source, embedding_source,
};
