//! Deterministic capability ranking and ordered snapshot construction.

use std::collections::{BTreeSet, HashMap, HashSet};

use super::contracts::{CapabilitySnapshot, ProofError};
use super::enhancement_contracts::RouteScore;
use crate::modules::capability::{CapabilityDescriptor, CapabilityKind, CapabilityRuntimeStatus};

const FORMAT_TERMS: &[&str] = &[
    "csv", "doc", "docx", "html", "jpeg", "jpg", "json", "markdown", "md", "mp3", "mp4", "pdf",
    "png", "sql", "svg", "text", "txt", "wav", "xls", "xlsx", "xml", "zip",
];

pub fn route_total(lexical_bp: u16, type_bp: u16, history_bp: Option<u16>) -> u16 {
    let lexical = u32::from(lexical_bp);
    let type_match = u32::from(type_bp);
    match history_bp {
        Some(history) => ((60 * lexical + 25 * type_match + 15 * u32::from(history)) / 100) as u16,
        None => ((60 * lexical + 25 * type_match) / 85) as u16,
    }
}

pub fn rank_capabilities(
    goal: &str,
    descriptors: &[CapabilityDescriptor],
    allowed_ids: Option<&[String]>,
    history: &HashMap<String, u16>,
) -> Vec<RouteScore> {
    let goal_terms = tokenize(goal);
    let field_universe: BTreeSet<String> = descriptors
        .iter()
        .flat_map(descriptor_field_terms)
        .collect();
    let format_universe: BTreeSet<String> = descriptors
        .iter()
        .flat_map(descriptor_format_terms)
        .collect();
    let requested_fields: BTreeSet<String> =
        goal_terms.intersection(&field_universe).cloned().collect();
    let requested_formats: BTreeSet<String> = goal_terms
        .iter()
        .filter(|term| FORMAT_TERMS.contains(&term.as_str()) || format_universe.contains(*term))
        .cloned()
        .collect();

    let mut scores = Vec::with_capacity(descriptors.len());
    for descriptor in descriptors {
        let capability_id = descriptor.id.as_str().to_string();
        if let Some(reason) = exclusion_reason(descriptor, allowed_ids) {
            scores.push(RouteScore {
                capability_id,
                lexical_bp: 0,
                type_bp: 0,
                history_bp: None,
                total_bp: 0,
                reasons: vec![format!("excluded:{reason}")],
            });
            continue;
        }

        let capability_terms = descriptor_terms(descriptor);
        let lexical_bp = ratio_bp(
            goal_terms.intersection(&capability_terms).count(),
            goal_terms.len(),
        );
        let type_terms = descriptor_field_terms(descriptor);
        let mut format_terms = descriptor_format_terms(descriptor);
        format_terms.extend(
            descriptor_terms(descriptor)
                .into_iter()
                .filter(|term| FORMAT_TERMS.contains(&term.as_str())),
        );
        let typed_matches = requested_fields.intersection(&type_terms).count()
            + requested_formats.intersection(&format_terms).count();
        let typed_count = requested_fields.len() + requested_formats.len();
        let type_bp = ratio_bp(typed_matches, typed_count);
        let history_bp = history.get(&capability_id).copied();
        let mut reasons = Vec::new();
        let matched_terms: Vec<_> = goal_terms
            .intersection(&capability_terms)
            .cloned()
            .collect();
        if !matched_terms.is_empty() {
            reasons.push(format!("词项命中:{}", matched_terms.join(",")));
        }
        let matched_fields: Vec<_> = requested_fields
            .intersection(&type_terms)
            .cloned()
            .chain(requested_formats.intersection(&format_terms).cloned())
            .collect();
        if !matched_fields.is_empty() {
            reasons.push(format!("字段或格式命中:{}", matched_fields.join(",")));
        }
        if history_bp.is_none() {
            reasons.push("无足量同版本已验收历史".into());
        }
        scores.push(RouteScore {
            capability_id,
            lexical_bp,
            type_bp,
            history_bp,
            total_bp: route_total(lexical_bp, type_bp, history_bp),
            reasons,
        });
    }

    scores.sort_by(|left, right| {
        let left_excluded = left
            .reasons
            .iter()
            .any(|reason| reason.starts_with("excluded:"));
        let right_excluded = right
            .reasons
            .iter()
            .any(|reason| reason.starts_with("excluded:"));
        left_excluded
            .cmp(&right_excluded)
            .then_with(|| right.total_bp.cmp(&left.total_bp))
            .then_with(|| left.capability_id.cmp(&right.capability_id))
    });
    scores
}

