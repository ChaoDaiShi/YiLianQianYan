// ============================================================
// Capability Registry — discovery, aggregation, and snapshot.
//
// The registry aggregates descriptors from providers. It is a *read* layer
// only: it has no execute/invoke/run entry point. It refreshes atomically so a
// reader never observes a partially-updated snapshot, and it fails closed on
// duplicate ids rather than silently overwriting.
// ============================================================

use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::RwLock;
use serde::Serialize;

use super::model::{validate_descriptor, CapabilityDescriptor, CapabilityId, CapabilityKind};
use super::provider::CapabilityProvider;

/// Result of a registry refresh.
#[derive(Debug, Clone, Default, Serialize)]
pub struct RegistryRefreshReport {
    pub discovered: usize,
    pub ready: usize,
    pub unavailable: usize,
    pub duplicates: usize,
    pub provider_failures: usize,
}

pub struct CapabilityRegistry {
    providers: Vec<Arc<dyn CapabilityProvider>>,
    snapshot: RwLock<HashMap<CapabilityId, CapabilityDescriptor>>,
}

impl CapabilityRegistry {
    pub fn new(providers: Vec<Arc<dyn CapabilityProvider>>) -> Self {
        Self {
            providers,
            snapshot: RwLock::new(HashMap::new()),
        }
    }

    /// Re-discover from all providers and atomically replace the snapshot.
    /// A single provider failure does not drop the others' capabilities.
    pub async fn refresh(&self) -> RegistryRefreshReport {
        let mut next: HashMap<CapabilityId, CapabilityDescriptor> = HashMap::new();
        let mut report = RegistryRefreshReport::default();

        for provider in &self.providers {
            match provider.discover().await {
                Ok(descriptors) => {
                    for descriptor in descriptors {
                        if let Err(error) = validate_descriptor(&descriptor) {
                            tracing::warn!(
                                provider = ?provider.provider_kind(),
                                id = %descriptor.id,
                                error = %error,
                                "rejecting invalid capability descriptor"
                            );
                            report.duplicates += 1;
                            continue;
                        }
                        if next.contains_key(&descriptor.id) {
                            tracing::warn!(
                                id = %descriptor.id,
                                "duplicate capability id detected; rejecting"
                            );
                            report.duplicates += 1;
                            continue;
                        }
                        next.insert(descriptor.id.clone(), descriptor);
                    }
                }
                Err(error) => {
                    tracing::warn!(
                        provider = ?provider.provider_kind(),
                        error = %error,
                        "capability provider discovery failed"
                    );
                    report.provider_failures += 1;
                }
            }
        }

        report.discovered = next.len();
        report.ready = next
            .values()
            .filter(|d| d.status == super::model::CapabilityRuntimeStatus::Ready)
            .count();
        report.unavailable = next
            .values()
            .filter(|d| {
                matches!(
                    d.status,
                    super::model::CapabilityRuntimeStatus::Unavailable
                        | super::model::CapabilityRuntimeStatus::Misconfigured
                )
            })
            .count();

        // Atomic replace.
        *self.snapshot.write() = next;
        report
    }

    pub fn list(&self) -> Vec<CapabilityDescriptor> {
        let snapshot = self.snapshot.read();
        let mut descriptors: Vec<CapabilityDescriptor> = snapshot.values().cloned().collect();
        descriptors.sort_by(|a, b| a.id.as_str().cmp(b.id.as_str()));
        descriptors
    }

    pub fn get(&self, id: &CapabilityId) -> Option<CapabilityDescriptor> {
        self.snapshot.read().get(id).cloned()
    }

    pub fn find_by_kind(&self, kind: CapabilityKind) -> Vec<CapabilityDescriptor> {
        self.list().into_iter().filter(|d| d.kind == kind).collect()
    }

    /// Deterministic lexical search over id / name / description / tags.
    pub fn search(&self, query: &str) -> Vec<CapabilityDescriptor> {
        let query = query.trim().to_lowercase();
        if query.is_empty() {
            return self.list();
        }
        self.list()
            .into_iter()
            .filter(|d| {
                d.id.as_str().to_lowercase().contains(&query)
                    || d.name.to_lowercase().contains(&query)
                    || d.description.to_lowercase().contains(&query)
                    || d.metadata
                        .tags
                        .iter()
                        .any(|t| t.to_lowercase().contains(&query))
            })
            .collect()
    }

    /// Return the current snapshot count.
    pub fn len(&self) -> usize {
        self.snapshot.read().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
