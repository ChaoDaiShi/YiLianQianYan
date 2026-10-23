import { Link } from "react-router-dom";
import { Pencil, Trash2 } from "lucide-react";
import type { MemoryRecord } from "../../api/memory";
import { Badge, Button } from "../../components/ui";
import { MemoryGovernancePanel } from "./MemoryGovernancePanel";
import { knowledgeDocumentForMemory } from "./knowledgeMemory";
import {
  categoryBadgeTone,
  formatMemoryTime,
  getMemoryCategoryPresentation,
  getMemorySourceLabel,
} from "./memoryCenterModel";

export default function MemoryDetail({
  memory, onEdit, onDelete, isDeleting, deleteError, onChanged, withdrawn,
}: {
  memory: MemoryRecord; onEdit: () => void; onDelete: () => void; isDeleting?: boolean; deleteError?: string | null; onChanged: () => void; withdrawn?: boolean;
}) {
  const category = getMemoryCategoryPresentation(memory.category);
  const documentId = knowledgeDocumentForMemory(memory);
  return (
    <div className="memory-detail-content">
      <div className="memory-detail-heading">
        <div>
          <div className="flex flex-wrap items-center gap-2">
            <Badge tone={categoryBadgeTone(memory.category)}>{category.label}</Badge>
            <span className="text-xs text-[var(--text-secondary)]">{getMemorySourceLabel(memory.source)}</span>
            {documentId && <Badge tone="default">文档片段 · 只读</Badge>}
          </div>
          <h2>记忆详情</h2>
        </div>
        {!documentId && <div className="flex items-center gap-1">
          <Button variant="ghost" size="sm" onClick={onEdit} aria-label="编辑记忆" disabled={withdrawn}>
            <Pencil className="h-4 w-4" />
            编辑
          </Button>
          <Button variant="ghost" size="sm" onClick={onDelete} aria-label="删除记忆" loading={isDeleting} loadingLabel="删除中…">
            <Trash2 className="h-4 w-4 text-[var(--danger)]" />
            删除
          </Button>
        </div>}
      </div>
      {documentId && <p className="rounded-lg bg-[var(--surface-muted)] p-3 text-xs leading-6 text-[var(--text-secondary)]">该片段由知识文档管理，请在知识库中重新导入或移除源文档。 <Link className="text-[var(--accent-primary)] underline" to={`/knowledge?document=${encodeURIComponent(documentId)}`}>管理源文档</Link></p>}

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

      {!documentId && <MemoryGovernancePanel memory={memory} onChanged={onChanged} />}

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
