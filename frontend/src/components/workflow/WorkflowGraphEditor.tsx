import { useMemo, useState } from "react";
import { ArrowLeft, Check, CircleAlert, GitBranch, Play, Plus, Save, Trash2, X } from "lucide-react";
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
import { Badge, Button, EmptyState, Input, Textarea } from "../ui";
import { getNodePresentation, getSaveState, layoutWorkflowNodes, workflowDefinitionsEqual } from "./workflowEditorModel";

export const WORKFLOW_NODE_KINDS: WorkflowNodeKind[] = ["agent", "tool", "subagent", "condition", "output"];

const KIND_HINTS: Record<WorkflowNodeKind, string> = {
  agent: "生成文字结果，不调用工具",
  tool: "通过安全网关执行工具",
  subagent: "委派给已发现的子智能体",
  condition: "使用已有条件决定是否继续",
  output: "输出模板或结束标记",
};

const TONE_CLASSES: Record<string, string> = {
  purple: "workflow-node-tone-purple",
  blue: "workflow-node-tone-blue",
  pink: "workflow-node-tone-pink",
  gold: "workflow-node-tone-gold",
  green: "workflow-node-tone-green",
};

function emptyNode(kind: WorkflowNodeKind, id: string): WorkflowNodeDefinition {
  const config: WorkflowNodeConfig = (() => {
    switch (kind) {
      case "agent": return { type: "agent", prompt: "" };
      case "tool": return { type: "tool", tool_name: "", arguments: {} };
      case "subagent": return { type: "subagent", subagent_name: "", task: "" };
      case "condition": return { type: "condition", when: "always" };
      case "output": return { type: "output", template: null };
    }
  })();
  return { id, kind, config };
}

export function validateGraphClientSide(definition: WorkflowGraphDefinition): string | null {
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
    if (!edge.from || !edge.to) return "连接必须指定起点和终点";
    if (!ids.has(edge.from) || !ids.has(edge.to)) return "连接引用了不存在的节点";
  }
  return null;
}

function definitionFor(nodes: WorkflowNodeDefinition[], edges: WorkflowGraphDefinition["edges"], entryNodeId: string): WorkflowGraphDefinition {
  return { schema_version: 1, entry_node_id: entryNodeId, nodes, edges };
}

