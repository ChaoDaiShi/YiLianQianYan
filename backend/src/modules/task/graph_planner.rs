//! Natural-language TaskGraph proposals. Planning has no execution authority.
use super::{TaskGraph, TaskGraphId};

pub fn parse_graph_proposal(
    content: &str,
    graph_id: &TaskGraphId,
    available_refs: &[String],
) -> Result<TaskGraph, String> {
    if content.len() > 128_000 {
        return Err("plan exceeds size limit".into());
    }
    let graph: TaskGraph =
        serde_json::from_str(content).map_err(|_| "planner returned malformed TaskGraph JSON")?;
    graph.validate().map_err(|error| error.to_string())?;
    if &graph.id != graph_id || graph.revision.value() != 1 {
        return Err("planner changed requested graph identity or initial revision".into());
    }
    if graph.nodes.is_empty() || graph.nodes.len() > super::model::MAX_TASK_PLAN_STEPS {
        return Err("plan must contain 1-20 nodes".into());
    }
    for node in &graph.nodes {
        let input = node
            .input
            .as_object()
            .ok_or("node input must be an object")?;
        if input.keys().any(|key| {
            !matches!(
                key.as_str(),
                "instruction" | "acceptance_criteria" | "executor_ref"
            )
        }) {
            return Err("planner returned unsupported input fields".into());
        }
        let instruction = input
            .get("instruction")
            .and_then(serde_json::Value::as_str)
            .ok_or("instruction must be a string")?;
        if instruction.trim().is_empty() || instruction.chars().count() > 4000 {
            return Err("instruction must contain 1-4000 characters".into());
        }
        let criteria = input
            .get("acceptance_criteria")
            .and_then(serde_json::Value::as_array)
            .ok_or("acceptance_criteria must be an array")?;
        if criteria.len() > 20
            || criteria
                .iter()
                .any(|value| value.as_str().is_none_or(|text| text.chars().count() > 500))
        {
            return Err("invalid acceptance criteria".into());
        }
        if let Some(reference) = input.get("executor_ref") {
            let reference = reference
                .as_str()
                .ok_or("executor_ref must be a string or omitted")?;
            if !available_refs
                .iter()
                .any(|available| available == reference)
            {
                return Err("planner selected an unavailable executor; configure a workflow or leave the node editable".into());
            }
        }
    }
    Ok(graph)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    fn proposal() -> Value {
        json!({"schema_version":1,"id":"planned","revision":1,"nodes":[
            {"id":"one","kind":"work","title":"Read","input":{
                "instruction":"Read the configured workflow output", "acceptance_criteria":[]
            },"retry_policy":{"max_attempts":1}}
        ],"edges":[]})
    }

    #[test]
    fn review_planner_retry_policy_matches_execution_bounds() {
        let id = TaskGraphId::new("planned").unwrap();
        for (attempts, accepted) in [(0, false), (8, true), (9, false)] {
            let mut value = proposal();
            value["nodes"][0]["retry_policy"]["max_attempts"] = json!(attempts);
            assert_eq!(
                parse_graph_proposal(&value.to_string(), &id, &[]).is_ok(),
                accepted,
                "attempts={attempts}"
            );
        }
    }

    #[test]
    fn accepts_editable_plan_without_inventing_an_executor() {
        let graph = parse_graph_proposal(
            &proposal().to_string(),
            &TaskGraphId::new("planned").unwrap(),
            &[],
        )
        .unwrap();
        assert_eq!(graph.nodes.len(), 1);
        assert!(graph.nodes[0].input.get("executor_ref").is_none());
    }

    #[test]
    fn accepts_only_available_workflow_references() {
        let mut value = proposal();
        value["nodes"][0]["input"]["executor_ref"] = json!("workflow://saved");
        let id = TaskGraphId::new("planned").unwrap();
        assert!(parse_graph_proposal(&value.to_string(), &id, &[]).is_err());
        assert!(
            parse_graph_proposal(&value.to_string(), &id, &["workflow://saved".into()]).is_ok()
        );
    }

    #[test]
    fn rejects_malformed_unknown_fields_cycles_and_identity_changes() {
        let id = TaskGraphId::new("planned").unwrap();
        assert!(parse_graph_proposal("not JSON", &id, &[]).is_err());
        for (field, bad) in [
            ("revision", json!(2)),
            ("id", json!("invented")),
            ("schema_version", json!(99)),
            ("extra", json!(true)),
        ] {
            let mut value = proposal();
            value[field] = bad;
            assert!(parse_graph_proposal(&value.to_string(), &id, &[]).is_err());
        }
        let mut value = proposal();
        value["edges"] = json!([{"from":"one","to":"one"}]);
        assert!(parse_graph_proposal(&value.to_string(), &id, &[]).is_err());
        let mut value = proposal();
        value["nodes"][0]["input"]["shell"] = json!("unexpected");
        assert!(parse_graph_proposal(&value.to_string(), &id, &[]).is_err());
    }
}
