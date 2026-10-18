import { useEffect, useState } from "react";
import {
  Plus, Trash2, Pencil, CheckCircle2, Loader2, AlertTriangle,
} from "lucide-react";
import {
  listWorkflows, createWorkflow, updateWorkflow, deleteWorkflow, activateWorkflow,
  startWorkflowRun,
  type Workflow,
} from "../api/client";
import PageShell from "../components/layout/PageShell";
import {
  Badge,
  Button,
  ConfirmDialog,
  EmptyState,
  Input,
  Modal,
  Panel,
  Spinner,
  Tabs,
  Textarea,
} from "../components/ui";
import RuntimeWorkflowList from "../components/workflow/RuntimeWorkflowList";
import WorkflowGraphEditor from "../components/workflow/WorkflowGraphEditor";
import WorkflowRunInspector from "../components/workflow/WorkflowRunInspector";
import WorkflowRecentRuns from "../components/workflow/WorkflowRecentRuns";
import type { WorkflowGraphRecord } from "../api/client";

const BUILTIN_ICONS: Record<string, string> = {
  "react-default": "🔄",
  "code-review": "🔍",
  "doc-generate": "📄",
  "research": "🔬",
  "multi-agent": "🤖",
  "hitl-approval": "✋",
};

type Tab = "templates" | "runtime";

export default function WorkflowsPage() {
  const [tab, setTab] = useState<Tab>("runtime");
  const [editorGraph, setEditorGraph] = useState<WorkflowGraphRecord | null>(null);
  const [editorOpen, setEditorOpen] = useState(false);
  const [activeRunId, setActiveRunId] = useState<string | null>(null);
  const [runtimeRefresh, setRuntimeRefresh] = useState(0);

  return (
    <PageShell
      title="工作流"
      description="模板工作流用于对话；运行工作流是可执行的 DAG 运行时"
      variant={editorOpen ? "workspace" : "standard"}
      className="page-canvas ui-opt-workflows flex h-full flex-col"
      contentClassName={editorOpen ? "overflow-hidden p-0" : undefined}
      actions={
        !editorOpen ? (
          <Tabs
            tabs={[
              { id: "runtime", label: "运行工作流" },
              { id: "templates", label: "模板工作流" },
            ]}
            activeId={tab}
            onChange={(id) => setTab(id as Tab)}
            variant="segment"
            size="sm"
          />
        ) : null
      }
    >
      {tab === "templates" ? (
        <TemplatesTab />
      ) : (
        <RuntimeTab
          editorGraph={editorGraph}
          setEditorGraph={setEditorGraph}
          editorOpen={editorOpen}
          setEditorOpen={setEditorOpen}
          activeRunId={activeRunId}
          setActiveRunId={setActiveRunId}
          runtimeRefresh={runtimeRefresh}
          setRuntimeRefresh={setRuntimeRefresh}
        />
      )}
    </PageShell>
  );
}

