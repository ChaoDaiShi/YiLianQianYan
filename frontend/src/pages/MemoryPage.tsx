import WorkspaceIcon from "../components/ui/WorkspaceIcon";
import { useEffect, useMemo, useState } from "react";
import { Clock3, Pencil, Plus, RefreshCw, Search, Trash2, X } from "lucide-react";
import type { MemoryRecord } from "../api/client";
import { listWithdrawnMemories } from "../api/memory";
import MemoryDetail from "../features/memory/MemoryDetail";
import { useMemoryStore } from "../stores/memoryStore";
import PageShell from "../components/layout/PageShell";
import { Badge, Button, EmptyState, ErrorState, Modal, Skeleton, Textarea } from "../components/ui";
import { knowledgeDocumentForMemory } from "../features/memory/knowledgeMemory";
import {
  categoryBadgeTone,
  memoryCategoryCounts,
  formatMemoryTime,
  getMemoryCategoryPresentation,
  getMemorySourceLabel,
  MEMORY_CATEGORY_OPTIONS,
  truncateMemoryContent,
} from "../features/memory/memoryCenterModel";

const SOURCE_OPTIONS = [
  { value: "", label: "全部来源" },
  { value: "auto", label: "AI 提取" },
  { value: "manual", label: "手动" },
  { value: "document", label: "文档" },
] as const;

