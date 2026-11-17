import WorkspaceIcon from "../components/ui/WorkspaceIcon";
import { useCallback, useEffect, useMemo, useState } from "react";
import { useSearchParams } from "react-router-dom";
import { ChevronDown, Clock3, Pencil, Plus, RefreshCw, Search, Trash2, X } from "lucide-react";
import type { MemoryRecord } from "../api/client";
import { getMemory, listWithdrawnMemories } from "../api/memory";
import MemoryDetail from "../features/memory/MemoryDetail";
import MemoryNetwork from "../features/memory/MemoryNetwork";
import { useMemoryStore } from "../stores/memoryStore";
import PageShell from "../components/layout/PageShell";
import { Badge, Button, ConfirmDialog, EmptyState, ErrorState, Modal, Select, Skeleton, Tabs, Textarea } from "../components/ui";
import { knowledgeDocumentForMemory } from "../features/memory/knowledgeMemory";
import MemoryActivityView from "../features/memory/MemoryActivityView";
import MemoryCandidateReview from "../features/memory/MemoryCandidateReview";
import MemoryKnowledgeReferences from "../features/memory/MemoryKnowledgeReferences";
import RelationshipFields from "../features/memory/RelationshipFields";
import { canSaveMemoryRelationship, emptyRelationship, isReviewedAiRelationship, readMemoryRelationship, relationshipKinds, relationshipMemory } from "../features/memory/relationship";
import { viewForRelationship, type MemoryGraphMode } from "../features/memory/memoryGraphModel";
import type { MemoryGraphSnapshot } from "../features/memory/memoryGraphApi";
import {
  categoryBadgeTone,
  formatMemoryTime,
  getMemoryCategoryPresentation,
  getMemorySourceLabel,
  MEMORY_CATEGORY_OPTIONS,
  truncateMemoryContent,
} from "../features/memory/memoryCenterModel";
import "../features/memory/memoryPageToolbar.css";

const SOURCE_OPTIONS = [
  { value: "", label: "全部来源" },
  { value: "auto", label: "AI 提取" },
  { value: "manual", label: "手动" },
  { value: "document", label: "文档" },
] as const;

const MEMORY_LAYERS = [
  { id: "recent", label: "近期记忆", panelId: false },
  { id: "long", label: "长期记忆", panelId: false },
  { id: "candidates", label: "待审阅", panelId: false },
  { id: "knowledge", label: "知识引用", panelId: false },
  { id: "journal", label: "底层记忆", panelId: false },
] as const;
type MemoryLayer = typeof MEMORY_LAYERS[number]["id"];