function TemplatesTab() {
  const [workflows, setWorkflows] = useState<Workflow[]>([]);
  const [activeId, setActiveId] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");

  const [showModal, setShowModal] = useState(false);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [formName, setFormName] = useState("");
  const [formDesc, setFormDesc] = useState("");
  const [formNodes, setFormNodes] = useState("");
  const [formTags, setFormTags] = useState("");
  const [formPrompt, setFormPrompt] = useState("");
  const [saving, setSaving] = useState(false);
  const [saveError, setSaveError] = useState("");
  const [deleteConfirm, setDeleteConfirm] = useState<{ id: string; name: string } | null>(null);

  const load = async () => {
    const res = await listWorkflows();
    if (res) {
      setWorkflows(res.workflows);
      setActiveId(res.active_id);
      setError("");
    } else {
      setError("无法连接到后端");
    }
    setLoading(false);
  };

  useEffect(() => { void load(); }, []);

  const openAdd = () => {
    setEditingId(null);
    setFormName("");
    setFormDesc("");
    setFormNodes("");
    setFormTags("");
    setFormPrompt("");
    setSaveError("");
    setShowModal(true);
  };

  const openEdit = (wf: Workflow) => {
    setEditingId(wf.id);
    setFormName(wf.name);
    setFormDesc(wf.description);
    setFormNodes(wf.nodes.join(", "));
    setFormTags(wf.tags.join(", "));
    setFormPrompt(wf.system_prompt_extra || "");
    setSaveError("");
    setShowModal(true);
  };

  const handleSave = async () => {
    if (!formName.trim()) return;
    setSaving(true);
    setSaveError("");
    const nodes = formNodes.trim() ? formNodes.split(",").map((n) => n.trim()).filter(Boolean) : [];
    const tags = formTags.trim() ? formTags.split(",").map((t) => t.trim()).filter(Boolean) : [];
    const payload = {
      name: formName.trim(),
      description: formDesc.trim(),
      nodes,
      tags,
      system_prompt_extra: formPrompt.trim(),
    };
    try {
      const res = editingId
        ? await updateWorkflow(editingId, payload)
        : await createWorkflow(payload);
      if (res) {
        setShowModal(false);
        await load();
      } else {
        setSaveError("保存工作流失败，请重试");
      }
    } catch (e) {
      setSaveError(e instanceof Error ? e.message : "保存工作流失败");
    } finally {
      setSaving(false);
    }
  };

  const handleDelete = async () => {
    if (!deleteConfirm) return;
    try {
      await deleteWorkflow(deleteConfirm.id);
      await load();
    } finally {
      setDeleteConfirm(null);
    }
  };

  const handleActivate = async (id: string) => {
    await activateWorkflow(id);
    setActiveId(id);
    await load();
  };

  if (loading) {
    return (
      <div className="flex justify-center py-20">
        <Spinner className="w-8 h-8" />
      </div>
    );
  }

  if (error && workflows.length === 0) {
    return (
      <EmptyState
        icon={<AlertTriangle className="w-12 h-12 text-[var(--warning)]" />}
        title="无法加载工作流"
        description={error}
        action={<Button variant="secondary" onClick={() => void load()}>重试</Button>}
      />
    );
  }

  return (
    <div className="flex flex-col">
      <div className="flex justify-end mb-4">
        <Button size="sm" onClick={openAdd}>
          <Plus className="w-4 h-4" />
          自定义工作流
        </Button>
      </div>

      <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
        {workflows.map((wf) => {
          const isActive = wf.id === activeId;
          return (
            <Panel key={wf.id} className={`group cursor-pointer transition-all ${isActive ? "ring-1 ring-[var(--accent)]/50" : ""}`}>
              <div className="flex items-start gap-3 mb-3">
                <span className="text-2xl flex-shrink-0">{BUILTIN_ICONS[wf.id] || "⚡"}</span>
                <div className="flex-1 min-w-0">
                  <div className="flex items-center gap-2">
                    <h3 className="font-semibold text-sm">{wf.name}</h3>
                    {isActive && (
                      <Badge tone="success">
                        <CheckCircle2 className="w-3 h-3" />
                        激活
                      </Badge>
                    )}
                    {wf.is_builtin && <Badge tone="default">内置</Badge>}
                  </div>
                  <div className="flex gap-1 mt-1 flex-wrap">
                    {wf.tags.map((t) => (
                      <span key={t} className="px-1.5 py-0.5 rounded text-[10px] bg-[var(--panel-2)] text-[var(--text-muted)]">
                        {t}
                      </span>
                    ))}
                  </div>
                </div>
                <div className="flex items-center gap-0.5 opacity-0 group-hover:opacity-100 group-focus-within:opacity-100 transition-opacity flex-shrink-0">
                  {!isActive && (
                    <button
                      type="button"
                      onClick={() => void handleActivate(wf.id)}
                      className="p-1.5 rounded hover:bg-[var(--accent)]/20 text-[var(--text-muted)] hover:text-[var(--accent)] focus-ring-token focus-visible:outline-none"
                      title="激活此工作流"
                    >
                      <CheckCircle2 className="w-4 h-4" />
                    </button>
                  )}
                  <button
                    type="button"
                    onClick={() => openEdit(wf)}
                    className="p-1.5 rounded hover:bg-[var(--panel-hover)] text-[var(--text-muted)] hover:text-[var(--text)] focus-ring-token focus-visible:outline-none"
                    title="编辑"
                  >
                    <Pencil className="w-4 h-4" />
                  </button>
                  {!wf.is_builtin && (
                    <button
                      type="button"
                      onClick={() => setDeleteConfirm({ id: wf.id, name: wf.name })}
                      className="p-1.5 rounded hover:bg-[var(--danger)]/20 text-[var(--text-muted)] hover:text-[var(--danger)] focus-ring-token focus-visible:outline-none"
                      title="删除"
                    >
                      <Trash2 className="w-4 h-4" />
                    </button>
                  )}
                </div>
              </div>
              <p className="text-sm text-[var(--text-muted)] mb-3">{wf.description}</p>
              <div className="flex items-center gap-1.5 flex-wrap">
                {wf.nodes.map((n, i) => (
                  <span key={n} className="inline-flex items-center gap-1">
                    <code className="px-2 py-1 rounded text-xs bg-[var(--accent)]/10 text-[var(--accent)] font-mono">{n}</code>
                    {i < wf.nodes.length - 1 && <span className="text-[var(--text-faint)]">→</span>}
                  </span>
                ))}
                {wf.nodes.length === 0 && <span className="text-xs text-[var(--text-faint)]">无预定义流程节点</span>}
              </div>
            </Panel>
          );
        })}

        <button
          type="button"
          onClick={openAdd}
          className="border-2 border-dashed border-[var(--border)] rounded-xl p-5 flex flex-col items-center justify-center text-center min-h-[200px] hover:border-[var(--accent)]/40 transition-colors cursor-pointer group focus-ring-token focus-visible:outline-none"
        >
          <Plus className="w-8 h-8 mb-2 text-[var(--text-faint)] group-hover:text-[var(--accent)] transition-colors" />
          <p className="font-medium text-sm text-[var(--text-muted)] group-hover:text-[var(--text)]">自定义工作流</p>
          <p className="text-xs text-[var(--text-faint)] mt-1">创建专属 AI 执行流程</p>
        </button>
      </div>

      <Modal
        open={showModal}
        onClose={() => setShowModal(false)}
        title={editingId ? "编辑工作流" : "创建工作流"}
        footer={
          <>
            <Button variant="secondary" onClick={() => setShowModal(false)}>取消</Button>
            <Button onClick={() => void handleSave()} disabled={saving}>
              {saving ? <Loader2 className="w-4 h-4 animate-spin" /> : null}
              {editingId ? "保存" : "创建"}
            </Button>
          </>
        }
      >
        <div className="space-y-4">
          {saveError && (
            <div className="rounded-lg border border-[var(--danger)]/30 bg-[var(--danger)]/10 px-3 py-2 text-xs text-[var(--danger)]" role="alert">
              {saveError}
            </div>
          )}
          <Input label="名称" value={formName} onChange={(e) => setFormName(e.target.value)} placeholder="例如：代码审查流水线" />
          <Textarea
            label="描述"
            value={formDesc}
            onChange={(e) => setFormDesc(e.target.value)}
            rows={2}
            placeholder="描述此工作流的用途..."
          />
          <Input label="流程节点 (逗号分隔)" value={formNodes} onChange={(e) => setFormNodes(e.target.value)} placeholder="scan, analyze, report" hint="节点将在对话中作为流程步骤展示" />
          <Input label="标签 (逗号分隔)" value={formTags} onChange={(e) => setFormTags(e.target.value)} placeholder="代码, 安全" />
          <Textarea
            label="额外系统提示词"
            value={formPrompt}
            onChange={(e) => setFormPrompt(e.target.value)}
            rows={3}
            placeholder="额外的 AI 行为指导，将在对话时注入系统提示词..."
          />
        </div>
      </Modal>

      <ConfirmDialog
        open={Boolean(deleteConfirm)}
        title="删除工作流"
        description={`确定删除工作流 "${deleteConfirm?.name}" 吗？此操作不可逆。`}
        confirmLabel="删除"
        tone="danger"
        onConfirm={() => void handleDelete()}
        onCancel={() => setDeleteConfirm(null)}
      />
    </div>
  );
}