export default function WorkflowGraphEditor({
  graph,
  onSaved,
  onClose,
  onRun,
}: {
  graph: WorkflowGraphRecord | null;
  onSaved: (graph: WorkflowGraphRecord) => void;
  onClose: () => void;
  onRun?: (graph: WorkflowGraphRecord) => void;
}) {
  const [persistedGraph, setPersistedGraph] = useState<WorkflowGraphRecord | null>(graph);
  const [name, setName] = useState(graph?.name ?? "");
  const [description, setDescription] = useState(graph?.description ?? "");
  const [nodes, setNodes] = useState<WorkflowNodeDefinition[]>(graph?.definition.nodes ?? []);
  const [edges, setEdges] = useState<WorkflowGraphDefinition["edges"]>(graph?.definition.edges ?? []);
  const [entryNodeId, setEntryNodeId] = useState(graph?.definition.entry_node_id ?? "");
  const [savedSnapshot, setSavedSnapshot] = useState({
    name: graph?.name ?? "",
    description: graph?.description ?? "",
    definition: graph?.definition ?? definitionFor([], [], ""),
  });
  const [selectedNodeId, setSelectedNodeId] = useState<string | null>(graph?.definition.nodes[0]?.id ?? null);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState("");
  const [validationMessage, setValidationMessage] = useState("");

  const definition = useMemo(() => definitionFor(nodes, edges, entryNodeId), [nodes, edges, entryNodeId]);
  const dirty = name !== savedSnapshot.name || description !== savedSnapshot.description || !workflowDefinitionsEqual(definition, savedSnapshot.definition);
  const saveState = getSaveState({ dirty, saving, error });
  const layout = useMemo(() => layoutWorkflowNodes(definition), [definition]);
  const selectedNode = nodes.find((node) => node.id === selectedNodeId) ?? null;

  const updateNode = (nodeId: string, patch: Partial<WorkflowNodeDefinition>) => {
    setNodes((current) => current.map((node) => (node.id === nodeId ? { ...node, ...patch } : node)));
    if (patch.id && entryNodeId === nodeId) setEntryNodeId(patch.id);
    if (patch.id && selectedNodeId === nodeId) setSelectedNodeId(patch.id);
  };

  const updateNodeConfig = (nodeId: string, config: WorkflowNodeConfig) => {
    setNodes((current) => current.map((node) => (node.id === nodeId ? { ...node, config } : node)));
  };

  const addNode = (kind: WorkflowNodeKind) => {
    const base = `${kind}-${nodes.length + 1}`;
    let id = base;
    let suffix = 2;
    while (nodes.some((node) => node.id === id)) {
      id = `${base}-${suffix}`;
      suffix += 1;
    }
    setNodes((current) => [...current, emptyNode(kind, id)]);
    if (!entryNodeId) setEntryNodeId(id);
    setSelectedNodeId(id);
    setValidationMessage("");
  };

  const removeNode = (nodeId: string) => {
    setNodes((current) => current.filter((node) => node.id !== nodeId));
    setEdges((current) => current.filter((edge) => edge.from !== nodeId && edge.to !== nodeId));
    if (entryNodeId === nodeId) setEntryNodeId("");
    if (selectedNodeId === nodeId) setSelectedNodeId(null);
  };

  const addEdge = () => {
    if (nodes.length < 2) return;
    const from = nodes[0].id;
    const to = nodes[1].id;
    if (edges.some((edge) => edge.from === from && edge.to === to)) return;
    setEdges((current) => [...current, { from, to }]);
  };

  const handleValidate = () => {
    const clientError = validateGraphClientSide(definition);
    setError(clientError ?? "");
    setValidationMessage(clientError ?? "本地结构校验通过，保存时仍由后端进行权威校验。");
  };

  const handleSave = async () => {
    const clientError = validateGraphClientSide(definition);
    if (clientError) {
      setError(clientError);
      setValidationMessage("");
      return;
    }
    if (!name.trim()) {
      setError("请先填写工作流名称");
      return;
    }
    setSaving(true);
    setError("");
    setValidationMessage("");
    const payload = { name: name.trim(), description: description.trim(), definition };
    const result = persistedGraph ? await updateWorkflowGraph(persistedGraph.id, payload) : await createWorkflowGraph(payload);
    setSaving(false);
    if (!result.ok) {
      setError(result.error);
      return;
    }
    setPersistedGraph(result.data);
    setSavedSnapshot({ name: result.data.name, description: result.data.description, definition: result.data.definition });
    setName(result.data.name);
    setDescription(result.data.description);
    setValidationMessage("已保存");
    onSaved(result.data);
  };

  const handleNodeKeyDown = (event: React.KeyboardEvent<HTMLButtonElement>, nodeId: string) => {
    if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      setSelectedNodeId(nodeId);
    } else if (event.key === "Delete") {
      event.preventDefault();
      removeNode(nodeId);
    }
  };

  return (
    <div className="workflow-editor-page" data-testid="workflow-editor">
      <header className="workflow-editor-header">
        <div className="flex min-w-0 items-center gap-3">
          <Button variant="ghost" size="sm" onClick={onClose} aria-label="返回工作流列表"><ArrowLeft className="h-4 w-4" /></Button>
          <div className="min-w-0">
            <div className="flex items-center gap-2">
              <GitBranch className="h-4 w-4 shrink-0 text-[var(--accent-purple)]" />
              <input aria-label="工作流名称" value={name} onChange={(event) => setName(event.target.value)} placeholder="未命名工作流" className="workflow-editor-title-input" />
              <span className={`workflow-save-state workflow-save-state-${saveState.tone}`}>
                {saveState.tone === "danger" ? <CircleAlert className="h-3.5 w-3.5" /> : null}{saveState.label}
              </span>
            </div>
            <input aria-label="工作流描述" value={description} onChange={(event) => setDescription(event.target.value)} placeholder="补充这个工作流的用途" className="workflow-editor-description-input" />
          </div>
        </div>
        <div className="flex shrink-0 items-center gap-2">
          <Button variant="ghost" size="sm" onClick={handleValidate}><Check className="h-3.5 w-3.5" />校验</Button>
          {onRun && persistedGraph && <Button variant="secondary" size="sm" onClick={() => onRun(persistedGraph)} disabled={dirty}><Play className="h-3.5 w-3.5" />运行</Button>}
          <Button size="sm" onClick={handleSave} disabled={saving || !dirty}><Save className="h-3.5 w-3.5" />保存</Button>
        </div>
      </header>

      {error && <div className="workflow-editor-alert workflow-editor-alert-danger" role="alert"><CircleAlert className="h-4 w-4 shrink-0" /><span>{error}</span><button type="button" className="ml-auto text-xs underline underline-offset-2" onClick={handleSave}>重试保存</button><button type="button" onClick={() => setError("")} aria-label="关闭错误提示"><X className="h-4 w-4" /></button></div>}
      {validationMessage && !error && <div className="workflow-editor-alert workflow-editor-alert-neutral" role="status"><Check className="h-4 w-4 shrink-0 text-[var(--success)]" /><span>{validationMessage}</span></div>}

      <div className="workflow-editor-columns">
        <aside className="workflow-editor-library scrollbar-thin">
          <section>
            <div className="workflow-editor-section-title">节点库</div>
            <p className="workflow-editor-section-hint">添加真实可执行节点到画布</p>
            <div className="mt-3 space-y-2">
              {WORKFLOW_NODE_KINDS.map((kind) => {
                const presentation = getNodePresentation(kind);
                return <button type="button" key={kind} data-testid={`workflow-node-library-${kind}`} className={`workflow-library-item ${TONE_CLASSES[presentation.tone]}`} onClick={() => addNode(kind)}><span className="workflow-library-icon">+</span><span className="min-w-0 text-left"><span className="block text-sm font-medium">{presentation.label}</span><span className="mt-0.5 block text-[11px] text-[var(--text-secondary)]">{KIND_HINTS[kind]}</span></span></button>;
              })}
            </div>
          </section>
          <section className="workflow-editor-library-note"><p className="text-xs font-medium text-[var(--text)]">编辑提示</p><p className="mt-1 text-xs leading-5 text-[var(--text-secondary)]">节点位置仅用于当前页面展示，不会写入 Workflow 数据。</p></section>
        </aside>

        <main className="workflow-editor-main">
          <section className="workflow-canvas-panel" aria-label="工作流画布">
            <div className="workflow-panel-heading"><div><h2 className="text-sm font-semibold">画布</h2><p className="text-xs text-[var(--text-secondary)]">从入口节点开始查看真实执行连接</p></div><Badge tone="default">{nodes.length} 节点 · {edges.length} 连接</Badge></div>
            <div className="workflow-canvas-viewport scrollbar-thin">
              {layout.nodes.length === 0 ? <EmptyState icon={<GitBranch className="h-6 w-6" />} title="画布还是空的" description="从左侧节点库添加一个真实节点，开始搭建工作流。" className="min-h-[360px]" /> : <div className="workflow-canvas" style={{ width: layout.width, minHeight: layout.height }}>
                <svg className="workflow-canvas-edges" width={layout.width} height={layout.height} aria-hidden="true"><defs><marker id="workflow-edge-arrow" markerWidth="8" markerHeight="8" refX="7" refY="4" orient="auto"><path d="M0,0 L8,4 L0,8 z" fill="var(--text-faint)" /></marker></defs>{layout.edges.map((edge) => { const from = layout.nodes.find((node) => node.id === edge.from); const to = layout.nodes.find((node) => node.id === edge.to); if (!from || !to) return null; return <line key={`${edge.from}-${edge.to}`} x1={from.x + 190} y1={from.y + 56} x2={to.x} y2={to.y + 56} stroke="var(--border-soft)" strokeWidth="2" markerEnd="url(#workflow-edge-arrow)" />; })}</svg>
                {layout.nodes.map(({ node, x, y }) => { const presentation = getNodePresentation(node.kind); const selected = selectedNodeId === node.id; return <button type="button" key={node.id} className={`workflow-canvas-node ${TONE_CLASSES[presentation.tone]} ${selected ? "workflow-canvas-node-selected" : ""}`} style={{ left: x, top: y }} aria-label={`选择节点 ${node.id}`} aria-pressed={selected} onClick={() => setSelectedNodeId(node.id)} onKeyDown={(event) => handleNodeKeyDown(event, node.id)}><span className="flex items-center justify-between gap-2"><span className="truncate text-[11px] font-medium uppercase tracking-[0.08em]">{presentation.label}</span>{entryNodeId === node.id && <span className="workflow-entry-dot" title="入口节点" />}</span><span className="mt-3 block truncate text-sm font-semibold">{node.id || "未命名节点"}</span><span className="mt-1 block truncate text-[11px] text-[var(--text-secondary)]">{KIND_HINTS[node.kind]}</span></button>; })}
              </div>}
            </div>
          </section>

          <section className="workflow-edges-panel" aria-label="工作流连接">
            <div className="workflow-panel-heading"><div><h2 className="text-sm font-semibold">连接</h2><p className="text-xs text-[var(--text-secondary)]">保留当前真实 from / to 关系</p></div><Button size="sm" variant="secondary" onClick={addEdge} disabled={nodes.length < 2}><Plus className="h-3.5 w-3.5" />添加连接</Button></div>
            {edges.length === 0 ? <p className="px-4 pb-4 text-xs text-[var(--text-faint)]">还没有连接。节点可以先独立编辑，保存时会由后端继续校验图结构。</p> : <div className="space-y-2 px-4 pb-4">{edges.map((edge, index) => <div className="workflow-edge-row" key={`${edge.from}-${edge.to}-${index}`}><select aria-label={`连接 ${index + 1} 起点`} value={edge.from} onChange={(event) => setEdges((current) => current.map((item, edgeIndex) => edgeIndex === index ? { ...item, from: event.target.value } : item))}>{nodes.map((node) => <option key={node.id} value={node.id}>{node.id}</option>)}</select><span>→</span><select aria-label={`连接 ${index + 1} 终点`} value={edge.to} onChange={(event) => setEdges((current) => current.map((item, edgeIndex) => edgeIndex === index ? { ...item, to: event.target.value } : item))}>{nodes.map((node) => <option key={node.id} value={node.id}>{node.id}</option>)}</select><button type="button" onClick={() => setEdges((current) => current.filter((_, edgeIndex) => edgeIndex !== index))} aria-label={`删除连接 ${edge.from} 到 ${edge.to}`} className="ml-auto rounded p-1 text-[var(--text-secondary)] hover:bg-[var(--danger-soft)] hover:text-[var(--danger)]"><Trash2 className="h-3.5 w-3.5" /></button></div>)}</div>}
          </section>
        </main>

        <aside className="workflow-editor-inspector scrollbar-thin">
          <div className="workflow-editor-section-title">Inspector</div><p className="workflow-editor-section-hint">只显示当前选中节点的真实配置</p>
          {selectedNode ? <NodeInspector node={selectedNode} isEntry={entryNodeId === selectedNode.id} onUpdate={(patch) => updateNode(selectedNode.id, patch)} onConfigChange={(config) => updateNodeConfig(selectedNode.id, config)} onSetEntry={() => setEntryNodeId(selectedNode.id)} onDelete={() => removeNode(selectedNode.id)} /> : <EmptyState icon={<GitBranch className="h-5 w-5" />} title="选择一个节点" description="节点的实际配置会显示在这里。" className="min-h-[260px] px-3 py-10" />}
        </aside>
      </div>
    </div>
  );
}

