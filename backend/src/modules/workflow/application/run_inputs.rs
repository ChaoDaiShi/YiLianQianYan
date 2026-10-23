//! Per-run values for existing nodes. These never change the stored graph,
//! selected tool, execution identity, or security policy.
use std::collections::BTreeMap;

use serde::Deserialize;
use serde_json::{Map, Value};

use super::super::definition::{WorkflowGraphDefinition, WorkflowNodeConfig};

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum WorkflowNodeInput {
    Agent { prompt: String },
    Tool { arguments: Map<String, Value> },
    Subagent { task: String },
    Output { template: Option<String> },
}

pub type WorkflowRunInputs = BTreeMap<String, WorkflowNodeInput>;

pub fn apply_run_inputs(
    definition: &mut WorkflowGraphDefinition,
    inputs: WorkflowRunInputs,
) -> Result<(), String> {
    for (id, input) in inputs {
        let node = definition
            .nodes
            .iter_mut()
            .find(|node| node.id.as_str() == id)
            .ok_or_else(|| format!("运行参数引用了不存在的节点：{id}"))?;
        match (&mut node.config, input) {
            (WorkflowNodeConfig::Agent { prompt }, WorkflowNodeInput::Agent { prompt: value }) => {
                if value.trim().is_empty() {
                    return Err(format!("节点 {id} 的提示词不能为空"));
                }
                *prompt = value;
            }
            (
                WorkflowNodeConfig::Tool { arguments, .. },
                WorkflowNodeInput::Tool { arguments: value },
            ) => {
                *arguments = Value::Object(value);
            }
            (
                WorkflowNodeConfig::Subagent { task, .. },
                WorkflowNodeInput::Subagent { task: value },
            ) => {
                if value.trim().is_empty() {
                    return Err(format!("节点 {id} 的任务不能为空"));
                }
                *task = value;
            }
            (
                WorkflowNodeConfig::Output { template },
                WorkflowNodeInput::Output { template: value },
            ) => {
                *template = value;
            }
            _ => return Err(format!("节点 {id} 的运行参数类型与节点配置不匹配")),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn graph() -> WorkflowGraphDefinition {
        serde_json::from_value(serde_json::json!({"schema_version":1,"entry_node_id":"read","nodes":[{"id":"read","kind":"tool","config":{"type":"tool","tool_name":"read_file","arguments":{"path":"old.txt"}}}],"edges":[]})).unwrap()
    }
    #[test]
    fn inputs_change_snapshot_only_and_preserve_tool_identity() {
        let original = graph();
        let mut snapshot = original.clone();
        let inputs = serde_json::from_value(
            serde_json::json!({"read":{"type":"tool","arguments":{"path":"new.txt"}}}),
        )
        .unwrap();
        apply_run_inputs(&mut snapshot, inputs).unwrap();
        assert_ne!(snapshot, original);
        match &snapshot.nodes[0].config {
            WorkflowNodeConfig::Tool {
                tool_name,
                arguments,
            } => {
                assert_eq!(tool_name, "read_file");
                assert_eq!(arguments["path"], "new.txt");
            }
            _ => panic!("tool type changed"),
        }
    }
    #[test]
    fn rejects_mismatched_unknown_and_privileged_inputs() {
        for value in [
            serde_json::json!({"read":{"type":"agent","prompt":"hello"}}),
            serde_json::json!({"missing":{"type":"tool","arguments":{}}}),
        ] {
            assert!(
                apply_run_inputs(&mut graph(), serde_json::from_value(value).unwrap()).is_err()
            );
        }
        assert!(serde_json::from_value::<WorkflowRunInputs>(
            serde_json::json!({"read":{"type":"tool","tool_name":"bash","arguments":{}}})
        )
        .is_err());
    }
}