function RuntimeTab({
  editorGraph,
  setEditorGraph,
  editorOpen,
  setEditorOpen,
  activeRunId,
  setActiveRunId,
  runtimeRefresh,
  setRuntimeRefresh,
}: {
  editorGraph: WorkflowGraphRecord | null;
  setEditorGraph: (g: WorkflowGraphRecord | null) => void;
  editorOpen: boolean;
  setEditorOpen: (open: boolean) => void;
  activeRunId: string | null;
  setActiveRunId: (id: string | null) => void;
  runtimeRefresh: number;
  setRuntimeRefresh: (n: number) => void;
}) {
  const [runError, setRunError] = useState("");
  const [selectedNodeId, setSelectedNodeId] = useState<string | null>(null);
  const [highlightedNodeId, setHighlightedNodeId] = useState<string | null>(null);
  const [locateNodeId, setLocateNodeId] = useState<string | null>(null);

  const handleRun = async (graph: WorkflowGraphRecord) => {
    setRunError("");
    const res = await startWorkflowRun(graph.id);
    if (res.ok) {
      setActiveRunId(res.data.run_id);
      setRuntimeRefresh(runtimeRefresh + 1);
    } else {
      setRunError(res.error);
    }
  };

  const handleCreate = () => {
    setEditorGraph(null);
    setActiveRunId(null);
    setSelectedNodeId(null);
    setHighlightedNodeId(null);
    setLocateNodeId(null);
    setEditorOpen(true);
  };

  if (editorOpen) {
    return (
      <div className="flex h-full min-h-0 flex-1 flex-col">
        {runError && (
          <div className="mx-4 mt-3 rounded-lg border border-[var(--danger)]/30 bg-[var(--danger)]/10 px-3 py-2 text-sm text-[var(--danger)]" role="alert">
            {runError}
          </div>
        )}
        <div className="flex-1 min-h-0 overflow-hidden">
          <WorkflowGraphEditor
            graph={editorGraph}
            onRun={handleRun}
            selectedNodeId={selectedNodeId}
            onSelectedNodeChange={setSelectedNodeId}
            highlightedNodeId={highlightedNodeId}
            locateNodeId={locateNodeId}
            onSaved={(graph) => {
              setEditorGraph(graph);
              setRuntimeRefresh(runtimeRefresh + 1);
            }}
            onClose={() => setEditorOpen(false)}
          />
        </div>
        {activeRunId && (
          <div className="shrink-0 max-h-[40vh] overflow-y-auto border-t border-[var(--border-soft)]">
            <WorkflowRunInspector
              runId={activeRunId}
              onClose={() => setActiveRunId(null)}
              selectedNodeId={selectedNodeId}
              onNodeSelect={(id) => setSelectedNodeId(id)}
              onNodeHighlight={(id) => setHighlightedNodeId(id)}
              onLocateNode={(id) => setLocateNodeId(id)}
            />
          </div>
        )}
      </div>
    );
  }

  return (
    <div className="space-y-6">
      <div className="flex justify-end">
        <Button size="sm" onClick={handleCreate}>
          <Plus className="w-4 h-4" />
          新建运行工作流
        </Button>
      </div>

      {runError && (
        <div className="rounded-lg border border-[var(--danger)]/30 bg-[var(--danger)]/10 px-3 py-2 text-sm text-[var(--danger)]">
          {runError}
        </div>
      )}

      <RuntimeWorkflowList
        onRun={handleRun}
        onEdit={(graph) => { setEditorGraph(graph); setEditorOpen(true); }}
        onCreate={handleCreate}
        refreshKey={runtimeRefresh}
      />

      {activeRunId && (
        <WorkflowRunInspector runId={activeRunId} onClose={() => setActiveRunId(null)} />
      )}

      <WorkflowRecentRuns
        onSelectRun={setActiveRunId}
        refreshKey={runtimeRefresh}
      />

    </div>
  );
}