function NodeInspector({ node, isEntry, onUpdate, onConfigChange, onSetEntry, onDelete }: { node: WorkflowNodeDefinition; isEntry: boolean; onUpdate: (patch: Partial<WorkflowNodeDefinition>) => void; onConfigChange: (config: WorkflowNodeConfig) => void; onSetEntry: () => void; onDelete: () => void }) {
  const presentation = getNodePresentation(node.kind);
  return <div className="mt-4 space-y-4">
    <div className="workflow-inspector-node-heading"><div className={`workflow-inspector-kind ${TONE_CLASSES[presentation.tone]}`}>{presentation.label}</div><button type="button" onClick={onDelete} className="rounded p-1.5 text-[var(--text-secondary)] hover:bg-[var(--danger-soft)] hover:text-[var(--danger)]" aria-label="删除节点"><Trash2 className="h-4 w-4" /></button></div>
    <Input label="节点 ID" value={node.id} onChange={(event) => onUpdate({ id: event.target.value })} />
    <div><label htmlFor="workflow-node-kind" className="mb-1.5 block text-sm font-medium">节点类型</label><select id="workflow-node-kind" value={node.kind} onChange={(event) => { const kind = event.target.value as WorkflowNodeKind; onUpdate({ kind, config: emptyNode(kind, node.id).config }); }} className="w-full rounded-[var(--radius-md)] border border-[var(--border-soft)] bg-[var(--surface-solid)] px-3 py-2 text-sm text-[var(--text)] focus-ring-token focus:outline-none">{WORKFLOW_NODE_KINDS.map((kind) => <option key={kind} value={kind}>{getNodePresentation(kind).label}</option>)}</select></div>
    <label className="workflow-entry-toggle"><input type="radio" name="workflow-entry-node" checked={isEntry} onChange={onSetEntry} /><span>设为入口节点</span></label>
    <NodeConfigFields node={node} onChange={onConfigChange} />
    <details className="workflow-technical-details"><summary>查看技术详情</summary><pre>{JSON.stringify({ id: node.id, kind: node.kind, config: node.config }, null, 2)}</pre></details>
  </div>;
}

