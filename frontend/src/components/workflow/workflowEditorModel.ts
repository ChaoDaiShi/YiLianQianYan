import type {
  WorkflowEdgeDefinition,
  WorkflowGraphDefinition,
  WorkflowNodeDefinition,
  WorkflowNodeKind,
} from "../../api/client";

export type WorkflowNodePresentation = {
  label: string;
  description: string;
  tone: "purple" | "blue" | "gold" | "pink" | "green";
};

const NODE_PRESENTATIONS: Record<WorkflowNodeKind, WorkflowNodePresentation> = {
  agent: { label: "Agent", description: "生成文字结果", tone: "purple" },
  tool: { label: "工具", description: "通过安全网关执行工具", tone: "blue" },
  subagent: { label: "子智能体", description: "委派给已发现的子智能体", tone: "pink" },
  condition: { label: "条件", description: "按已有条件决定是否继续", tone: "gold" },
  output: { label: "输出", description: "输出模板或结束标记", tone: "green" },
};

export function getNodePresentation(kind: WorkflowNodeKind): WorkflowNodePresentation {
  return NODE_PRESENTATIONS[kind];
}

export type WorkflowInspectorField = {
  key: string;
  label: string;
  value: unknown;
  kind: "text" | "textarea" | "json" | "select";
};

export function getInspectorFields(node: WorkflowNodeDefinition): WorkflowInspectorField[] {
  switch (node.kind) {
    case "agent": {
      const config = node.config as Extract<typeof node.config, { type: "agent" }>;
      return [{ key: "prompt", label: "Agent 指令", value: config.prompt, kind: "textarea" }];
    }
    case "tool": {
      const config = node.config as Extract<typeof node.config, { type: "tool" }>;
      return [
        { key: "tool_name", label: "工具名", value: config.tool_name, kind: "text" },
        { key: "arguments", label: "参数", value: config.arguments, kind: "json" },
      ];
    }
    case "subagent": {
      const config = node.config as Extract<typeof node.config, { type: "subagent" }>;
      return [
        { key: "subagent_name", label: "子智能体名", value: config.subagent_name, kind: "text" },
        { key: "task", label: "任务", value: config.task, kind: "textarea" },
      ];
    }
    case "condition": {
      const config = node.config as Extract<typeof node.config, { type: "condition" }>;
      return [{ key: "when", label: "条件", value: config.when, kind: "select" }];
    }
    case "output": {
      const config = node.config as Extract<typeof node.config, { type: "output" }>;
      return [{ key: "template", label: "输出模板", value: config.template ?? "", kind: "textarea" }];
    }
  }
}

export type WorkflowLayoutNode = {
  id: string;
  node: WorkflowNodeDefinition;
  x: number;
  y: number;
};

export type WorkflowLayout = {
  nodes: WorkflowLayoutNode[];
  edges: WorkflowEdgeDefinition[];
  width: number;
  height: number;
};

/**
 * Positions are presentation-only. The persisted workflow schema remains the
 * backend's nodes/edges/entry_node_id model and never receives coordinates.
 */
export function layoutWorkflowNodes(definition: WorkflowGraphDefinition): WorkflowLayout {
  const nodeWidth = 190;
  const columnGap = 48;
  const rowGap = 24;
  const nodeHeight = 112;
  const incoming = new Map<string, number>();
  const level = new Map<string, number>();

  definition.nodes.forEach((node) => {
    incoming.set(node.id, 0);
    level.set(node.id, 0);
  });
  definition.edges.forEach((edge) => incoming.set(edge.to, (incoming.get(edge.to) ?? 0) + 1));

  const queue = definition.nodes.filter((node) => (incoming.get(node.id) ?? 0) === 0).map((node) => node.id);
  const remainingIncoming = new Map(incoming);
  while (queue.length > 0) {
    const current = queue.shift()!;
    const currentLevel = level.get(current) ?? 0;
    definition.edges.filter((edge) => edge.from === current).forEach((edge) => {
      level.set(edge.to, Math.max(level.get(edge.to) ?? 0, currentLevel + 1));
      const remaining = (remainingIncoming.get(edge.to) ?? 1) - 1;
      remainingIncoming.set(edge.to, remaining);
      if (remaining === 0) queue.push(edge.to);
    });
  }

  const columns = new Map<number, WorkflowNodeDefinition[]>();
  definition.nodes.forEach((node, index) => {
    const nodeLevel = level.has(node.id) ? level.get(node.id)! : index;
    const column = columns.get(nodeLevel) ?? [];
    column.push(node);
    columns.set(nodeLevel, column);
  });

  const laidOut = definition.nodes.map((node) => {
    const nodeLevel = level.has(node.id) ? level.get(node.id)! : definition.nodes.indexOf(node);
    const row = columns.get(nodeLevel)?.indexOf(node) ?? 0;
    return {
      id: node.id,
      node,
      x: 24 + nodeLevel * (nodeWidth + columnGap),
      y: 24 + row * (nodeHeight + rowGap),
    };
  });

  const maxLevel = Math.max(0, ...laidOut.map((node) => node.x));
  const maxY = Math.max(0, ...laidOut.map((node) => node.y));
  return {
    nodes: laidOut,
    edges: definition.edges,
    width: maxLevel + nodeWidth + 24,
    height: maxY + nodeHeight + 24,
  };
}

export function workflowDefinitionsEqual(
  left: WorkflowGraphDefinition,
  right: WorkflowGraphDefinition
): boolean {
  return JSON.stringify(left) === JSON.stringify(right);
}

export type WorkflowSaveStateInput = {
  dirty: boolean;
  saving: boolean;
  error: string;
};

export function getSaveState(input: WorkflowSaveStateInput): {
  label: string;
  tone: "muted" | "accent" | "danger";
} {
  if (input.saving) return { label: "保存中…", tone: "accent" };
  if (input.error) return { label: "保存失败", tone: "danger" };
  if (input.dirty) return { label: "有未保存修改", tone: "accent" };
  return { label: "已保存", tone: "muted" };
}
