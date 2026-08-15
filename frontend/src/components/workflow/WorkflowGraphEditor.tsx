import { useState } from "react";
import { Plus, Trash2, X } from "lucide-react";
import { Button, Input, Modal, Badge } from "../ui";
import {
  createWorkflowGraph,
  updateWorkflowGraph,
  type WorkflowGraphDefinition,
  type WorkflowGraphRecord,
  type WorkflowNodeConfig,
  type WorkflowNodeDefinition,
  type WorkflowNodeKind,
  type WorkflowCondition,
} from "../../api/client";

const KINDS: WorkflowNodeKind[] = ["agent", "tool", "subagent", "condition", "output"];

const KIND_HINTS: Record<WorkflowNodeKind, string> = {
  agent: "Agent（LLM-only）：仅生成文字，不调用工具",
  tool: "Tool：经安全网关执行内置/ MCP 工具",
  subagent: "Subagent：委派给已发现子智能体",
  condition: "Condition：always / previous_succeeded",
  output: "Output：输出固定模板或结束标记",
};

function emptyNode(kind: WorkflowNodeKind): WorkflowNodeDefinition {
  const config = ((): WorkflowNodeConfig => {
    switch (kind) {
      case "agent":
        return { type: "agent", prompt: "" };
      case "tool":
        return { type: "tool", tool_name: "", arguments: {} };
      case "subagent":
        return { type: "subagent", subagent_name: "", task: "" };
      case "condition":
        return { type: "condition", when: "always" };
      case "output":
        return { type: "output", template: null };
    }
  })();
  return { id: "", kind, config };
}

/** Client-side experience validation. The authoritative check is the backend. */
export function validateGraphClientSide(
  definition: WorkflowGraphDefinition
): string | null {
  if (definition.nodes.length === 0) return "至少需要一个节点";
  const ids = new Set<string>();
  for (const node of definition.nodes) {
    if (!node.id.trim()) return "节点 ID 不能为空";
    if (ids.has(node.id)) return `重复的节点 ID：${node.id}`;
    ids.add(node.id);
  }
  if (!definition.entry_node_id) return "请选择入口节点";
  if (!ids.has(definition.entry_node_id)) return "入口节点不存在";
  for (const edge of definition.edges) {
    if (!edge.from || !edge.to) return "连接必须指定 from 和 to";
    if (!ids.has(edge.from) || !ids.has(edge.to)) return "连接引用了不存在的节点";
  }
  return null;
}

