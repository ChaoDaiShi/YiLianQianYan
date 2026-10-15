use std::collections::HashMap;

use yilian_backend::modules::{
    capability::model::{
        CapabilityDescriptor, CapabilityId, CapabilityKind, CapabilityMetadata,
        CapabilityProviderKind, CapabilityRisk, CapabilityRuntimeStatus,
    },
    task::proofloop::{
        enhancement_contracts::RouteScore,
        routing::{build_ranked_snapshot, rank_capabilities, route_total},
    },
};

fn descriptor(id: &str, name: &str, description: &str, enabled: bool) -> CapabilityDescriptor {
    CapabilityDescriptor {
        id: CapabilityId::new(id).unwrap(),
        kind: CapabilityKind::McpTool,
        provider: CapabilityProviderKind::Mcp,
        name: name.into(),
        description: description.into(),
        input_schema: Some(serde_json::json!({
            "type": "object",
            "properties": { "title": { "type": "string" } }
        })),
        output_schema: None,
        risk: CapabilityRisk::Low,
        permissions: Vec::new(),
        status: if enabled {
            CapabilityRuntimeStatus::Ready
        } else {
            CapabilityRuntimeStatus::Disabled
        },
        enabled,
        metadata: CapabilityMetadata {
            runtime_ready: enabled,
            ..CapabilityMetadata::default()
        },
    }
}

#[test]
fn missing_history_is_normalized_instead_of_counted_as_zero_success() {
    assert_eq!(route_total(10_000, 10_000, None), 10_000);
    assert_eq!(route_total(10_000, 10_000, Some(0)), 8_500);
}

#[test]
fn relevant_capability_after_first_hundred_is_ranked_and_disabled_is_omitted() {
    let mut descriptors: Vec<_> = (0..149)
        .map(|index| {
            descriptor(
                &format!("mcp.unrelated-{index:03}"),
                "weather lookup",
                "查找天气预报",
                true,
            )
        })
        .collect();
    descriptors.push(descriptor(
        "mcp.report-csv",
        "CSV report writer",
        "生成报告文件并写入 title 字段",
        true,
    ));
    descriptors.push(descriptor(
        "mcp.disabled-report",
        "CSV report writer",
        "生成CSV报告",
        false,
    ));

    let ranked = rank_capabilities(
        "生成CSV报告并包含 title",
        &descriptors,
        None,
        &HashMap::new(),
    );

    assert_eq!(
        ranked.first().map(|score| score.capability_id.as_str()),
        Some("mcp.report-csv")
    );
    assert_eq!(ranked.len(), 151);
    assert_eq!(ranked.last().unwrap().capability_id, "mcp.disabled-report");
    assert!(ranked
        .last()
        .unwrap()
        .reasons
        .iter()
        .any(|reason| reason == "excluded:not_ready"));
}

#[test]
fn ranked_snapshot_skips_a_large_entry_and_keeps_later_ranked_capabilities() {
    let mut large = descriptor("mcp.large", "Large schema", "An oversized schema", true);
    large.input_schema = Some(serde_json::json!({
        "type": "object",
        "properties": { "payload": { "type": "string", "description": "x".repeat(3_000) } }
    }));
    let descriptors = vec![
        large,
        descriptor("mcp.small", "Small report", "CSV report", true),
    ];
    let ranked = vec![
        RouteScore {
            capability_id: "mcp.large".into(),
            lexical_bp: 10_000,
            type_bp: 0,
            history_bp: None,
            total_bp: 10_000,
            reasons: Vec::new(),
        },
        RouteScore {
            capability_id: "mcp.small".into(),
            lexical_bp: 5_000,
            type_bp: 10_000,
            history_bp: None,
            total_bp: 6_470,
            reasons: Vec::new(),
        },
    ];

    let snapshot = build_ranked_snapshot(&descriptors, &ranked, None, 123, 1_000).unwrap();

    assert_eq!(snapshot.capabilities.len(), 1);
    assert_eq!(snapshot.capabilities[0].capability_id, "mcp.small");
    assert_eq!(snapshot.captured_at_ms, 123);

    let explicit = vec!["mcp.large".to_string()];
    let error =
        build_ranked_snapshot(&descriptors, &ranked, Some(&explicit), 123, 1_000).unwrap_err();
    assert_eq!(error.code, "proofloop_capability_selection_incomplete");
}
