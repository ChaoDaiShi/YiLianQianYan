import { useEffect, useMemo, useState } from "react";
import { Brain, Clock3, Pencil, Plus, RefreshCw, Search, Trash2, X } from "lucide-react";
import type { MemoryRecord } from "../api/client";
import { useMemoryStore } from "../stores/memoryStore";
import { Badge, Button, EmptyState, ErrorState, Modal, PageHeader, Skeleton, Textarea } from "../components/ui";
import {
  getMemoryCategoryPresentation,
  getMemorySourceLabel,
  memoryCategoryCounts,
  MEMORY_CATEGORY_OPTIONS,
  truncateMemoryContent,
} from "../features/memory/memoryCenterModel";

const SOURCE_OPTIONS = [
  { value: "", label: "全部来源" },
  { value: "auto", label: "AI 提取" },
  { value: "manual", label: "手动" },
  { value: "document", label: "文档" },
] as const;

type BadgeTone = "default" | "success" | "warning" | "danger" | "info" | "accent";

function categoryBadgeTone(category: string): BadgeTone {
  const tone = getMemoryCategoryPresentation(category).tone;
  return tone === "info" ? "info" : tone === "accent" || tone === "purple" ? "accent" : tone === "warning" ? "warning" : "default";
}

function formatMemoryTime(timestamp: number): string {
  return new Date(timestamp).toLocaleString("zh-CN", { month: "short", day: "numeric", hour: "2-digit", minute: "2-digit" });
}

