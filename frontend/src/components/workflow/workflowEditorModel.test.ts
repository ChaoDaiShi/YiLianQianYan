import { describe, expect, it } from "vitest";
import type {
  WorkflowGraphDefinition,
  WorkflowNodeDefinition,
} from "../../api/client";
import {
  getInspectorFields,
  getNodePresentation,
  getSaveState,
  layoutWorkflowNodes,
  workflowDefinitionsEqual,
} from "./workflowEditorModel";

function graph(nodes: WorkflowNodeDefinition[], edges: WorkflowGraphDefinition["edges"] = []): WorkflowGraphDefinition {
  return {
    schema_version: 1,
    entry_node_id: nodes[0]?.id ?? "",
    nodes,
    edges,
  };
}

describe("workflow editor model", () => {
  it("presents every real workflow node kind without inventing a type", () => {
    expect(getNodePresentation("agent").label).toBe("Agent");
    expect(getNodePresentation("tool").label).toBe("工具");
    expect(getNodePresentation("subagent").label).toBe("子智能体");
    expect(getNodePresentation("condition").label).toBe("条件");
    expect(getNodePresentation("output").label).toBe("输出");
  });

  it("lays out real nodes and preserves real edge endpoints", () => {
    const definition = graph(
      [
        { id: "start", kind: "agent", config: { type: "agent", prompt: "准备" } },
        { id: "inspect", kind: "tool", config: { type: "tool", tool_name: "process", arguments: {} } },
        { id: "done", kind: "output", config: { type: "output", template: null } },
      ],
      [{ from: "start", to: "inspect" }, { from: "inspect", to: "done" }]
    );

    const laidOut = layoutWorkflowNodes(definition);
    expect(laidOut.nodes.map((node) => node.id)).toEqual(["start", "inspect", "done"]);
    expect(laidOut.nodes[0].x).toBeLessThan(laidOut.nodes[1].x);
    expect(laidOut.nodes[1].x).toBeLessThan(laidOut.nodes[2].x);
    expect(laidOut.edges).toEqual(definition.edges);
  });

  it("exposes only actual config fields in the inspector", () => {
    const fields = getInspectorFields({
      id: "read",
      kind: "tool",
      config: { type: "tool", tool_name: "read_file", arguments: { path: "README.md" } },
    });

    expect(fields.map((field) => field.key)).toEqual(["tool_name", "arguments"]);
    expect(fields.map((field) => field.label)).toEqual(["工具名", "参数"]);
    expect(fields.find((field) => field.key === "tool_name")?.value).toBe("read_file");
  });

  it("derives save state from real local state", () => {
    expect(getSaveState({ dirty: false, saving: false, error: "" }).label).toBe("已保存");
    expect(getSaveState({ dirty: true, saving: false, error: "" }).label).toBe("有未保存修改");
    expect(getSaveState({ dirty: true, saving: true, error: "" }).label).toBe("保存中…");
    expect(getSaveState({ dirty: true, saving: false, error: "保存失败" }).label).toBe("保存失败");
  });

  it("compares definitions without changing the workflow schema", () => {
    const definition = graph([
      { id: "done", kind: "output", config: { type: "output", template: null } },
    ]);
    expect(workflowDefinitionsEqual(definition, structuredClone(definition))).toBe(true);
    expect(
      workflowDefinitionsEqual(definition, {
        ...definition,
        nodes: [{ ...definition.nodes[0], config: { type: "output", template: "完成" } }],
      })
    ).toBe(false);
  });

  it("keeps a freshly loaded graph saved when the backend sorts object keys", () => {
    const editor = graph([{
      id: "read", kind: "tool", config: {
        type: "tool", tool_name: "read_file", arguments: {
          path: "README.md", options: { encoding: "utf-8", include: { hidden: false, pattern: "*.md" } },
          transforms: [{ label: "trim", enabled: true }, { label: "lines", enabled: false }],
        },
      },
    }]);
    const backend: WorkflowGraphDefinition = JSON.parse(`{
      "edges": [], "entry_node_id": "read", "nodes": [{
        "config": {"arguments": {
          "transforms": [{"enabled": true, "label": "trim"}, {"enabled": false, "label": "lines"}],
          "options": {"include": {"pattern": "*.md", "hidden": false}, "encoding": "utf-8"},
          "path": "README.md"
        }, "tool_name": "read_file", "type": "tool"}, "id": "read", "kind": "tool"
      }], "schema_version": 1
    }`);
    expect(JSON.stringify(editor)).not.toBe(JSON.stringify(backend));
    expect(workflowDefinitionsEqual(editor, backend)).toBe(true);
    expect(getSaveState({ dirty: !workflowDefinitionsEqual(editor, backend), saving: false, error: "" }).label).toBe("已保存");

    const changed = structuredClone(backend);
    const config = changed.nodes[0].config;
    const backendConfig = backend.nodes[0].config;
    if (config.type !== "tool" || backendConfig.type !== "tool") throw new Error("Expected tool fixture");
    config.arguments.options = { include: { hidden: false, pattern: "*.txt" }, encoding: "utf-8" };
    expect(workflowDefinitionsEqual(editor, changed)).toBe(false);
    config.arguments = { ...backendConfig.arguments, transforms: [{ enabled: false, label: "lines" }, { enabled: true, label: "trim" }] };
    expect(workflowDefinitionsEqual(editor, changed)).toBe(false);
  });

  it("preserves the order of node and edge arrays when comparing definitions", () => {
    const definition = graph([
      { id: "start", kind: "agent", config: { type: "agent", prompt: "prepare" } },
      { id: "read", kind: "tool", config: { type: "tool", tool_name: "read_file", arguments: { path: "README.md" } } },
      { id: "done", kind: "output", config: { type: "output", template: null } },
    ], [{ from: "start", to: "read" }, { from: "read", to: "done" }]);
    expect(workflowDefinitionsEqual(definition, { ...definition, nodes: [...definition.nodes].reverse() })).toBe(false);
    expect(workflowDefinitionsEqual(definition, { ...definition, edges: [...definition.edges].reverse() })).toBe(false);
  });
});
