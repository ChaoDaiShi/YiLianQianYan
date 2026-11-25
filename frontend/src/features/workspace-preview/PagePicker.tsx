import { useEffect, useState } from "react";
import { Button, Input } from "../../components/ui";
import { listPages, type WorkspacePage } from "../../api/workspacePages";

export default function PagePicker({ workspace, selected, excluded, onSelect }: {
  workspace: string; selected?: string; excluded?: string; onSelect: (page: WorkspacePage) => void;
}) {
  const [query, setQuery] = useState("");
  const [pages, setPages] = useState<WorkspacePage[]>([]);
  const [offset, setOffset] = useState<number | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [retry, setRetry] = useState(0);
  useEffect(() => {
    let current = true;
    setLoading(true); setError(""); setPages([]);
    const delay = window.setTimeout(() => {
      void listPages(workspace, { q: query, view: query.trim() ? undefined : "all" }).then(result => {
        if (!current) return;
        setLoading(false);
        if (result.ok) { setPages(result.data.pages); setOffset(result.data.next_offset); }
        else setError(result.error);
      });
    }, 180);
    return () => { current = false; window.clearTimeout(delay); };
  }, [workspace, query, retry]);

  const next = async () => {
    if (offset === null) return;
    setLoading(true); setError("");
    const result = await listPages(workspace, { q: query, view: query.trim() ? undefined : "all", offset });
    setLoading(false);
    if (result.ok) { setPages(current => [...current, ...result.data.pages]); setOffset(result.data.next_offset); }
    else setError(result.error);
  };

  const choices = pages.filter(page => page.id !== excluded);
  return <div className="workspace-page-picker">
    <Input label="查找页面" value={query} onChange={event => setQuery(event.target.value)} />
    {loading && <p role="status">正在读取页面…</p>}
    {error && <p role="alert">{error} <Button size="sm" variant="ghost" onClick={() => setRetry(value => value + 1)}>重试</Button></p>}
    {!loading && !error && !choices.length && <p>没有找到可选页面。</p>}
    <div className="workspace-page-picker-list">
      {choices.map(page => <button key={page.id} type="button" onClick={() => onSelect(page)} aria-pressed={selected === page.id}>
        {page.icon && <span aria-hidden="true">{page.icon}</span>}{page.title}
      </button>)}
    </div>
    {offset !== null && <Button size="sm" variant="secondary" disabled={loading} onClick={() => void next()}>加载更多页面</Button>}
  </div>;
}