export default function MemoryPage() {
  const [params, setParams] = useSearchParams();
  const linkedId = params.get("record");
  const [linked, setLinked] = useState<MemoryRecord | null>(null);
  const [linkedError, setLinkedError] = useState("");
  const [linkedRevision, setLinkedRevision] = useState(0);
  useEffect(() => {
    let active = true;
    setLinked(null);
    setLinkedError("");
    if (linkedId) void getMemory(linkedId).then(memory => { if (active) setLinked(memory); })
      .catch(() => { if (active) setLinkedError("这条引用记忆暂时无法打开。"); });
    return () => { active = false; };
  }, [linkedId, linkedRevision]);
  function closeLinked() {
    setParams(current => { const next = new URLSearchParams(current); next.delete("record"); return next; }, { replace: true });
  }
  const {
    memories: activeMemories,
    isLoading,
    error,
    loadMemories,
    addMemory,
    editMemory,
    removeMemory,
    isReindexing,
    reindexResult,
    reindexMemoriesAction,
  } = useMemoryStore();

  const [layer, setLayer] = useState<MemoryLayer>("long");
  const [view, setView] = useState<"active" | "withdrawn">("active");
  const [presentation, setPresentation] = useState<MemoryGraphMode | "list">("network");
  const [graphRefresh, setGraphRefresh] = useState(0);
  const [graphScope, setGraphScope] = useState("");
  const [graphRecords, setGraphRecords] = useState<MemoryRecord[]>([]);
  const [graphHistories, setGraphHistories] = useState<MemoryGraphSnapshot["histories"]>({});
  const [graphHeads, setGraphHeads] = useState<NonNullable<MemoryGraphSnapshot["current_version_ids"]>>({});
  const handleGraphSnapshot = useCallback((snapshot: MemoryGraphSnapshot) => {
    setGraphRecords(snapshot.memories);
    setGraphHistories(snapshot.histories);
    setGraphHeads(snapshot.current_version_ids ?? {});
  }, []);
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
  useEffect(() => { if (layer === "long" && view === "withdrawn" && presentation === "list") void loadWithdrawn(); }, [layer, view, presentation]);


  // Add modal state
  const [showAddModal, setShowAddModal] = useState(false);
  const [newContent, setNewContent] = useState("");
  const [newCategory, setNewCategory] = useState("fact");
  const [newRelationship, setNewRelationship] = useState(emptyRelationship);
  const [isAdding, setIsAdding] = useState(false);
  const [addError, setAddError] = useState<string | null>(null);

  // Edit modal state
  const [editingMemory, setEditingMemory] = useState<MemoryRecord | null>(null);
  const [editRevision, setEditRevision] = useState<number>();
  const [editContent, setEditContent] = useState("");
  const [editRelationship, setEditRelationship] = useState<typeof emptyRelationship | null>(null);
  const [isEditing, setIsEditing] = useState(false);
  const [editError, setEditError] = useState<string | null>(null);

  // Delete state
  const [isDeleting, setIsDeleting] = useState(false);
  const [deletingMemory, setDeletingMemory] = useState<MemoryRecord | null>(null);
  const [deleteError, setDeleteError] = useState<string | null>(null);

  useEffect(() => {
    if (layer === "long" && view === "active" && presentation === "list") void loadMemories({
      q: search.trim() || undefined,
      category: filterCategory || undefined,
      source: filterSource || undefined,
    });
  }, [layer, view, presentation, search, filterCategory, filterSource, loadMemories]);

  useEffect(() => {
    if (!selectedId || (!memories.some((memory) => memory.id === selectedId) && !graphRecords.some(memory => memory.id === selectedId))) {
      setSelectedId(memories[0]?.id ?? null);
    }
  }, [memories, graphRecords, selectedId]);

  const selectedMemory = useMemo(
    () => (presentation === "list" ? memories : graphRecords).find(memory => memory.id === selectedId) ?? null,
    [memories, graphRecords, presentation, selectedId]
  );
  const listLoading = view === "active" ? isLoading : withdrawnLoading;
  const listError = view === "active" ? error : withdrawnError;

  const hasActiveFilter = Boolean(search.trim() || filterCategory || filterSource);
  const canAdd = newCategory === "relationship" ? canSaveMemoryRelationship(newRelationship) : Boolean(newContent.trim());
  const canEdit = editRelationship ? canSaveMemoryRelationship(editRelationship) : Boolean(editContent.trim());

  const reload = () => {
    setGraphRefresh(value => value + 1);
    if (presentation !== "list") return;
    if (view === "withdrawn") void loadWithdrawn();
    else void loadMemories({
      q: search.trim() || undefined,
      category: filterCategory || undefined,
      source: filterSource || undefined,
    });
  };

  const handleClearFilters = () => {
    setSearch("");
    setFilterCategory("");
    setFilterSource("");
  };

  const handleAdd = async () => {
    if (!canAdd || isAdding) return;
    setIsAdding(true);
    setAddError(null);
    try {
      const created = await addMemory({
        ...(newCategory === "relationship" ? relationshipMemory(newRelationship) : { content: newContent.trim() }),
        category: newCategory === "relationship" ? "fact" : newCategory, source: "manual",
      });
      if (created) {
        setGraphRefresh(value => value + 1);
        setNewContent("");
        setNewCategory("fact");
        setNewRelationship(emptyRelationship);
        setLayer("long");
        setView("active");
        if (newCategory === "relationship") setPresentation(viewForRelationship(newRelationship.kind));
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
    setEditRevision(presentation === "list" ? undefined :
      graphHistories[memory.id]?.find(version => version.version_id === graphHeads[memory.id])?.revision);
    setEditContent(memory.content);
    setEditRelationship(readMemoryRelationship(memory));
    setEditError(null);
  };

  const handleEdit = async () => {
    if (!editingMemory || !canEdit || isEditing) return;
    setIsEditing(true);
    setEditError(null);
    try {
      const change = editRelationship ? relationshipMemory(editRelationship, editingMemory.metadata) : { content: editContent.trim() };
      const saved = await editMemory(editingMemory.id, { ...change, expected_revision: editRevision });
      if (saved) {
        setGraphRefresh(value => value + 1);
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
    setIsDeleting(true);
    setDeleteError(null);
    try {
      const targetId = memory.id;
      const currentIndex = (presentation === "list" ? memories : graphRecords).findIndex((m) => m.id === targetId);
      if (!await removeMemory(targetId)) {
        setDeleteError(useMemoryStore.getState().error || "删除失败，记录未能从记忆中心移除。");
      } else {
        setGraphRecords(records => records.filter(record => record.id !== targetId));
        setGraphRefresh(value => value + 1);
        const remaining = presentation === "list" ? useMemoryStore.getState().memories : graphRecords.filter(record => record.id !== targetId);
        if (remaining.length === 0) {
          setSelectedId(null);
        } else {
          const nextIndex = Math.min(Math.max(0, currentIndex), remaining.length - 1);
          setSelectedId(remaining[nextIndex].id);
        }
        setDeleteError(null);
        setDetailOpen(false);
        setDeletingMemory(null);
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
      variant="workspace"
      className="memory-page page-canvas ui-opt-memory management-memory-page"
      contentClassName="management-memory-content"
      actions={<Button size="sm" onClick={() => { setShowAddModal(true); setAddError(null); }}><Plus className="h-4 w-4" />添加记忆</Button>}
    >
      <div className="management-memory-workspace">
        <div className="memory-layer-navigation">
          <Tabs tabs={[...MEMORY_LAYERS]} activeId={layer} onChange={id => { setLayer(id as MemoryLayer); setDetailOpen(false); }} variant="segment" size="sm" aria-label="记忆层级" />
        </div>
        {layer === "recent" || layer === "journal" ? <MemoryActivityView key={layer} recent={layer === "recent"} onPromoted={() => { setLayer("long"); setView("active"); }} onPurged={id => {
          setWithdrawn(records => records.filter(record => record.id !== id));
          if (linkedId === id) closeLinked();
        }} />
          : layer === "knowledge" ? <MemoryKnowledgeReferences /> : layer === "candidates" ? <MemoryCandidateReview onViewRelationship={(kind, scope) => {
            handleClearFilters();
            setGraphScope(scope === "profile" ? "" : scope ?? graphScope);
            setView("active");
            setPresentation(viewForRelationship(kind));
            setLayer("long");
          }} /> : <>
        <div className="memory-page-toolbar">
          <div className="memory-search-field management-memory-search">
            <Search className="h-4 w-4 shrink-0 text-[var(--text-faint)]" />
            <input aria-label="搜索记忆" value={search} onChange={event => setSearch(event.target.value)} placeholder="搜索记忆…" />
            {search && <button type="button" onClick={() => setSearch("")} aria-label="清除记忆搜索" className="memory-clear-search"><X className="h-3.5 w-3.5" /></button>}
          </div>
          <div className="management-status-tabs" aria-label="记忆状态">
            <button type="button" aria-pressed={view === "active"} onClick={() => setView("active")}>当前可用</button>
            <button type="button" aria-pressed={view === "withdrawn"} onClick={() => setView("withdrawn")}>已撤回</button>
          </div>
          <div className="management-status-tabs" aria-label="记忆显示方式">
            <button type="button" aria-pressed={presentation !== "people" && presentation !== "list"} onClick={() => setPresentation("network")}>关系图谱</button>
            <button type="button" aria-pressed={presentation === "people"} onClick={() => setPresentation("people")}>人物关系</button>
            <button type="button" aria-pressed={presentation === "list"} onClick={() => setPresentation("list")}>列表</button>
          </div>
          <div className="memory-category-select"><Select size="sm" aria-label="记忆分类筛选" value={filterCategory} onChange={event => setFilterCategory(event.target.value)} options={[...MEMORY_CATEGORY_OPTIONS]} /></div>
          <Button size="sm" variant="ghost" iconOnly onClick={reload} aria-label="刷新记忆列表"><RefreshCw className="h-4 w-4" /></Button>
          <details
            className="memory-page-more"
            onBlur={event => {
              if (!event.currentTarget.contains(event.relatedTarget)) event.currentTarget.open = false;
            }}
            onKeyDown={event => {
              if (event.key !== "Escape" || !event.currentTarget.open) return;
              event.preventDefault();
              event.currentTarget.open = false;
              event.currentTarget.querySelector("summary")?.focus();
            }}
          >
            <summary aria-label={filterSource ? "更多，已按来源筛选" : undefined}>更多{filterSource && " · 来源"}<ChevronDown size={14} aria-hidden="true" /></summary>
            <div className="memory-page-more-panel">
              <label htmlFor="memory-page-source">来源</label>
              <Select size="sm" id="memory-page-source" aria-label="记忆来源筛选" value={filterSource} onChange={event => setFilterSource(event.target.value)} options={[...SOURCE_OPTIONS]} />
              <div className="memory-page-index-maintenance">
                <Button variant="secondary" size="sm" onClick={reindexMemoriesAction} loading={isReindexing} loadingLabel="正在重建…">重建索引</Button>
                <p>为缺少索引的记忆补充检索数据。</p>
              </div>
            </div>
          </details>
        </div>
        {reindexResult && <p className={`break-words text-xs ${reindexResult.ok ? "text-[var(--text-secondary)]" : "text-[var(--danger)]"}`} role="status">{reindexResult.ok ? `本次处理 ${reindexResult.processed ?? 0} 条，成功 ${reindexResult.succeeded ?? 0} 条，失败 ${reindexResult.failed ?? 0} 条。` : `重建失败：${reindexResult.error ?? "请求失败"}`}</p>}
        {deleteError && !detailOpen && <p role="alert" className="text-sm text-[var(--danger)]">{deleteError}</p>}
        {presentation !== "list" ? (
          <MemoryNetwork key={view} query={{ q: search.trim() || undefined, category: filterCategory || undefined, source: filterSource || undefined, withdrawn: view === "withdrawn" }}
            refreshKey={graphRefresh} mode={presentation} onModeChange={setPresentation} onVisible={handleGraphSnapshot}
            scope={graphScope} onScopeChange={scope => {
              setGraphScope(scope);
              setGraphRecords([]);
              setGraphHistories({});
              setGraphHeads({});
              setDetailOpen(false);
            }}
            onSelect={id => { setSelectedId(id); setDetailOpen(true); }} onReview={() => setLayer("candidates")}
            onAdd={() => setShowAddModal(true)} onClearFilters={handleClearFilters} />
        ) : <section className="management-memory-list scrollbar-thin" aria-label="记忆列表">
          {listLoading ? <MemoryListSkeleton /> : listError ? <ErrorState title="记忆暂时无法加载" description={listError} action={<Button variant="secondary" size="sm" onClick={reload}>重试</Button>} /> : memories.length === 0 ? hasActiveFilter ? <EmptyState icon={<WorkspaceIcon name="memory-record" size={24} />} title="没有匹配的记忆记录" description="当前搜索或筛选条件下未找到匹配项。" action={<Button variant="secondary" size="sm" onClick={handleClearFilters}>清除筛选</Button>} /> : <EmptyState icon={<WorkspaceIcon name="memory-record" size={24} />} title="这里暂时还没有记忆记录" description={view === "withdrawn" ? "撤回的记忆会保留在这里，不再用于检索。" : "添加一条记忆，或在对话中明确让小涟记住。"} /> : memories.map(memory => <MemoryRow key={memory.id} memory={memory} selected={memory.id === selectedId} withdrawn={view === "withdrawn"} onSelect={() => { setSelectedId(memory.id); setDetailOpen(true); }} onEdit={() => handleOpenEdit(memory)} onDelete={() => setDeletingMemory(memory)} />)}
        </section>}
        </>}
      </div>
      <Modal open={Boolean(linkedId)} onClose={closeLinked} title="引用记忆与来源" size="lg" footer={<Button variant="secondary" onClick={closeLinked}>关闭</Button>}>
        {linked?.id === linkedId && linked ? <MemoryDetail memory={linked} readOnly onEdit={() => {}} onDelete={() => {}} onChanged={() => {}} />
          : linkedError ? <ErrorState title="无法读取引用记忆" description={linkedError} onRetry={() => setLinkedRevision(value => value + 1)} />
            : <p role="status">正在读取引用记忆…</p>}
      </Modal>
      <Modal open={!linkedId && detailOpen && Boolean(selectedMemory) && !editingMemory && !showAddModal && !deletingMemory} onClose={() => setDetailOpen(false)} title="记忆详情与来源" size="lg" footer={<Button variant="secondary" onClick={() => setDetailOpen(false)}>关闭</Button>}>
        {selectedMemory && <MemoryDetail memory={selectedMemory} graphHistory={presentation !== "list" ? graphHistories[selectedMemory.id] ?? [] : undefined}
          onEdit={() => handleOpenEdit(selectedMemory)} onDelete={() => { setDeleteError(null); setDeletingMemory(selectedMemory); }} isDeleting={isDeleting} deleteError={deleteError} onChanged={() => { setDetailOpen(false); reload(); }} withdrawn={view === "withdrawn"} />}
      </Modal>
      <ConfirmDialog open={Boolean(deletingMemory)} title="永久清除记忆" tone="danger" confirmLabel="清除记忆"
        description={deleteError || "将清除这条记忆的正文、历史版本、检索索引和关系图引用。来源对话及独立消息记录会保留。底层日志只记录清除时间。已同步的副本需等待同步完成，本地页面无法确认云端清除结果。"} loading={isDeleting}
        onCancel={() => { setDeletingMemory(null); setDeleteError(null); }} onConfirm={async () => { if (deletingMemory) await handleDelete(deletingMemory); }} />

      <Modal
        open={showAddModal}
        onClose={() => { if (!isAdding) setShowAddModal(false); }}
        title="添加记忆"
        footer={
          <>
            <Button variant="secondary" onClick={() => setShowAddModal(false)} disabled={isAdding}>
              取消
            </Button>
            <Button onClick={handleAdd} loading={isAdding} loadingLabel="正在添加…" disabled={!canAdd}>
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
          <Select label="分类" value={newCategory} onChange={event => setNewCategory(event.target.value)} options={[
            ...MEMORY_CATEGORY_OPTIONS.filter(option => option.value), { value: "relationship", label: "关系记录" },
          ]} />
          {newCategory === "relationship" ? <RelationshipFields value={newRelationship} onChange={setNewRelationship} /> : <Textarea
            label="记忆内容"
            value={newContent}
            onChange={(event) => setNewContent(event.target.value)}
            placeholder="只填写希望长期保留的信息"
            autoFocus
          />}
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
            <Button onClick={handleEdit} loading={isEditing} loadingLabel="正在保存…" disabled={!canEdit}>
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
          {editRelationship ? <RelationshipFields value={editRelationship} onChange={setEditRelationship} /> : <Textarea
            label="记忆内容"
            value={editContent}
            onChange={(event) => setEditContent(event.target.value)}
            autoFocus
          />}
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
  const relationship = readMemoryRelationship(memory);
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
            <Badge tone={categoryBadgeTone(memory.category)}>{relationship ? relationshipKinds[relationship.kind] : category.label}</Badge>
            <span className="memory-row-source">{relationship && isReviewedAiRelationship(memory) ? "AI 提议 · 用户审阅" : getMemorySourceLabel(memory.source)}</span>
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
            <button type="button" aria-label="永久清除记忆" title="永久清除记忆" onClick={(e) => { e.stopPropagation(); onDelete(); }} onKeyDown={(e) => e.stopPropagation()} className="memory-row-action-btn memory-row-action-danger">
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