export default function MemoryPage() {
  const {
    memories,
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

  const [search, setSearch] = useState("");
  const [filterCategory, setFilterCategory] = useState("");
  const [filterSource, setFilterSource] = useState("");
  const [selectedId, setSelectedId] = useState<string | null>(null);

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
  const categoryCounts = memoryCategoryCounts(stats);

  const hasActiveFilter = Boolean(search.trim() || filterCategory || filterSource);

  const reload = () => {
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
    if (isDeleting) return;
    if (!window.confirm("删除这条记忆？\n\n删除后，这条记录将从记忆中心移除。")) return;
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
      }
    } catch (err) {
      setDeleteError(err instanceof Error ? err.message : String(err));
    } finally {
      setIsDeleting(false);
    }
  };

  return (
    <div className="memory-page page-canvas ui-opt-memory">
      <PageHeader
        title="记忆中心"
        description="小昔涟长期保留的重要信息"
        actions={
          <Button size="sm" onClick={() => { setShowAddModal(true); setAddError(null); }}>
            <Plus className="h-4 w-4" />
            添加记忆
          </Button>
        }
      />

      <div className="memory-page-body scrollbar-thin">
        <MemoryOverview counts={categoryCounts} total={stats?.total ?? memories.length} onSelect={setFilterCategory} />

        <div className="memory-toolbar">
          <label className="memory-search-field">
            <Search className="h-4 w-4 shrink-0 text-[var(--text-faint)]" />
            <span className="sr-only">搜索记忆</span>
            <input
              aria-label="搜索记忆"
              value={search}
              onChange={(event) => setSearch(event.target.value)}
              placeholder="搜索记忆…"
            />
            {search && (
              <button type="button" onClick={() => setSearch("")} aria-label="清除记忆搜索" className="memory-clear-search">
                <X className="h-3.5 w-3.5" />
              </button>
            )}
          </label>
          <div className="memory-filter-group" aria-label="记忆分类筛选">
            {MEMORY_CATEGORY_OPTIONS.map((option) => (
              <button
                type="button"
                key={option.value || "all"}
                aria-pressed={filterCategory === option.value}
                onClick={() => setFilterCategory(option.value)}
                className="memory-filter-button"
              >
                {option.label}
              </button>
            ))}
          </div>
          <label className="sr-only" htmlFor="memory-source-filter">记忆来源筛选</label>
          <select
            id="memory-source-filter"
            value={filterSource}
            onChange={(event) => setFilterSource(event.target.value)}
            className="memory-source-filter"
          >
            {SOURCE_OPTIONS.map((option) => (
              <option key={option.value || "all"} value={option.value}>
                {option.label}
              </option>
            ))}
          </select>
        </div>

        <div className="memory-lower-grid">
          <section className="memory-list-panel" aria-label="记忆列表">
            <div className="memory-panel-header">
              <div>
                <h2>已保存的记忆</h2>
                <p>{memories.length} 条当前记录</p>
              </div>
              <button type="button" onClick={reload} aria-label="刷新记忆列表" className="memory-icon-button">
                <RefreshCw className="h-4 w-4" />
              </button>
            </div>
            <div className="memory-list-scroll scrollbar-thin">
              {isLoading ? (
                <MemoryListSkeleton />
              ) : error ? (
                <ErrorState
                  title="记忆暂时无法加载"
                  description={error}
                  action={<Button variant="secondary" size="sm" onClick={reload}>重试</Button>}
                  className="m-3"
                />
              ) : memories.length === 0 ? (
                hasActiveFilter ? (
                  <EmptyState
                    icon={<Brain className="h-6 w-6" />}
                    title="没有匹配的记忆记录"
                    description="当前搜索或筛选条件下未找到匹配项，可以清除筛选重试。"
                    action={
                      <Button variant="secondary" size="sm" onClick={handleClearFilters}>
                        清除筛选
                      </Button>
                    }
                    className="min-h-[280px]"
                  />
                ) : (
                  <EmptyState
                    icon={<Brain className="h-6 w-6" />}
                    title="这里暂时还没有记忆记录"
                    description="小昔涟会在对话中自动提取，也可以手动添加一条真实记忆。"
                    action={
                      <Button size="sm" onClick={() => { setShowAddModal(true); setAddError(null); }}>
                        <Plus className="h-4 w-4" />
                        添加记忆
                      </Button>
                    }
                    className="min-h-[280px]"
                  />
                )
              ) : (
                <div className="space-y-2 p-3">
                  {memories.map((memory) => (
                    <MemoryRow
                      key={memory.id}
                      memory={memory}
                      selected={memory.id === selectedId}
                      onSelect={() => setSelectedId(memory.id)}
                      onEdit={() => handleOpenEdit(memory)}
                      onDelete={() => handleDelete(memory)}
                    />
                  ))}
                </div>
              )}
            </div>
          </section>

          <section className="memory-detail-panel" aria-label="记忆详情">
            {selectedMemory ? (
              <MemoryDetail
                memory={selectedMemory}
                onEdit={() => handleOpenEdit(selectedMemory)}
                onDelete={() => handleDelete(selectedMemory)}
                isDeleting={isDeleting}
                deleteError={deleteError}
              />
            ) : (
              <EmptyState
                icon={<Brain className="h-6 w-6" />}
                title="选择一条记忆"
                description="完整内容和真实字段会显示在这里。"
                className="h-full min-h-[300px]"
              />
            )}
          </section>
        </div>

        <details className="memory-maintenance-panel">
          <summary>记忆维护</summary>
          <div className="memory-maintenance-content">
            <div>
              <p className="text-sm font-medium">重建记忆索引</p>
              <p className="mt-1 text-xs text-[var(--text-secondary)]">为缺少索引的记忆补充检索数据。</p>
            </div>
            <Button variant="secondary" size="sm" onClick={reindexMemoriesAction} disabled={isReindexing}>
              <RefreshCw className={`h-3.5 w-3.5 ${isReindexing ? "animate-spin" : ""}`} />
              {isReindexing ? "处理中…" : "重建索引"}
            </Button>
          </div>
          {reindexResult && (
            <p
              className={`mt-3 text-xs ${reindexResult.ok ? "text-[var(--text-secondary)]" : "text-[var(--danger)]"}`}
              role="status"
            >
              {reindexResult.ok
                ? `本次处理 ${reindexResult.processed ?? 0} 条，成功 ${reindexResult.succeeded ?? 0} 条，失败 ${reindexResult.failed ?? 0} 条。`
                : `重建失败：${reindexResult.error ?? "请求失败"}`}
            </p>
          )}
        </details>
      </div>

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
    </div>
  );
}

function MemoryOverview({ counts, total, onSelect }: { counts: Record<string, number>; total: number; onSelect: (category: string) => void }) {
  return (
    <section className="memory-overview" aria-label="记忆分类概览">
      <div className="memory-overview-copy">
        <span className="memory-overview-kicker">MEMORY OVERVIEW</span>
        <h2>小昔涟记住了什么</h2>
        <p>按真实分类浏览长期记忆，不表示记忆之间存在关系。</p>
      </div>
      <div className="memory-bubbles">
        <button type="button" aria-label="查看全部记忆" onClick={() => onSelect("")} className="memory-bubble-center">
          <Brain className="h-4 w-4" />
          <strong>{total}</strong>
          <span>条记忆</span>
        </button>
        <div className="memory-bubble-grid">
          {MEMORY_CATEGORY_OPTIONS.filter((o) => o.value).map((opt, i) => (
            <button type="button" key={opt.value} aria-label={`按${opt.label}筛选`} onClick={() => onSelect(opt.value)} className={`memory-bubble memory-bubble-${i}`}>
              <span>{opt.label}</span>
              <strong>{counts[opt.value] ?? 0}</strong>
            </button>
          ))}
        </div>
      </div>
    </section>
  );
}