pub fn build_ranked_snapshot(
    descriptors: &[CapabilityDescriptor],
    ranked: &[RouteScore],
    explicit_ids: Option<&[String]>,
    now_ms: i64,
    max_chars: usize,
) -> Result<CapabilitySnapshot, ProofError> {
    let descriptors_by_id: HashMap<_, _> = descriptors
        .iter()
        .map(|descriptor| (descriptor.id.as_str(), descriptor))
        .collect();
    let explicit: HashSet<&str> = explicit_ids
        .into_iter()
        .flatten()
        .map(String::as_str)
        .collect();
    let mut capabilities = Vec::new();

    for route in ranked {
        if route
            .reasons
            .iter()
            .any(|reason| reason.starts_with("excluded:"))
        {
            continue;
        }
        if explicit_ids.is_some() && !explicit.contains(route.capability_id.as_str()) {
            continue;
        }
        if capabilities.len() >= super::capabilities::MAX_SNAPSHOT_CAPABILITIES {
            break;
        }
        let Some(descriptor) = descriptors_by_id.get(route.capability_id.as_str()) else {
            continue;
        };
        let Some(spec) = super::capabilities::project_capability_descriptor(descriptor) else {
            continue;
        };
        if spec.readiness != "ready" {
            continue;
        }

        let mut trial = capabilities.clone();
        trial.push(spec.clone());
        let serialized_chars = serde_json::to_string(&trial)
            .expect("capability snapshot serialization is infallible")
            .chars()
            .count();
        if serialized_chars > max_chars {
            continue;
        }
        capabilities.push(spec);
    }

    if let Some(requested) = explicit_ids {
        let included: HashSet<&str> = capabilities
            .iter()
            .map(|capability| capability.capability_id.as_str())
            .collect();
        if requested.iter().any(|id| !included.contains(id.as_str())) {
            return Err(ProofError::new(
                "proofloop_capability_selection_incomplete",
                "显式选择的能力不能完整装入当前规划上下文",
            ));
        }
    }

    Ok(CapabilitySnapshot {
        snapshot_id: uuid::Uuid::new_v4().to_string(),
        captured_at_ms: now_ms,
        capabilities,
    })
}

fn exclusion_reason(
    descriptor: &CapabilityDescriptor,
    allowed_ids: Option<&[String]>,
) -> Option<&'static str> {
    if allowed_ids.is_some_and(|allowed| {
        !allowed
            .iter()
            .any(|id| id.as_str() == descriptor.id.as_str())
    }) {
        return Some("not_selected");
    }
    if !matches!(
        descriptor.kind,
        CapabilityKind::McpTool | CapabilityKind::Workflow
    ) {
        return Some("unsupported_kind");
    }
    if !descriptor.enabled
        || descriptor.status != CapabilityRuntimeStatus::Ready
        || !descriptor.metadata.runtime_ready
    {
        return Some("not_ready");
    }
    None
}

fn descriptor_terms(descriptor: &CapabilityDescriptor) -> BTreeSet<String> {
    let mut terms = tokenize(&descriptor.name);
    terms.extend(tokenize(&descriptor.description));
    terms.extend(descriptor_field_terms(descriptor));
    terms
}

fn descriptor_field_terms(descriptor: &CapabilityDescriptor) -> BTreeSet<String> {
    let mut terms = schema_field_terms(descriptor.input_schema.as_ref());
    terms.extend(schema_field_terms(descriptor.output_schema.as_ref()));
    terms
}