function NodeConfigFields({ node, onChange }: { node: WorkflowNodeDefinition; onChange: (config: WorkflowNodeConfig) => void }) {
  switch (node.kind) {
    case "agent": { const config = node.config as Extract<WorkflowNodeConfig, { type: "agent" }>; return <Textarea label="Agent 指令" value={config.prompt} onChange={(event) => onChange({ type: "agent", prompt: event.target.value })} placeholder="输入这个 Agent 节点实际使用的指令" />; }
    case "tool": return <ToolConfigFields node={node} onChange={onChange} />;
    case "subagent": { const config = node.config as Extract<WorkflowNodeConfig, { type: "subagent" }>; return <div className="space-y-3"><Input label="子智能体名" value={config.subagent_name} onChange={(event) => onChange({ type: "subagent", subagent_name: event.target.value, task: config.task })} /><Textarea label="任务" value={config.task} onChange={(event) => onChange({ type: "subagent", subagent_name: config.subagent_name, task: event.target.value })} /></div>; }
    case "condition": { const config = node.config as Extract<WorkflowNodeConfig, { type: "condition" }>; return <div><label htmlFor="workflow-condition" className="mb-1.5 block text-sm font-medium">条件</label><select id="workflow-condition" value={config.when} onChange={(event) => onChange({ type: "condition", when: event.target.value as WorkflowCondition })} className="w-full rounded-[var(--radius-md)] border border-[var(--border-soft)] bg-[var(--surface-solid)] px-3 py-2 text-sm focus-ring-token focus:outline-none"><option value="always">始终继续</option><option value="previous_succeeded">上一步成功时继续</option></select></div>; }
    case "output": { const config = node.config as Extract<WorkflowNodeConfig, { type: "output" }>; return <Textarea label="输出模板" value={config.template ?? ""} onChange={(event) => onChange({ type: "output", template: event.target.value || null })} placeholder="留空则使用运行时默认输出" />; }
  }
}

function ToolConfigFields({ node, onChange }: { node: WorkflowNodeDefinition; onChange: (config: WorkflowNodeConfig) => void }) {
  const config = node.config as Extract<WorkflowNodeConfig, { type: "tool" }>;
  return <div className="space-y-3"><Input label="工具名" value={config.tool_name} onChange={(event) => onChange({ type: "tool", tool_name: event.target.value, arguments: config.arguments })} hint="使用现有工具注册表中的真实名称" /><Textarea label="参数（JSON）" value={JSON.stringify(config.arguments ?? {}, null, 2)} onChange={(event) => { try { const parsed = JSON.parse(event.target.value); if (parsed && typeof parsed === "object" && !Array.isArray(parsed)) onChange({ type: "tool", tool_name: config.tool_name, arguments: parsed }); } catch { /* Keep the last valid arguments. */ } }} className="font-mono text-xs" rows={5} /></div>;
}