export default function MemoryPage() {
  const {
    memories: activeMemories,
    stats,
    isLoading,
    error,
    loadMemories,
    loadStats,
    addMemory,
    editMemory,
    removeMemory,
    isReindexing,
    reindexResult,
    reindexMemoriesAction,
  } = useMemoryStore();

  const [view, setView] = useState<"active" | "withdrawn">("active");
  const [withdrawn, setWithdrawn] = useState<MemoryRecord[]>([]);
  const [withdrawnLoading, setWithdrawnLoading] = useState(false);
  const [detailOpen, setDetailOpen] = useState(false);
  const [withdrawnError, setWithdrawnError] = useState<string | null>(null);
  const [search, setSearch] = useState("");
  const [filterCategory, setFilterCategory] = useState("");
  const [filterSource, setFilterSource] = useState("");
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const memories = view === "active" ? activeMemories : withdrawn.filter((memory) =>
    (!search.trim() || memory.content.includes(search.trim())) && (!filterCategory || memory.category === filterCategory) && (!filterSource || memory.source === filterSource));
  const loadWithdrawn = async () => {
    setWithdrawnLoading(true);
    try { setWithdrawn(await listWithdrawnMemories()); setWithdrawnError(null); }
    catch (cause) { setWithdrawnError(cause instanceof Error ? cause.message : String(cause)); }
    finally { setWithdrawnLoading(false); }
  };
  useEffect(() => { if (view === "withdrawn") void loadWithdrawn(); }, [view]);


  // Add modal state
  const [showAddModal, setShowAddModal] = useState(false);
  const [newContent, setNewContent] = useState("");
  const [newCategory, setNewCategory] = useState("fact");
  const [isAdding, setIsAdding] = useState(false);
  const [addError, setAddError] = useState<string | null>(null);

  // Edit modal state
  const [editingMemory, setEditingMemory] = useState<MemoryRecord | null>(null);
  const [editContent, setEditContent] = useState("");
  const [isEditing, setIsEditing] = useState(false);
  const [editError, setEditError] = useState<string | null>(null);

  // Delete state
  const [isDeleting, setIsDeleting] = useState(false);
  const [deleteError, setDeleteError] = useState<string | null>(null);

  useEffect(() => {
    loadStats();
  }, []);

  useEffect(() => {
    loadMemories({
      q: search.trim() || undefined,
      category: filterCategory || undefined,
      source: filterSource || undefined,
    });
  }, [search, filterCategory, filterSource]);

  useEffect(() => {
    if (!selectedId || !memories.some((memory) => memory.id === selectedId)) {
      setSelectedId(memories[0]?.id ?? null);
    }
  }, [memories, selectedId]);

  const selectedMemory = useMemo(
    () => memories.find((memory) => memory.id === selectedId) ?? null,
    [memories, selectedId]
  );
  const listLoading = view === "active" ? isLoading : withdrawnLoading;
  const listError = view === "active" ? error : withdrawnError;

  const hasActiveFilter = Boolean(search.trim() || filterCategory || filterSource);

  const reload = () => {
    if (view === "withdrawn") void loadWithdrawn();
    loadMemories({
      q: search.trim() || undefined,
      category: filterCategory || undefined,
      source: filterSource || undefined,
    });
    loadStats();
  };

  const handleClearFilters = () => {
    setSearch("");
    setFilterCategory("");
    setFilterSource("");
  };

  const handleAdd = async () => {
    const trimmed = newContent.trim();
    if (!trimmed || isAdding) return;
    setIsAdding(true);
    setAddError(null);
    try {
      const created = await addMemory({ content: trimmed, category: newCategory, source: "manual" });
      if (created) {
        setNewContent("");
        setNewCategory("fact");
        setShowAddModal(false);
        setAddError(null);
        setSelectedId(created.id);
      } else {
        const storeError = useMemoryStore.getState().error;
        setAddError(storeError || "添加记忆失败，请重试。");
      }
    } catch (err) {
      setAddError(err instanceof Error ? err.message : String(err));
    } finally {
      setIsAdding(false);
    }
  };

  const handleOpenEdit = (memory: MemoryRecord) => {
    if (knowledgeDocumentForMemory(memory) || view === "withdrawn") return;
    setEditingMemory(memory);
    setEditContent(memory.content);
    setEditError(null);
  };

  const handleEdit = async () => {
    if (!editingMemory || !editContent.trim() || isEditing) return;
    setIsEditing(true);
    setEditError(null);
    try {
      const saved = await editMemory(editingMemory.id, { content: editContent.trim() });
      if (saved) {
        setEditingMemory(null);
        setEditContent("");
        setEditError(null);
      } else {
        const storeError = useMemoryStore.getState().error;
        setEditError(storeError || "保存记忆失败，请重试。");
      }
    } catch (err) {
      setEditError(err instanceof Error ? err.message : String(err));
    } finally {
      setIsEditing(false);
    }
  };

  const handleDelete = async (memory: MemoryRecord) => {
    if (knowledgeDocumentForMemory(memory)) return;
    if (isDeleting) return;
    if (!window.confirm("删除这条记忆？\n\n删除后，这条记录及全部历史版本将永久移除。若需要保留历史，请使用撤回。")) return;
    setIsDeleting(true);
    setDeleteError(null);
    try {
      const targetId = memory.id;
      const currentIndex = memories.findIndex((m) => m.id === targetId);
      await removeMemory(targetId);

      const stateAfter = useMemoryStore.getState();
      const stillExists = stateAfter.memories.some((m) => m.id === targetId);
      if (stillExists || stateAfter.error) {
        setDeleteError(stateAfter.error || "删除失败，记录未能从记忆中心移除。");
      } else {
        const remaining = stateAfter.memories;
        if (remaining.length === 0) {
          setSelectedId(null);
        } else {
          const nextIndex = Math.min(Math.max(0, currentIndex), remaining.length - 1);
          setSelectedId(remaining[nextIndex].id);
        }
        setDeleteError(null);
        setDetailOpen(false);
        if (view === "withdrawn") await loadWithdrawn();
      }
    } catch (err) {
      setDeleteError(err instanceof Error ? err.message : String(err));
    } finally {
      setIsDeleting(false);
    }
  };

  return (
    <PageShell
      title="记忆"
      description="你可以查看、修改，也可以让小涟忘记。"
      variant="workspace"
      className="memory-page page-canvas ui-opt-memory management-memory-page"
      contentClassName="management-memory-content"
      actions={<Button size="sm" onClick={() => { setShowAddModal(true); setAddError(null); }}><Plus className="h-4 w-4" />添加记忆</Button>}
    >
      <div className="management-memory-workspace">
        <section aria-label="记忆分类概览" className="flex flex-wrap items-center gap-2 text-sm">
          <strong>记忆概览</strong>
          <button type="button" className="memory-filter-button" aria-pressed={view === "active" && !filterCategory} onClick={() => { setView("active"); setFilterCategory(""); }}>全部 {stats ? stats.total : "—"}</button>
          {MEMORY_CATEGORY_OPTIONS.filter(option => option.value).map(option => <button key={option.value} type="button" className="memory-filter-button" aria-pressed={view === "active" && filterCategory === option.value} onClick={() => { setView("active"); setFilterCategory(option.value); }}>{option.label} {stats ? memoryCategoryCounts(stats)[option.value] : "—"}</button>)}
        </section>
        <label className="memory-search-field management-memory-search">
          <Search className="h-4 w-4 shrink-0 text-[var(--text-faint)]" />
          <span className="sr-only">搜索记忆</span><input aria-label="搜索记忆" value={search} onChange={event => setSearch(event.target.value)} placeholder="搜索记忆…" />
          {search && <button type="button" onClick={() => setSearch("")} aria-label="清除记忆搜索" className="memory-clear-search"><X className="h-3.5 w-3.5" /></button>}
        </label>
        <div className="management-memory-toolbar">
          <div className="management-status-tabs" aria-label="记忆状态">
            <button type="button" aria-pressed={view === "active"} onClick={() => setView("active")}>当前可用</button>
            <button type="button" aria-pressed={view === "withdrawn"} onClick={() => setView("withdrawn")}>已撤回</button>
          </div>
          <button type="button" onClick={reload} aria-label="刷新记忆列表" className="memory-icon-button"><RefreshCw className="h-4 w-4" /></button>
          <details className="management-memory-filter"><summary>筛选{hasActiveFilter ? " · 已筛选" : ""}</summary><div>
            <label>分类<select aria-label="记忆分类筛选" value={filterCategory} onChange={event => setFilterCategory(event.target.value)}>{MEMORY_CATEGORY_OPTIONS.map(option => <option key={option.value} value={option.value}>{option.label}</option>)}</select></label>
            <label>来源<select aria-label="记忆来源筛选" value={filterSource} onChange={event => setFilterSource(event.target.value)}>{SOURCE_OPTIONS.map(option => <option key={option.value} value={option.value}>{option.label}</option>)}</select></label>
            {hasActiveFilter && <Button size="sm" variant="ghost" onClick={handleClearFilters}>清除筛选</Button>}
          </div></details>
        </div>
        {deleteError && !detailOpen && <p role="alert" className="text-sm text-[var(--danger)]">{deleteError}</p>}
        <section className="management-memory-list scrollbar-thin" aria-label="记忆列表">
          {listLoading ? <MemoryListSkeleton /> : listError ? <ErrorState title="记忆暂时无法加载" description={listError} action={<Button variant="secondary" size="sm" onClick={reload}>重试</Button>} /> : memories.length === 0 ? hasActiveFilter ? <EmptyState icon={<WorkspaceIcon name="memory-record" size={24} />} title="没有匹配的记忆记录" description="当前搜索或筛选条件下未找到匹配项。" action={<Button variant="secondary" size="sm" onClick={handleClearFilters}>清除筛选</Button>} /> : <EmptyState icon={<WorkspaceIcon name="memory-record" size={24} />} title="这里暂时还没有记忆记录" description={view === "withdrawn" ? "撤回的记忆会保留在这里，不再用于检索。" : "小涟会在对话中提取，也可以手动添加。"} action={view === "active" ? <Button size="sm" onClick={() => { setShowAddModal(true); setAddError(null); }}>添加记忆</Button> : undefined} /> : memories.map(memory => <MemoryRow key={memory.id} memory={memory} selected={memory.id === selectedId} withdrawn={view === "withdrawn"} onSelect={() => { setSelectedId(memory.id); setDetailOpen(true); }} onEdit={() => handleOpenEdit(memory)} onDelete={() => handleDelete(memory)} />)}
        </section>
        <details className="memory-maintenance-panel management-memory-maintenance"><summary>记忆维护 · 当前列表 {memories.length} 条</summary>
          <div className="memory-maintenance-content"><div><p className="text-sm font-medium">重建记忆索引</p><p className="mt-1 text-xs text-[var(--text-secondary)]">为缺少索引的记忆补充检索数据。</p></div><Button variant="secondary" size="sm" onClick={reindexMemoriesAction} disabled={isReindexing}><RefreshCw className={`h-3.5 w-3.5 ${isReindexing ? "animate-spin" : ""}`} />{isReindexing ? "处理中…" : "重建索引"}</Button></div>
          {reindexResult && <p className={`mt-3 text-xs ${reindexResult.ok ? "text-[var(--text-secondary)]" : "text-[var(--danger)]"}`} role="status">{reindexResult.ok ? `本次处理 ${reindexResult.processed ?? 0} 条，成功 ${reindexResult.succeeded ?? 0} 条，失败 ${reindexResult.failed ?? 0} 条。` : `重建失败：${reindexResult.error ?? "请求失败"}`}</p>}
        </details>
      </div>
      <Modal open={detailOpen && Boolean(selectedMemory) && !editingMemory && !showAddModal} onClose={() => setDetailOpen(false)} title="记忆详情与来源" size="lg" footer={<Button variant="secondary" onClick={() => setDetailOpen(false)}>关闭</Button>}>
        {selectedMemory && <MemoryDetail memory={selectedMemory} onEdit={() => handleOpenEdit(selectedMemory)} onDelete={() => handleDelete(selectedMemory)} isDeleting={isDeleting} deleteError={deleteError} onChanged={() => { setDetailOpen(false); reload(); }} withdrawn={view === "withdrawn"} />}
      </Modal>

      <Modal
        open={showAddModal}
        onClose={() => { if (!isAdding) setShowAddModal(false); }}
        title="添加记忆"
        footer={
          <>
            <Button variant="secondary" onClick={() => setShowAddModal(false)} disabled={isAdding}>
              取消
            </Button>
            <Button onClick={handleAdd} loading={isAdding} loadingLabel="正在添加…" disabled={!newContent.trim()}>
              添加
            </Button>
          </>
        }
      >
        <div className="space-y-4">
          {addError && (
            <div className="rounded-[var(--radius-sm)] border border-[var(--danger)]/30 bg-[var(--danger)]/10 p-2.5 text-xs text-[var(--danger)]" role="alert">
              {addError}
            </div>
          )}
          <Textarea
            label="记忆内容"
            value={newContent}
            onChange={(event) => setNewContent(event.target.value)}
            placeholder="只填写希望长期保留的信息"
            autoFocus
          />
          <div>
            <label htmlFor="new-memory-category" className="mb-1.5 block text-sm font-medium">
              分类
            </label>
            <select
              id="new-memory-category"
              value={newCategory}
              onChange={(event) => setNewCategory(event.target.value)}
              className="w-full rounded-[var(--radius-md)] border border-[var(--border-soft)] bg-[var(--surface-solid)] px-3 py-2 text-sm text-[var(--text)] focus-ring-token focus:outline-none"
            >
              {MEMORY_CATEGORY_OPTIONS.filter((option) => option.value).map((option) => (
                <option key={option.value} value={option.value}>
                  {option.label}
                </option>
              ))}
            </select>
          </div>
        </div>
      </Modal>

      <Modal
        open={Boolean(editingMemory)}
        onClose={() => { if (!isEditing) setEditingMemory(null); }}
        title="编辑记忆"
        footer={
          <>
            <Button variant="secondary" onClick={() => setEditingMemory(null)} disabled={isEditing}>
              取消
            </Button>
            <Button onClick={handleEdit} loading={isEditing} loadingLabel="正在保存…" disabled={!editContent.trim()}>
              保存
            </Button>
          </>
        }
      >
        <div className="space-y-4">
          {editError && (
            <div className="rounded-[var(--radius-sm)] border border-[var(--danger)]/30 bg-[var(--danger)]/10 p-2.5 text-xs text-[var(--danger)]" role="alert">
              {editError}
            </div>
          )}
          <Textarea
            label="记忆内容"
            value={editContent}
            onChange={(event) => setEditContent(event.target.value)}
            autoFocus
          />
        </div>
      </Modal>
    </PageShell>
  );
}

