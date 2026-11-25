import { useEffect, useState } from "react";
import { ChevronDown, ChevronRight, FileText, Plus } from "lucide-react";
import { Button } from "../../components/ui";
import { listPages, type WorkspacePage } from "../../api/workspacePages";

export type PageTreeView = "tree" | "favorites" | "recent" | "trash";

export default function PageTree({ workspace, parent, query, view, epoch, selected, writable, onSelect, onCreateChild, onMove, depth = 0 }: {
  workspace: string; parent?: string; query: string; view: PageTreeView; epoch: number; selected?: string; writable: boolean;
  onSelect: (page: WorkspacePage) => void;
  onCreateChild: (page: WorkspacePage) => void;
  onMove: (id: string, parent: string | null, before?: string) => void;
  depth?: number;
}) {
  const [pages, setPages] = useState<WorkspacePage[]>([]);
  const [expanded, setExpanded] = useState<ReadonlySet<string>>(() => new Set());
  const [offset, setOffset] = useState<number | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [retry, setRetry] = useState(0);
  const [dragTarget, setDragTarget] = useState<{ id: string; before: boolean } | null>(null);
  const flat = view !== "tree" || Boolean(query.trim());
  const viewFilter = view === "tree" ? undefined : view;
  useEffect(() => {
    let current = true;
    setError(""); setLoading(true); setOffset(null);
    const timer = window.setTimeout(() => {
      void listPages(workspace, { parent, q: query, view: viewFilter }).then(result => {
        if (!current) return;
        setLoading(false);
        if (result.ok) { setPages(result.data.pages); setOffset(result.data.next_offset); }
        else { setPages([]); setError(result.error); }
      });
    }, query.trim() ? 200 : 0);
    return () => { current = false; window.clearTimeout(timer); };
  }, [epoch, parent, query, retry, viewFilter, workspace]);

  const next = async () => {
    if (offset === null) return;
    setLoading(true); setError("");
    const result = await listPages(workspace, { parent, q: query, view: viewFilter, offset });
    setLoading(false);
    if (result.ok) { setPages(current => [...current, ...result.data.pages]); setOffset(result.data.next_offset); }
    else setError(result.error);
  };
  const toggle = (id: string) => setExpanded(current => {
    const next = new Set(current);
    if (next.has(id)) next.delete(id); else next.add(id);
    return next;
  });

  return <div className="workspace-page-branch">
    {loading && !pages.length && <p role="status" className="workspace-page-note">正在读取页面…</p>}
    {error && <p role="alert" className="workspace-page-error">{error} <Button size="sm" variant="ghost" onClick={() => setRetry(value => value + 1)}>重试</Button></p>}
    {!loading && !error && !pages.length && <p className="workspace-page-note">{query ? "没有找到匹配的页面。" : parent ? "没有子页面。" : view === "trash" ? "回收站为空。" : "暂无页面。"}</p>}
    {pages.map(page => {
      const isExpanded = expanded.has(page.id);
      return <div key={page.id}>
        <div className="workspace-page-tree-row" data-selected={selected === page.id || undefined}
          data-drop={dragTarget?.id === page.id ? dragTarget.before ? "before" : "child" : undefined}
          style={{ paddingInlineStart: Math.min(depth, 8) * 12 }}
          draggable={writable && !flat}
          onDragStart={event => { event.dataTransfer.setData("application/yilian-page", page.id); event.dataTransfer.effectAllowed = "move"; }}
          onDragOver={event => {
            if (!writable || flat || !event.dataTransfer.types.includes("application/yilian-page")) return;
            event.preventDefault();
            setDragTarget({ id: page.id, before: event.clientY - event.currentTarget.getBoundingClientRect().top < 10 });
          }}
          onDragLeave={() => setDragTarget(null)}
          onDrop={event => {
            event.preventDefault();
            const id = event.dataTransfer.getData("application/yilian-page");
            const before = dragTarget?.before;
            setDragTarget(null);
            if (id && id !== page.id) {
              if (!before) setExpanded(current => new Set([...current, page.id]));
              onMove(id, before ? page.parent_page_id : page.id, before ? page.id : undefined);
            }
          }}>
          {!flat && page.has_children
            ? <button type="button" className="workspace-page-disclosure" aria-label={(isExpanded ? "收起 " : "展开 ") + page.title} aria-expanded={isExpanded}
              onClick={() => toggle(page.id)}>{isExpanded ? <ChevronDown size={15} /> : <ChevronRight size={15} />}</button>
            : <span className="workspace-page-disclosure" />}
          <button className="workspace-page-tree-title" type="button" onClick={() => onSelect(page)} aria-current={selected === page.id ? "page" : undefined}>
            {page.icon ? <span aria-hidden="true">{page.icon}</span> : <FileText size={14} aria-hidden="true" />}<span>{page.title}</span>
          </button>
          {writable && !flat && <button className="workspace-page-child" aria-label={"新建子页面 " + page.title} type="button" onClick={() => {
            setExpanded(current => new Set([...current, page.id]));
            onCreateChild(page);
          }}><Plus size={15} /></button>}
        </div>
        {!flat && isExpanded && page.has_children && <PageTree workspace={workspace} parent={page.id} query="" view="tree" epoch={epoch}
          selected={selected} writable={writable} onSelect={onSelect} onCreateChild={onCreateChild} onMove={onMove} depth={depth + 1} />}
      </div>;
    })}
    {offset !== null && <Button size="sm" variant="ghost" disabled={loading} onClick={() => void next()}>加载更多</Button>}
  </div>;
}
