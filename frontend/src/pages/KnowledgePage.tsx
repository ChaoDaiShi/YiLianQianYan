import { useEffect, useState } from "react";
import {
  Plus, Search, Trash2, Pencil, Brain, User, FileText, X, Check,
} from "lucide-react";
import { useMemoryStore } from "../stores/memoryStore";
import { Button, Badge, PageHeader, EmptyState, Spinner, Modal, Panel } from "../components/ui";

const CATEGORY_LABELS: Record<string, { label: string }> = {
  fact: { label: "事实" },
  preference: { label: "偏好" },
  knowledge: { label: "知识" },
  note: { label: "笔记" },
};

const SOURCE_LABELS: Record<string, { label: string; icon: React.ReactNode }> = {
  auto: { label: "AI提取", icon: <Brain className="w-3 h-3" /> },
  manual: { label: "手动", icon: <User className="w-3 h-3" /> },
  document: { label: "文档", icon: <FileText className="w-3 h-3" /> },
};

export default function KnowledgePage() {
  const {
    memories, stats, isLoading,
    loadMemories, addMemory, editMemory, removeMemory,
    loadStats, triggerExtraction,
  } = useMemoryStore();

  const [search, setSearch] = useState("");
  const [filterCategory, setFilterCategory] = useState<string>("");
  const [filterSource, setFilterSource] = useState<string>("");
  const [showAddModal, setShowAddModal] = useState(false);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [editContent, setEditContent] = useState("");

  // New memory form
  const [newContent, setNewContent] = useState("");
  const [newCategory, setNewCategory] = useState("fact");

  useEffect(() => {
    loadMemories();
    loadStats();
  }, []);

  // Apply filters when they change
  useEffect(() => {
    const q = search.trim();
    loadMemories({
      q: q || undefined,
      category: filterCategory || undefined,
      source: filterSource || undefined,
    });
  }, [search, filterCategory, filterSource]);

  const handleAdd = async () => {
    if (!newContent.trim()) return;
    await addMemory({ content: newContent.trim(), category: newCategory, source: "manual" });
    setNewContent("");
    setNewCategory("fact");
    setShowAddModal(false);
  };

  const handleEdit = async (id: string) => {
    if (!editContent.trim()) return;
    await editMemory(id, { content: editContent.trim() });
    setEditingId(null);
    setEditContent("");
  };

  const handleDelete = async (id: string) => {
    if (!window.confirm("确定要删除这条记忆吗？")) return;
    await removeMemory(id);
  };

  const handleExtract = async () => {
    await triggerExtraction();
  };

  const formatTime = (ts: number) => {
    const d = new Date(ts);
    return d.toLocaleDateString("zh-CN", { month: "short", day: "numeric", hour: "2-digit", minute: "2-digit" });
  };

  // Compute stats
  const totalAuto = stats?.by_source.find(([s]) => s === "auto")?.[1] || 0;
  const totalManual = stats?.by_source.find(([s]) => s === "manual")?.[1] || 0;
  const totalDoc = stats?.by_source.find(([s]) => s === "document")?.[1] || 0;

  return (
    <div className="flex flex-col h-full">
      <PageHeader
        title="知识库"
        description="管理 AI 长期记忆和知识上下文"
        actions={
          <div className="flex items-center gap-2">
            <Button variant="secondary" size="sm" onClick={handleExtract} title="从最近对话中提取记忆">
              <Brain className="w-4 h-4" />
              提取记忆
            </Button>
            <Button size="sm" onClick={() => setShowAddModal(true)}>
              <Plus className="w-4 h-4" />
              添加记忆
            </Button>
          </div>
        }
      />

      <div className="flex-1 overflow-y-auto p-6 space-y-6 scrollbar-thin">
        {/* Stats */}
        <div className="grid grid-cols-4 gap-4">
          {[
            { label: "总记忆", value: stats?.total || 0, icon: Brain },
            { label: "AI提取", value: totalAuto, icon: Brain },
            { label: "手动添加", value: totalManual, icon: User },
            { label: "文档导入", value: totalDoc, icon: FileText },
          ].map((stat) => (
            <Panel key={stat.label} className="flex items-center gap-3">
              <stat.icon className="w-5 h-5 text-[var(--accent)]" />
              <div>
                <div className="text-2xl font-bold font-mono">{stat.value}</div>
                <div className="text-xs text-[var(--text-faint)]">{stat.label}</div>
              </div>
            </Panel>
          ))}
        </div>

        {/* Filters */}
        <div className="flex items-center gap-3">
          <div className="relative flex-1 max-w-sm">
            <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-[var(--text-faint)]" />
            <input
              type="text"
              value={search}
              onChange={(e) => setSearch(e.target.value)}
              placeholder="搜索记忆..."
              className="w-full pl-9 pr-3 py-2 rounded-lg border border-[var(--border)] bg-[var(--input-bg)] text-sm text-[var(--text)] placeholder:text-[var(--text-faint)] focus:outline-none focus:ring-2 focus:ring-[var(--accent)]/40"
            />
          </div>

          <select
            value={filterCategory}
            onChange={(e) => setFilterCategory(e.target.value)}
            className="px-3 py-2 rounded-lg border border-[var(--border)] bg-[var(--input-bg)] text-sm text-[var(--text)] focus:outline-none focus:ring-2 focus:ring-[var(--accent)]/40"
          >
            <option value="">全部分类</option>
            {Object.entries(CATEGORY_LABELS).map(([k, v]) => (
              <option key={k} value={k}>{v.label}</option>
            ))}
          </select>

          <select
            value={filterSource}
            onChange={(e) => setFilterSource(e.target.value)}
            className="px-3 py-2 rounded-lg border border-[var(--border)] bg-[var(--input-bg)] text-sm text-[var(--text)] focus:outline-none focus:ring-2 focus:ring-[var(--accent)]/40"
          >
            <option value="">全部来源</option>
            {Object.entries(SOURCE_LABELS).map(([k, v]) => (
              <option key={k} value={k}>{v.label}</option>
            ))}
          </select>
        </div>

        {/* Memory List */}
        {isLoading ? (
          <div className="flex items-center justify-center py-16">
            <Spinner className="w-6 h-6" />
          </div>
        ) : memories.length === 0 ? (
          <EmptyState
            icon={<Brain className="w-12 h-12" />}
            title="暂无记忆数据"
            description="AI 会自动从对话中提取重要信息，也可以手动添加"
          />
        ) : (
          <div className="space-y-2">
            {memories.map((mem) => (
              <div
                key={mem.id}
                className="border border-[var(--border)] rounded-xl p-4 hover:border-[var(--text-faint)]/30 transition-colors group"
              >
                <div className="flex items-start justify-between gap-3">
                  <div className="flex-1 min-w-0">
                    {editingId === mem.id ? (
                      <div className="flex gap-2">
                        <input
                          type="text"
                          value={editContent}
                          onChange={(e) => setEditContent(e.target.value)}
                          className="flex-1 px-3 py-1.5 rounded-lg border border-[var(--border)] bg-[var(--input-bg)] text-sm text-[var(--text)] focus:outline-none focus:ring-2 focus:ring-[var(--accent)]/40"
                          autoFocus
                          onKeyDown={(e) => {
                            if (e.key === "Enter") handleEdit(mem.id);
                            if (e.key === "Escape") { setEditingId(null); setEditContent(""); }
                          }}
                        />
                        <button onClick={() => handleEdit(mem.id)} className="p-1.5 rounded-lg bg-[var(--accent)] text-[var(--accent-fg)] hover:opacity-90 transition-opacity"><Check className="w-4 h-4" /></button>
                        <button onClick={() => { setEditingId(null); setEditContent(""); }} className="p-1.5 rounded-lg border border-[var(--border)] hover:bg-[var(--panel-hover)] text-[var(--text-muted)]"><X className="w-4 h-4" /></button>
                      </div>
                    ) : (
                      <p className="text-sm leading-relaxed">{mem.content}</p>
                    )}
                  </div>

                  {editingId !== mem.id && (
                    <div className="flex items-center gap-1 opacity-0 group-hover:opacity-100 transition-opacity flex-shrink-0">
                      <button
                        onClick={() => { setEditingId(mem.id); setEditContent(mem.content); }}
                        className="p-1 rounded hover:bg-[var(--panel-hover)] text-[var(--text-faint)] hover:text-[var(--text)]"
                      >
                        <Pencil className="w-3.5 h-3.5" />
                      </button>
                      <button
                        onClick={() => handleDelete(mem.id)}
                        className="p-1 rounded hover:bg-[var(--danger)]/20 text-[var(--text-faint)] hover:text-[var(--danger)]"
                      >
                        <Trash2 className="w-3.5 h-3.5" />
                      </button>
                    </div>
                  )}
                </div>

                <div className="flex items-center gap-2 mt-2 flex-wrap">
                  <Badge tone="default">
                    {CATEGORY_LABELS[mem.category]?.label || mem.category}
                  </Badge>
                  <span className="inline-flex items-center gap-1 px-1.5 py-0.5 rounded text-[10px] bg-[var(--panel-2)] text-[var(--text-muted)]">
                    {SOURCE_LABELS[mem.source]?.icon}
                    {SOURCE_LABELS[mem.source]?.label || mem.source}
                  </span>
                  {mem.source_conversation_id && (
                    <span className="text-[10px] text-[var(--text-faint)] font-mono">
                      {mem.source_conversation_id.slice(0, 8)}...
                    </span>
                  )}
                  <span className="text-[10px] text-[var(--text-faint)] ml-auto">{formatTime(mem.updated_at)}</span>
                </div>
              </div>
            ))}
          </div>
        )}
      </div>

      {/* Add Memory Modal */}
      <Modal open={showAddModal} onClose={() => setShowAddModal(false)} title="添加记忆">
        <div className="space-y-4">
          <div>
            <label className="block text-sm font-medium mb-1">记忆内容</label>
            <textarea
              value={newContent}
              onChange={(e) => setNewContent(e.target.value)}
              rows={3}
              placeholder="例如：用户偏好使用 TypeScript，项目使用 Tailwind CSS..."
              className="w-full rounded-lg border border-[var(--border)] bg-[var(--input-bg)] px-3 py-2 text-sm text-[var(--text)] placeholder:text-[var(--text-faint)] focus:outline-none focus:ring-2 focus:ring-[var(--accent)]/40 resize-none"
              autoFocus
            />
          </div>
          <div>
            <label className="block text-sm font-medium mb-1">分类</label>
            <select
              value={newCategory}
              onChange={(e) => setNewCategory(e.target.value)}
              className="w-full rounded-lg border border-[var(--border)] bg-[var(--input-bg)] px-3 py-2 text-sm text-[var(--text)] focus:outline-none focus:ring-2 focus:ring-[var(--accent)]/40"
            >
              {Object.entries(CATEGORY_LABELS).map(([k, v]) => (
                <option key={k} value={k}>{v.label}</option>
              ))}
            </select>
          </div>
        </div>
        <div className="flex justify-end gap-2 mt-6">
          <Button variant="secondary" onClick={() => setShowAddModal(false)}>取消</Button>
          <Button onClick={handleAdd}>添加</Button>
        </div>
      </Modal>
    </div>
  );
}