function MemoryRow({
  memory, selected, onSelect, onEdit, onDelete,
}: {
  memory: MemoryRecord; selected: boolean; onSelect: () => void; onEdit: () => void; onDelete: () => void;
}) {
  const category = getMemoryCategoryPresentation(memory.category);
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
      className={`memory-row ${selected ? "memory-row-selected" : ""}`}
    >
      <div className="flex min-w-0 items-start justify-between gap-3">
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2">
            <Badge tone={categoryBadgeTone(memory.category)}>{category.label}</Badge>
            <span className="memory-row-source">{getMemorySourceLabel(memory.source)}</span>
          </div>
          <p className="memory-row-content">{truncateMemoryContent(memory.content)}</p>
        </div>
        <div className="memory-row-side">
          <Clock3 className="mt-1 h-3.5 w-3.5 shrink-0 text-[var(--text-faint)] memory-row-clock" />
          <div className="memory-row-actions">
            <button type="button" aria-label="编辑记忆" title="编辑记忆" onClick={(e) => { e.stopPropagation(); onEdit(); }} onKeyDown={(e) => e.stopPropagation()} className="memory-row-action-btn">
              <Pencil className="h-3.5 w-3.5" />
            </button>
            <button type="button" aria-label="删除记忆" title="删除记忆" onClick={(e) => { e.stopPropagation(); onDelete(); }} onKeyDown={(e) => e.stopPropagation()} className="memory-row-action-btn memory-row-action-danger">
              <Trash2 className="h-3.5 w-3.5" />
            </button>
          </div>
        </div>
      </div>
      <div className="memory-row-meta">
        <span>{formatMemoryTime(memory.updated_at)}</span>
      </div>
    </div>
  );
}

function MemoryDetail({
  memory, onEdit, onDelete, isDeleting, deleteError,
}: {
  memory: MemoryRecord; onEdit: () => void; onDelete: () => void; isDeleting?: boolean; deleteError?: string | null;
}) {
  const category = getMemoryCategoryPresentation(memory.category);
  return (
    <div className="memory-detail-content">
      <div className="memory-detail-heading">
        <div>
          <div className="flex items-center gap-2">
            <Badge tone={categoryBadgeTone(memory.category)}>{category.label}</Badge>
            <span className="text-xs text-[var(--text-secondary)]">{getMemorySourceLabel(memory.source)}</span>
          </div>
          <h2>记忆详情</h2>
        </div>
        <div className="flex items-center gap-1">
          <Button variant="ghost" size="sm" onClick={onEdit} aria-label="编辑记忆">
            <Pencil className="h-4 w-4" />
            编辑
          </Button>
          <Button variant="ghost" size="sm" onClick={onDelete} aria-label="删除记忆" loading={isDeleting} loadingLabel="删除中…">
            <Trash2 className="h-4 w-4 text-[var(--danger)]" />
            删除
          </Button>
        </div>
      </div>

      {deleteError && (
        <div className="rounded-[var(--radius-sm)] border border-[var(--danger)]/30 bg-[var(--danger)]/10 p-2.5 text-xs text-[var(--danger)]" role="alert">
          {deleteError}
        </div>
      )}

      <div className="memory-detail-body">
        <p>{memory.content}</p>
      </div>

      <dl className="memory-detail-fields">
        <div><dt>分类</dt><dd>{category.label}</dd></div>
        <div><dt>来源</dt><dd>{getMemorySourceLabel(memory.source)}</dd></div>
        <div><dt>创建时间</dt><dd>{formatMemoryTime(memory.created_at)}</dd></div>
        <div><dt>更新时间</dt><dd>{formatMemoryTime(memory.updated_at)}</dd></div>
      </dl>

      <details className="memory-technical-details">
        <summary>查看技术详情</summary>
        <dl>
          <div><dt>ID</dt><dd className="font-mono text-xs">{memory.id}</dd></div>
          {memory.source_conversation_id && (
            <div><dt>来源对话 ID</dt><dd className="font-mono text-xs">{memory.source_conversation_id}</dd></div>
          )}
        </dl>
      </details>
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
