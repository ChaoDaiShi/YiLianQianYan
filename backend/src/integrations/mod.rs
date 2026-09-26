//! Infrastructure integrations.
//!
//! Adapters to external systems (MCP servers, and later LLM providers, secret
//! stores, filesystem and other external services). Integrations implement
//! contracts defined by the domain/application layers; they do not own product
//! policy.

pub mod mcp;
