//! Discovery remains visible when the native host is absent; executable support
//! must describe the actual process, not merely the compiled tool names.
use crate::modules::capability::{CapabilityDescriptor, CapabilityRuntimeStatus};

pub(super) fn effective_descriptor(
    mut descriptor: CapabilityDescriptor,
    native_available: bool,
) -> CapabilityDescriptor {
    let native_tool = descriptor
        .metadata
        .source_id
        .as_deref()
        .is_some_and(|id| id.starts_with("visible_browser_"));
    if native_tool && !native_available {
        descriptor.metadata.runtime_ready = false;
        if descriptor.enabled && descriptor.status == CapabilityRuntimeStatus::Ready {
            descriptor.status = CapabilityRuntimeStatus::Unavailable;
        }
    }
    descriptor
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::capability::{
        CapabilityId, CapabilityKind, CapabilityMetadata, CapabilityProviderKind, CapabilityRisk,
    };
    fn descriptor(source: &str) -> CapabilityDescriptor {
        CapabilityDescriptor {
            id: CapabilityId::new(format!("builtin.tool.{source}")).unwrap(),
            kind: CapabilityKind::Tool,
            provider: CapabilityProviderKind::Builtin,
            name: source.into(),
            description: "native fixture".into(),
            input_schema: None,
            output_schema: None,
            risk: CapabilityRisk::High,
            permissions: vec![],
            status: CapabilityRuntimeStatus::Ready,
            enabled: true,
            metadata: CapabilityMetadata {
                source_id: Some(source.into()),
                runtime_ready: true,
                ..Default::default()
            },
        }
    }
    #[test]
    fn browser_catalogue_keeps_discovery_but_disables_execution_without_native_host() {
        let projected = effective_descriptor(descriptor("visible_browser_observe"), false);
        assert_eq!(projected.status, CapabilityRuntimeStatus::Unavailable);
        assert!(!projected.metadata.runtime_ready);
        let availability = crate::modules::capability::availability::for_descriptor(
            &projected,
            &["visible_browser_observe".into()].into(),
        );
        assert!(availability.chat.discoverable);
        assert!(!availability.chat.executable);
        assert!(!availability.personal.bindable);
    }
    #[test]
    fn browser_catalogue_host_presence_does_not_enable_disabled_or_unrelated_tools() {
        assert_eq!(
            effective_descriptor(descriptor("read_file"), false).status,
            CapabilityRuntimeStatus::Ready
        );
        assert_eq!(
            effective_descriptor(descriptor("visible_browser_observe"), true).status,
            CapabilityRuntimeStatus::Ready
        );
        let mut disabled = descriptor("visible_browser_observe");
        disabled.enabled = false;
        disabled.status = CapabilityRuntimeStatus::Disabled;
        assert_eq!(
            effective_descriptor(disabled, false).status,
            CapabilityRuntimeStatus::Disabled
        );
    }
}
