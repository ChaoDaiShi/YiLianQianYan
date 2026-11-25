import { useState, type FormEvent } from "react";
import { Button, Input, Modal } from "../../components/ui";
import { retrieveMemories } from "../../api/memory";

interface Props {
  onClose: () => void;
  onSelect: (id: string) => void;
}

export function MemorySearchDialog({ onClose, onSelect }: Props) {
  const [query, setQuery] = useState("");
  const [response, setResponse] = useState<Awaited<ReturnType<typeof retrieveMemories>>>();
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");

  const search = async (event: FormEvent) => {
    event.preventDefault();
    if (!query.trim() || loading) return;
    setLoading(true);
    setError("");
    setResponse(undefined);
    try {
      setResponse(await retrieveMemories(query.trim()));
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setLoading(false);
    }
  };

  return <Modal open title="语义检索" size="lg" onClose={onClose}>
    <form className="flex flex-wrap items-end gap-2" onSubmit={search}>
      <div className="min-w-0 grow"><Input label="查找个人记忆" placeholder="描述你想找的内容" value={query} onChange={event => setQuery(event.target.value)} autoFocus /></div>
      <Button type="submit" disabled={loading || !query.trim()}>{loading ? "检索中…" : "检索"}</Button>
    </form>
    {error && <p role="alert" className="mt-3 text-sm text-[var(--danger)]">{error}</p>}
    {response && <div className="mt-3 space-y-2">
      <p role="status" className="text-sm text-[var(--text-muted)]">
        {response.mode === "hybrid" ? "混合检索" : response.mode === "lexical_fallback" ? "向量服务暂不可用，已使用关键词检索" : "关键词检索"} · {response.count} 条
      </p>
      {response.memories.map(memory =>
        <button type="button" key={memory.id} onClick={() => onSelect(memory.id)}
          className="block w-full break-words rounded-md border border-[var(--border-soft)] p-2 text-left line-clamp-3">
          {memory.content}
        </button>)}
    </div>}
  </Modal>;
}
