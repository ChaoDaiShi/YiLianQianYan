import { useEffect, useMemo, useState } from "react";
import { getKnowledgeEntry, type KnowledgeEntry, type KnowledgeEntrySummary } from "../../api/knowledge";
import { Button, ErrorState } from "../../components/ui";
import { MemoryGraphCanvas } from "./MemoryGraphCanvas";
import { buildKnowledgeGraph } from "./memoryGraphModel";

export default function KnowledgeReferenceGraph({ entries, onEntry, revision }: {
  entries: KnowledgeEntrySummary[];
  onEntry: (id: string) => void;
  revision: number;
}) {
  const [focusId, setFocusId] = useState<string | null>(null);
  const [entryId, setEntryId] = useState<string | null>(null);
  const [result, setResult] = useState<{ id: string; entry?: KnowledgeEntry; error?: string }>();
  const [retry, setRetry] = useState(0);
  const visibleEntryId = entries.some(entry => entry.id === entryId) ? entryId : null;
  const current = result?.id === visibleEntryId ? result : undefined;

  useEffect(() => {
    if (!visibleEntryId) return;
    let active = true;
    setResult(undefined);
    void getKnowledgeEntry(visibleEntryId).then(entry => {
      if (active) setResult({ id: visibleEntryId, entry });
    }).catch(error => {
      if (active) setResult({ id: visibleEntryId, error: error instanceof Error ? error.message : String(error) });
    });
    return () => { active = false; };
  }, [visibleEntryId, revision, retry]);

  const graph = useMemo(() => buildKnowledgeGraph(entries, current?.entry), [entries, current?.entry]);

  return <div className="memory-network-workspace">
    {visibleEntryId && !current && <p className="memory-layer-hint" role="status">正在核对原文引用…</p>}
    {current?.error && <ErrorState title="无法读取引用" description={current.error} onRetry={() => setRetry(value => value + 1)} />}
    {current?.entry && <div className="memory-network-toolbar">
      <Button variant="ghost" size="sm" onClick={() => setRetry(value => value + 1)}>重新核对来源</Button>
    </div>}
    <MemoryGraphCanvas graph={graph} focusId={focusId} onRecord={onEntry} label="知识引用关系图" emptyMessage="没有匹配的知识条目"
      onFocus={node => { setFocusId(node?.id ?? null); if (!node || node.kind === "knowledge") setEntryId(node?.recordId ?? null); }} />
  </div>;
}