fn descriptor_format_terms(descriptor: &CapabilityDescriptor) -> BTreeSet<String> {
    let mut terms = schema_format_terms(descriptor.input_schema.as_ref());
    terms.extend(schema_format_terms(descriptor.output_schema.as_ref()));
    terms
}

fn tokenize(text: &str) -> BTreeSet<String> {
    let mut terms = BTreeSet::new();
    let mut latin = String::new();
    let mut han = Vec::new();
    let flush_latin = |latin: &mut String, terms: &mut BTreeSet<String>| {
        if !latin.is_empty() {
            terms.insert(latin.to_lowercase());
            latin.clear();
        }
    };
    let flush_han = |han: &mut Vec<char>, terms: &mut BTreeSet<String>| {
        for index in 0..han.len() {
            terms.insert(han[index].to_string());
            if index + 1 < han.len() {
                terms.insert(format!("{}{}", han[index], han[index + 1]));
            }
        }
        han.clear();
    };

    for character in text.chars() {
        if is_han(character) {
            flush_latin(&mut latin, &mut terms);
            han.push(character);
        } else if character.is_alphanumeric() {
            flush_han(&mut han, &mut terms);
            latin.extend(character.to_lowercase());
        } else {
            flush_latin(&mut latin, &mut terms);
            flush_han(&mut han, &mut terms);
        }
    }
    flush_latin(&mut latin, &mut terms);
    flush_han(&mut han, &mut terms);
    terms
}

fn is_han(character: char) -> bool {
    matches!(
        character as u32,
        0x3400..=0x4dbf | 0x4e00..=0x9fff | 0xf900..=0xfaff | 0x20000..=0x2fa1f
    )
}

fn schema_field_terms(schema: Option<&serde_json::Value>) -> BTreeSet<String> {
    let mut fields = BTreeSet::new();
    if let Some(schema) = schema {
        collect_schema_fields(schema, &mut fields);
    }
    fields
}

fn collect_schema_fields(value: &serde_json::Value, fields: &mut BTreeSet<String>) {
    let Some(object) = value.as_object() else {
        return;
    };
    if let Some(properties) = object
        .get("properties")
        .and_then(serde_json::Value::as_object)
    {
        for (name, schema) in properties {
            fields.extend(tokenize(name));
            collect_schema_fields(schema, fields);
        }
    }
    if let Some(items) = object.get("items") {
        collect_schema_fields(items, fields);
    }
    for key in ["allOf", "anyOf", "oneOf"] {
        if let Some(children) = object.get(key).and_then(serde_json::Value::as_array) {
            for child in children {
                collect_schema_fields(child, fields);
            }
        }
    }
}

fn schema_format_terms(schema: Option<&serde_json::Value>) -> BTreeSet<String> {
    let mut formats = BTreeSet::new();
    if let Some(schema) = schema {
        collect_schema_formats(schema, &mut formats);
    }
    formats
}

fn collect_schema_formats(value: &serde_json::Value, formats: &mut BTreeSet<String>) {
    match value {
        serde_json::Value::Object(object) => {
            if let Some(format) = object.get("format").and_then(serde_json::Value::as_str) {
                formats.extend(tokenize(format));
            }
            if let Some(values) = object.get("enum").and_then(serde_json::Value::as_array) {
                for item in values.iter().filter_map(serde_json::Value::as_str) {
                    let normalized = tokenize(item);
                    formats.extend(
                        normalized
                            .into_iter()
                            .filter(|term| FORMAT_TERMS.contains(&term.as_str())),
                    );
                }
            }
            for child in object.values() {
                collect_schema_formats(child, formats);
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                collect_schema_formats(item, formats);
            }
        }
        _ => {}
    }
}

fn ratio_bp(numerator: usize, denominator: usize) -> u16 {
    if denominator == 0 {
        0
    } else {
        ((numerator as u64 * 10_000) / denominator as u64) as u16
    }
}