export default function WorkflowGraphEditor({
  graph,
  onSaved,
  onClose,
}: {
  graph: WorkflowGraphRecord | null;
  onSaved: (graph: WorkflowGraphRecord) => void;
  onClose: () => void;
}) {
  const [name, setName] = useState(graph?.name ?? "");
  const [description, setDescription] = useState(graph?.description ?? "");
  const [nodes, setNodes] = useState<WorkflowNodeDefinition[]>(
    graph?.definition.nodes ?? [emptyNode("output")]
  );
  const [edges, setEdges] = useState(graph?.definition.edges ?? []);
  const [entryNodeId, setEntryNodeId] = useState(
    graph?.definition.entry_node_id ?? ""
  );
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState("");

  const updateNode = (index: number, patch: Partial<WorkflowNodeDefinition>) => {
    setNodes((prev) => prev.map((n, i) => (i === index ? { ...n, ...patch } : n)));
  };

  const updateNodeConfig = (index: number, config: WorkflowNodeConfig) => {
    setNodes((prev) => prev.map((n, i) => (i === index ? { ...n, config } : n)));
  };

  const addNode = () => {
    setNodes((prev) => [...prev, emptyNode("output")]);
  };

  const removeNode = (index: number) => {
    const removed = nodes[index];
    setNodes((prev) => prev.filter((_, i) => i !== index));
    setEdges((prev) =>
      prev.filter((e) => e.from !== removed.id && e.to !== removed.id)
    );
    if (entryNodeId === removed.id) setEntryNodeId("");
  };

  const addEdge = () => {
    if (nodes.length >= 2) {
      setEdges((prev) => [...prev, { from: nodes[0].id, to: nodes[1].id }]);
    }
  };

  const removeEdge = (index: number) => {
    setEdges((prev) => prev.filter((_, i) => i !== index));
  };

  const handleSave = async () => {
    const definition: WorkflowGraphDefinition = {
      schema_version: 1,
      entry_node_id: entryNodeId,
      nodes,
      edges,
    };
    const clientError = validateGraphClientSide(definition);
    if (clientError) {
      setError(clientError);
      return;
    }
    setSaving(true);
    setError("");
    const payload = { name, description, definition };
    const res = graph
      ? await updateWorkflowGraph(graph.id, payload)
      : await createWorkflowGraph(payload);
    setSaving(false);
    if (res.ok) {
      onSaved(res.data);
    } else {
      setError(res.error);
    }
  };

  const nodeOptions = nodes.map((n) => n.id).filter(Boolean);

  return (
    <Modal
      open
      onClose={onClose}
      title={graph ? "编辑运行工作流" : "新建运行工作流"}
      className="max-w-3xl"
      footer={
        <>
          <Button variant="secondary" onClick={onClose}>
            取消
          </Button>
          <Button onClick={handleSave} disabled={saving}>
            {saving ? "保存中…" : "保存"}
          </Button>
        </>
      }
    >
      <div className="space-y-4">
        {error && (
          <div className="rounded-lg border border-[var(--danger)]/30 bg-[var(--danger)]/10 px-3 py-2 text-sm text-[var(--danger)]">
            {error}
          </div>
        )}
        <Input label="名称" value={name} onChange={(e) => setName(e.target.value)} />
        <div>
          <label className="block text-sm font-medium mb-1">描述</label>
          <textarea
            value={description}
            onChange={(e) => setDescription(e.target.value)}
            rows={2}
            placeholder="描述此工作流的用途..."
            className="w-full rounded-lg border border-[var(--border)] bg-[var(--input-bg)] px-3 py-2 text-sm text-[var(--text)] placeholder:text-[var(--text-faint)] focus:outline-none focus:ring-2 focus:ring-[var(--accent)]/40 resize-none"
          />
        </div>

        {/* Nodes */}
        <div>
          <div className="flex items-center justify-between mb-2">
            <h4 className="text-sm font-semibold">节点</h4>
            <Button size="sm" variant="secondary" onClick={addNode}>
              <Plus className="w-3.5 h-3.5" />
              添加节点
            </Button>
          </div>
          <div className="space-y-3">
            {nodes.map((node, index) => (
              <div
                key={index}
                className="rounded-lg border border-[var(--border)] p-3 space-y-2"
              >
                <div className="flex items-center gap-2">
                  <Input
                    label="ID"
                    value={node.id}
                    onChange={(e) => updateNode(index, { id: e.target.value })}
                    placeholder="node_id"
                  />
                  <div className="flex-1">
                    <label className="block text-sm font-medium mb-1">类型</label>
                    <select
                      value={node.kind}
                      onChange={(e) => {
                        const kind = e.target.value as WorkflowNodeKind;
                        updateNode(index, { kind, config: emptyNode(kind).config });
                      }}
                      className="w-full rounded-lg border border-[var(--border)] bg-[var(--input-bg)] px-3 py-2 text-sm"
                    >
                      {KINDS.map((k) => (
                        <option key={k} value={k}>
                          {k}
                        </option>
                      ))}
                    </select>
                  </div>
                  <button
                    onClick={() => removeNode(index)}
                    className="self-end p-1.5 rounded hover:bg-[var(--danger)]/20 text-[var(--text-muted)] hover:text-[var(--danger)]"
                    aria-label="删除节点"
                  >
                    <Trash2 className="w-4 h-4" />
                  </button>
                </div>
                <p className="text-xs text-[var(--text-faint)]">{KIND_HINTS[node.kind]}</p>
                <NodeConfigFields
                  node={node}
                  onChange={(config) => updateNodeConfig(index, config)}
                />
                <div className="flex items-center gap-2">
                  <Badge tone="default">入口</Badge>
                  <input
                    type="radio"
                    checked={entryNodeId === node.id}
                    onChange={() => setEntryNodeId(node.id)}
                  />
                </div>
              </div>
            ))}
          </div>
        </div>

        {/* Edges */}
        <div>
          <div className="flex items-center justify-between mb-2">
            <h4 className="text-sm font-semibold">连接</h4>
            <Button size="sm" variant="secondary" onClick={addEdge}>
              <Plus className="w-3.5 h-3.5" />
              添加连接
            </Button>
          </div>
          <div className="space-y-2">
            {edges.map((edge, index) => (
              <div key={index} className="flex items-center gap-2">
                <select
                  value={edge.from}
                  onChange={(e) =>
                    setEdges((prev) =>
                      prev.map((ed, i) => (i === index ? { ...ed, from: e.target.value } : ed))
                    )
                  }
                  className="flex-1 rounded-lg border border-[var(--border)] bg-[var(--input-bg)] px-3 py-2 text-sm"
                >
                  <option value="">from…</option>
                  {nodeOptions.map((id) => (
                    <option key={id} value={id}>
                      {id}
                    </option>
                  ))}
                </select>
                <span className="text-[var(--text-faint)]">→</span>
                <select
                  value={edge.to}
                  onChange={(e) =>
                    setEdges((prev) =>
                      prev.map((ed, i) => (i === index ? { ...ed, to: e.target.value } : ed))
                    )
                  }
                  className="flex-1 rounded-lg border border-[var(--border)] bg-[var(--input-bg)] px-3 py-2 text-sm"
                >
                  <option value="">to…</option>
                  {nodeOptions.map((id) => (
                    <option key={id} value={id}>
                      {id}
                    </option>
                  ))}
                </select>
                <button
                  onClick={() => removeEdge(index)}
                  className="p-1.5 rounded hover:bg-[var(--danger)]/20 text-[var(--text-muted)] hover:text-[var(--danger)]"
                  aria-label="删除连接"
                >
                  <X className="w-4 h-4" />
                </button>
              </div>
            ))}
            {edges.length === 0 && (
              <p className="text-xs text-[var(--text-faint)]">暂无连接</p>
            )}
          </div>
        </div>
      </div>
    </Modal>
  );
}