function MemoryRow({
  memory, selected, withdrawn, onSelect, onEdit, onDelete,
}: {
  memory: MemoryRecord; selected: boolean; withdrawn: boolean; onSelect: () => void; onEdit: () => void; onDelete: () => void;
}) {
  const category = getMemoryCategoryPresentation(memory.category);
  const documentId = knowledgeDocumentForMemory(memory);
  return (
    <div
      role="button"
      tabIndex={0}
      aria-pressed={selected}
      onClick={onSelect}
      onKeyDown={(event) => {
        if (event.target === event.currentTarget && (event.key === "Enter" || event.key === " ")) {
          event.preventDefault();
          onSelect();
        }
      }}
      className={`memory-row management-memory-row ${selected ? "memory-row-selected" : ""}`}
      aria-label={`查看记忆 ${truncateMemoryContent(memory.content, 40)}`}
    >
      <div className="flex min-w-0 items-start justify-between gap-3">
        <span className="management-memory-icon" aria-hidden="true"><WorkspaceIcon name="memory-record" size={22} /></span>
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-2">
            <Badge tone={categoryBadgeTone(memory.category)}>{category.label}</Badge>
            <span className="memory-row-source">{getMemorySourceLabel(memory.source)}</span>
            {documentId && <Badge tone="default">文档片段 · 只读</Badge>}
          </div>
          <p className="memory-row-content">{truncateMemoryContent(memory.content)}</p>
        </div>
        <div className="memory-row-side">
          <Clock3 className="mt-1 h-3.5 w-3.5 shrink-0 text-[var(--text-faint)] memory-row-clock" />
          {!documentId && <div className="memory-row-actions">
            <button type="button" aria-label="编辑记忆" title="编辑记忆" disabled={withdrawn} onClick={(e) => { e.stopPropagation(); onEdit(); }} onKeyDown={(e) => e.stopPropagation()} className="memory-row-action-btn">
              <Pencil className="h-3.5 w-3.5" />
            </button>
            <button type="button" aria-label="删除记忆" title="删除记忆" onClick={(e) => { e.stopPropagation(); onDelete(); }} onKeyDown={(e) => e.stopPropagation()} className="memory-row-action-btn memory-row-action-danger">
              <Trash2 className="h-3.5 w-3.5" />
            </button>
          </div>}
        </div>
      </div>
      <div className="memory-row-meta">
        <span>{formatMemoryTime(memory.updated_at)}</span>
      </div>
    </div>
  );
}

function MemoryListSkeleton() {
  return (
    <div className="space-y-2 p-3" aria-label="正在加载记忆">
      {Array.from({ length: 5 }).map((_, i) => (
        <Skeleton key={i} className="h-20 w-full" />
      ))}
    </div>
  );
}
