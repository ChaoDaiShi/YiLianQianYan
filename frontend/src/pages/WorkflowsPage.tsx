import { useEffect, useState } from "react";
import {
  Plus, Trash2, Pencil, CheckCircle2,
  Loader2, AlertTriangle,
} from "lucide-react";
import {
  listWorkflows, createWorkflow, updateWorkflow, deleteWorkflow,
  activateWorkflow,
  type Workflow,
} from "../api/client";
import { PageHeader, Button, Badge, Modal, Input, EmptyState, Spinner, Panel } from "../components/ui";

const BUILTIN_ICONS: Record<string, string> = {
  "react-default": "🔄",
  "code-review": "🔍",
  "doc-generate": "📄",
  "research": "🔬",
  "multi-agent": "🤖",
  "hitl-approval": "✋",
};

export default function WorkflowsPage() {
  const [workflows, setWorkflows] = useState<Workflow[]>([]);
  const [activeId, setActiveId] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");

  // Form state
  const [showModal, setShowModal] = useState(false);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [formName, setFormName] = useState("");
  const [formDesc, setFormDesc] = useState("");
  const [formNodes, setFormNodes] = useState("");
  const [formTags, setFormTags] = useState("");
  const [formPrompt, setFormPrompt] = useState("");
  const [saving, setSaving] = useState(false);

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

  useEffect(() => { load(); }, []);

  const openAdd = () => {
    setEditingId(null);
    setFormName("");
    setFormDesc("");
    setFormNodes("");
    setFormTags("");
    setFormPrompt("");
    setShowModal(true);
  };

  const openEdit = (wf: Workflow) => {
    setEditingId(wf.id);
    setFormName(wf.name);
    setFormDesc(wf.description);
    setFormNodes(wf.nodes.join(", "));
    setFormTags(wf.tags.join(", "));
    setFormPrompt(wf.system_prompt_extra || "");
    setShowModal(true);
  };

  const handleSave = async () => {
    if (!formName.trim()) return;
    setSaving(true);
    const nodes = formNodes.trim()
      ? formNodes.split(",").map((n) => n.trim()).filter(Boolean)
      : [];
    const tags = formTags.trim()
      ? formTags.split(",").map((t) => t.trim()).filter(Boolean)
      : [];
    const payload = {
      name: formName.trim(),
      description: formDesc.trim(),
      nodes,
      tags,
      system_prompt_extra: formPrompt.trim(),
    };
    if (editingId) {
      await updateWorkflow(editingId, payload);
    } else {
      await createWorkflow(payload);
    }
    setShowModal(false);
    setSaving(false);
    load();
  };

  const handleDelete = async (id: string, name: string) => {
    if (!window.confirm(`确定删除工作流 "${name}" 吗？`)) return;
    await deleteWorkflow(id);
    load();
  };

  const handleActivate = async (id: string) => {
    await activateWorkflow(id);
    setActiveId(id);
    load();
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
        action={<Button variant="secondary" onClick={load}>重试</Button>}
      />
    );
  }

  return (
    <div className="flex flex-col h-full">
      <PageHeader
        title="工作流"
        description="预定义的 AI 工作流模板，激活后在对话中自动生效"
        actions={
          <Button size="sm" onClick={openAdd}>
            <Plus className="w-4 h-4" />
            自定义工作流
          </Button>
        }
      />

      <div className="flex-1 overflow-y-auto p-6 scrollbar-thin">
        <div className="grid grid-cols-2 gap-4">
          {workflows.map((wf) => {
            const isActive = wf.id === activeId;
            return (
              <Panel
                key={wf.id}
                className={`group cursor-pointer transition-all ${
                  isActive ? "ring-1 ring-[var(--accent)]/50" : ""
                }`}
              >
                <div className="flex items-start gap-3 mb-3">
                  <span className="text-2xl flex-shrink-0">
                    {BUILTIN_ICONS[wf.id] || "⚡"}
                  </span>
                  <div className="flex-1 min-w-0">
                    <div className="flex items-center gap-2">
                      <h3 className="font-semibold text-sm">{wf.name}</h3>
                      {isActive && (
                        <Badge tone="success">
                          <CheckCircle2 className="w-3 h-3" />
                          激活
                        </Badge>
                      )}
                      {wf.is_builtin && (
                        <Badge tone="default">内置</Badge>
                      )}
                    </div>
                    <div className="flex gap-1 mt-1 flex-wrap">
                      {wf.tags.map((t) => (
                        <span
                          key={t}
                          className="px-1.5 py-0.5 rounded text-[10px] bg-[var(--panel-2)] text-[var(--text-muted)]"
                        >
                          {t}
                        </span>
                      ))}
                    </div>
                  </div>

                  {/* Actions */}
                  <div className="flex items-center gap-0.5 opacity-0 group-hover:opacity-100 transition-opacity flex-shrink-0">
                    {!isActive && (
                      <button
                        onClick={() => handleActivate(wf.id)}
                        className="p-1.5 rounded hover:bg-[var(--accent)]/20 text-[var(--text-muted)] hover:text-[var(--accent)]"
                        title="激活此工作流"
                      >
                        <CheckCircle2 className="w-4 h-4" />
                      </button>
                    )}
                    <button
                      onClick={() => openEdit(wf)}
                      className="p-1.5 rounded hover:bg-[var(--panel-hover)] text-[var(--text-muted)] hover:text-[var(--text)]"
                      title="编辑"
                    >
                      <Pencil className="w-4 h-4" />
                    </button>
                    {!wf.is_builtin && (
                      <button
                        onClick={() => handleDelete(wf.id, wf.name)}
                        className="p-1.5 rounded hover:bg-[var(--danger)]/20 text-[var(--text-muted)] hover:text-[var(--danger)]"
                        title="删除"
                      >
                        <Trash2 className="w-4 h-4" />
                      </button>
                    )}
                  </div>
                </div>

                <p className="text-sm text-[var(--text-muted)] mb-3">{wf.description}</p>

                {/* Flow visualization */}
                <div className="flex items-center gap-1.5 flex-wrap">
                  {wf.nodes.map((n, i) => (
                    <span key={n} className="inline-flex items-center gap-1">
                      <code className="px-2 py-1 rounded text-xs bg-[var(--accent)]/10 text-[var(--accent)] font-mono">
                        {n}
                      </code>
                      {i < wf.nodes.length - 1 && (
                        <span className="text-[var(--text-faint)]">→</span>
                      )}
                    </span>
                  ))}
                  {wf.nodes.length === 0 && (
                    <span className="text-xs text-[var(--text-faint)]">无预定义流程节点</span>
                  )}
                </div>

                {wf.system_prompt_extra && (
                  <p className="mt-2 text-xs text-[var(--text-faint)] italic line-clamp-2">
                    "{wf.system_prompt_extra.slice(0, 120)}{wf.system_prompt_extra.length > 120 ? "…" : ""}"
                  </p>
                )}
              </Panel>
            );
          })}

          {/* Add custom workflow card */}
          <button
            onClick={openAdd}
            className="border-2 border-dashed border-[var(--border)] rounded-xl p-5 flex flex-col items-center justify-center text-center min-h-[200px] hover:border-[var(--accent)]/40 transition-colors cursor-pointer group"
          >
            <Plus className="w-8 h-8 mb-2 text-[var(--text-faint)] group-hover:text-[var(--accent)] transition-colors" />
            <p className="font-medium text-sm text-[var(--text-muted)] group-hover:text-[var(--text)]">自定义工作流</p>
            <p className="text-xs text-[var(--text-faint)] mt-1">创建专属 AI 执行流程</p>
          </button>
        </div>
      </div>

      {/* Add/Edit Workflow Modal */}
      <Modal
        open={showModal}
        onClose={() => setShowModal(false)}
        title={editingId ? "编辑工作流" : "创建工作流"}
        footer={
          <>
            <Button variant="secondary" onClick={() => setShowModal(false)}>取消</Button>
            <Button onClick={handleSave} disabled={saving}>
              {saving ? <Loader2 className="w-4 h-4 animate-spin" /> : null}
              {editingId ? "保存" : "创建"}
            </Button>
          </>
        }
      >
        <div className="space-y-4">
          <Input
            label="名称"
            value={formName}
            onChange={(e) => setFormName(e.target.value)}
            placeholder="例如：代码审查流水线"
          />
          <div>
            <label className="block text-sm font-medium mb-1">描述</label>
            <textarea
              value={formDesc}
              onChange={(e) => setFormDesc(e.target.value)}
              rows={2}
              placeholder="描述此工作流的用途..."
              className="w-full rounded-lg border border-[var(--border)] bg-[var(--input-bg)] px-3 py-2 text-sm text-[var(--text)] placeholder:text-[var(--text-faint)] focus:outline-none focus:ring-2 focus:ring-[var(--accent)]/40 resize-none"
            />
          </div>
          <Input
            label="流程节点 (逗号分隔)"
            value={formNodes}
            onChange={(e) => setFormNodes(e.target.value)}
            placeholder="scan, analyze, report"
            hint="节点将在对话中作为流程步骤展示"
          />
          <Input
            label="标签 (逗号分隔)"
            value={formTags}
            onChange={(e) => setFormTags(e.target.value)}
            placeholder="代码, 安全"
          />
          <div>
            <label className="block text-sm font-medium mb-1">额外系统提示词</label>
            <textarea
              value={formPrompt}
              onChange={(e) => setFormPrompt(e.target.value)}
              rows={3}
              placeholder="额外的 AI 行为指导，将在对话时注入系统提示词..."
              className="w-full rounded-lg border border-[var(--border)] bg-[var(--input-bg)] px-3 py-2 text-sm text-[var(--text)] placeholder:text-[var(--text-faint)] focus:outline-none focus:ring-2 focus:ring-[var(--accent)]/40 resize-none font-mono"
            />
          </div>
        </div>
      </Modal>
    </div>
  );
}