function NodeConfigFields({
  node,
  onChange,
}: {
  node: WorkflowNodeDefinition;
  onChange: (config: WorkflowNodeConfig) => void;
}) {
  const config = node.config;
  switch (node.kind) {
    case "agent":
      return (
        <textarea
          value={(config as { type: "agent"; prompt: string }).prompt}
          onChange={(e) => onChange({ type: "agent", prompt: e.target.value })}
          rows={2}
          placeholder="Agent 指令（LLM-only，不调用工具）"
          className="w-full rounded-lg border border-[var(--border)] bg-[var(--input-bg)] px-3 py-2 text-sm"
        />
      );
    case "tool": {
      const cfg = config as { type: "tool"; tool_name: string; arguments: Record<string, unknown> };
      return (
        <div className="space-y-2">
          <Input
            label="工具名"
            value={cfg.tool_name}
            onChange={(e) => onChange({ type: "tool", tool_name: e.target.value, arguments: cfg.arguments })}
            placeholder="read_file / bash / mcp_xxx"
          />
          <div>
            <label className="block text-sm font-medium mb-1">参数 (JSON)</label>
            <textarea
              value={JSON.stringify(cfg.arguments ?? {}, null, 2)}
              onChange={(e) => {
                try {
                  const parsed = JSON.parse(e.target.value);
                  if (parsed && typeof parsed === "object" && !Array.isArray(parsed)) {
                    onChange({ type: "tool", tool_name: cfg.tool_name, arguments: parsed });
                  }
                } catch {
                  /* keep last valid arguments */
                }
              }}
              rows={3}
              placeholder='{"path": "README.md"}'
              className="w-full rounded-lg border border-[var(--border)] bg-[var(--input-bg)] px-3 py-2 text-sm font-mono"
            />
          </div>
        </div>
      );
    }
    case "subagent": {
      const cfg = config as { type: "subagent"; subagent_name: string; task: string };
      return (
        <div className="space-y-2">
          <Input
            label="子智能体名"
            value={cfg.subagent_name}
            onChange={(e) => onChange({ type: "subagent", subagent_name: e.target.value, task: cfg.task })}
            placeholder="researcher"
          />
          <Input
            label="任务"
            value={cfg.task}
            onChange={(e) => onChange({ type: "subagent", subagent_name: cfg.subagent_name, task: e.target.value })}
            placeholder="委派给子智能体的任务"
          />
        </div>
      );
    }
    case "condition": {
      const cfg = config as { type: "condition"; when: WorkflowCondition };
      return (
        <select
          value={cfg.when}
          onChange={(e) => onChange({ type: "condition", when: e.target.value as WorkflowCondition })}
          className="w-full rounded-lg border border-[var(--border)] bg-[var(--input-bg)] px-3 py-2 text-sm"
        >
          <option value="always">always</option>
          <option value="previous_succeeded">previous_succeeded</option>
        </select>
      );
    }
    case "output": {
      const cfg = config as { type: "output"; template?: string | null };
      return (
        <textarea
          value={cfg.template ?? ""}
          onChange={(e) => onChange({ type: "output", template: e.target.value })}
          rows={2}
          placeholder="输出模板（留空则显示：工作流执行完成）"
          className="w-full rounded-lg border border-[var(--border)] bg-[var(--input-bg)] px-3 py-2 text-sm"
        />
      );
    }
  }
}
