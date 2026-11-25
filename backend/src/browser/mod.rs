//! Trusted bridge to the same visible native browser. Correlation never grants permissions.
mod credentials;
mod evidence;
mod model;
mod profile;
mod runtime;
mod visits;
pub use credentials::{BrowserCredentialEntry, BrowserCredentialInput, BrowserCredentialVault};
pub use model::*;
pub use profile::BrowserProfileContext;
pub use runtime::*;

#[cfg(test)]
mod visits_tests;
