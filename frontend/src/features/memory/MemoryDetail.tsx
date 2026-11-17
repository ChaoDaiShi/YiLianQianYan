import { Link } from "react-router-dom";
import { Pencil, Trash2 } from "lucide-react";
import type { MemoryRecord, MemoryVersion } from "../../api/memory";
import { Badge, Button } from "../../components/ui";
import { MemoryGovernancePanel } from "./MemoryGovernancePanel";
import { knowledgeDocumentForMemory } from "./knowledgeMemory";
import { isReviewedAiRelationship, readMemoryRelationship, relationshipKinds } from "./relationship";
import {
  categoryBadgeTone,
  formatMemoryTime,
  getMemoryCategoryPresentation,
  getMemorySourceLabel,
} from "./memoryCenterModel";

function proofloopSource(memory: MemoryRecord) {
  if (!memory.metadata) return null;
  try {
    const source = JSON.parse(memory.metadata);
    return source?.source_kind === "proofloop_acceptance" && typeof source.source_graph_id === "string" && typeof source.source_workspace_id === "string"
      ? { graph: source.source_graph_id, workspace: source.source_workspace_id }
      : null;
  } catch { return null; }
}

export default function MemoryDetail({
  memory, onEdit, onDelete, isDeleting, deleteError, onChanged, withdrawn, readOnly, graphHistory,
}: {
  memory: MemoryRecord; onEdit: () => void; onDelete: () => void; isDeleting?: boolean; deleteError?: string | null; onChanged: () => void; withdrawn?: boolean;
  readOnly?: boolean;
  graphHistory?: MemoryVersion[];
}) {
  const relationship = readMemoryRelationship(memory);
  const categoryLabel = relationship ? relationshipKinds[relationship.kind] : getMemoryCategoryPresentation(memory.category).label;
  const sourceLabel = relationship && isReviewedAiRelationship(memory) ? "AI 提议 · 用户审阅" : getMemorySourceLabel(memory.source);
  const documentId = knowledgeDocumentForMemory(memory);
  const taskSource = proofloopSource(memory);
  return (
    <div className="memory-detail-content">
      <div className="memory-detail-heading">
        <div>
          <div className="flex flex-wrap items-center gap-2">
            <Badge tone={categoryBadgeTone(memory.category)}>{categoryLabel}</Badge>
            {relationship && <Badge tone={relationship.certainty === "inferred" ? "warning" : "success"}>{relationship.certainty === "inferred" ? "推断 · 待确认" : "关系已确认"}</Badge>}
            <span className="text-xs text-[var(--text-secondary)]">{sourceLabel}</span>
            {documentId && <Badge tone="default">资料库引用</Badge>}
          </div>
        </div>
        {!documentId && !readOnly && <div className="flex items-center gap-1">
          <Button variant="ghost" size="sm" onClick={onEdit} aria-label="编辑记忆" disabled={withdrawn}>
            <Pencil className="h-4 w-4" />
            编辑
          </Button>
          <Button variant="ghost" size="sm" onClick={onDelete} aria-label="永久清除记忆" loading={isDeleting} loadingLabel="清除中…">
            <Trash2 className="h-4 w-4 text-[var(--danger)]" />
            永久清除
          </Button>
        </div>}
      </div>
      {documentId && <p className="rounded-lg bg-[var(--surface-muted)] p-3 text-xs leading-6 text-[var(--text-secondary)]">全文与切片在资料库中查看。 <Link className="text-[var(--accent-soft-fg)] underline" to={`/knowledge?document=${encodeURIComponent(documentId)}`}>打开源文档</Link></p>}

      {deleteError && (
        <div className="rounded-[var(--radius-sm)] border border-[var(--danger)]/30 bg-[var(--danger)]/10 p-2.5 text-xs text-[var(--danger)]" role="alert">
          {deleteError}
        </div>
      )}

      {!documentId && <div className="memory-detail-body">
        <p>{memory.content}</p>
      </div>}

      <dl className="memory-detail-fields">
        <div><dt>分类</dt><dd>{categoryLabel}</dd></div>
        <div><dt>来源</dt><dd>{sourceLabel}</dd></div>
        {memory.source_conversation_id && !memory.source_removed && <div><dt>来源对话</dt><dd><Link className="text-[var(--accent-soft-fg)] underline" to={`/chat/${encodeURIComponent(memory.source_conversation_id)}`}>打开来源对话</Link></dd></div>}
        {taskSource && !memory.source_removed && <div><dt>来源任务</dt><dd><Link className="text-[var(--accent-soft-fg)] underline" to={`/workspaces/${encodeURIComponent(taskSource.workspace)}?tab=tasks&graph=${encodeURIComponent(taskSource.graph)}`}>查看任务图与验收</Link></dd></div>}
        {memory.source_removed && <div><dt>{taskSource ? "来源任务" : "来源对话"}</dt><dd><Badge tone="warning">{taskSource ? "来源任务已移除" : "来源对话已移除"}</Badge></dd></div>}
        <div><dt>创建时间</dt><dd>{formatMemoryTime(memory.created_at)}</dd></div>
        <div><dt>更新时间</dt><dd>{formatMemoryTime(memory.updated_at)}</dd></div>
      </dl>

      {!documentId && <MemoryGovernancePanel memory={memory} onChanged={onChanged} readOnly={readOnly} graphHistory={graphHistory} />}

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
