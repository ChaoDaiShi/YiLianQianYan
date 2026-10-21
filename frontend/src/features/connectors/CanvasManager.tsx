import { useEffect, useState } from "react";
import { deleteConnectorCanvas, listConnectorCanvases, renameConnectorCanvas, type ConnectorCanvas } from "../../api/connectors";
import { Button, ErrorState, Input, Modal } from "../../components/ui";

type CanvasTarget = Pick<ConnectorCanvas, "id" | "name">;
export default function CanvasManager({ open, onClose, activeId, onDeleted, onUpdated, initialDeleteCanvas }: { open: boolean; onClose: () => void; activeId: string | null; onDeleted: (id: string) => Promise<void>; onUpdated: () => Promise<void>; initialDeleteCanvas?: CanvasTarget | null }) {
  const [canvases, setCanvases] = useState<ConnectorCanvas[]>([]);
  const [selected, setSelected] = useState<string[]>([]);
  const [confirm, setConfirm] = useState<CanvasTarget[]>([]);
  const [renaming, setRenaming] = useState<{ id: string; name: string } | null>(null);
  const [error, setError] = useState("");
  const [results, setResults] = useState<string[]>([]);
  const [busy, setBusy] = useState(false);

  async function reload() {
    const result = await listConnectorCanvases();
    setCanvases(result.canvases);
    setSelected(current => current.filter(id => result.canvases.some(canvas => canvas.id === id)));
  }
  useEffect(() => {
    if (!open) return;
    setError(""); setResults([]); setConfirm(initialDeleteCanvas ? [initialDeleteCanvas] : []); setRenaming(null); setSelected([]);
    if (!initialDeleteCanvas) void reload().catch(cause => setError(String(cause)));
  }, [open, initialDeleteCanvas?.id, initialDeleteCanvas?.name]);

  function back() { if (initialDeleteCanvas) { onClose(); return; } setConfirm([]); setRenaming(null); setError(""); }
  async function remove() {
    setBusy(true); setError("");
    const messages: string[] = [];
    const errors: string[] = [];
    const retained: CanvasTarget[] = [];
    for (const target of confirm) {
      try { await deleteConnectorCanvas(target.id); }
      catch (cause) { errors.push(`${target.name} 删除失败：${String(cause)}`); retained.push(target); continue; }
      // The service has already deleted the layout; a refresh error must not report it as retained.
      setCanvases(current => current.filter(canvas => canvas.id !== target.id));
      setSelected(current => current.filter(id => id !== target.id));
      messages.push(`已删除 ${target.name}`);
      try { await onDeleted(target.id); }
      catch (cause) { errors.push(`${target.name} 已删除，画布列表刷新失败：${String(cause)}`); }
    }
    setConfirm(initialDeleteCanvas ? retained : []); setResults(messages);
    setError(errors.join("；")); setBusy(false);
    if (initialDeleteCanvas && errors.length === 0) onClose();
  }
  async function rename() {
    if (!renaming) return;
    setBusy(true); setError("");
    try { await renameConnectorCanvas(renaming.id, renaming.name); setRenaming(null); await reload(); await onUpdated(); }
    catch (cause) { setError(String(cause)); }
    finally { setBusy(false); }
  }
  const confirming = confirm.length > 0;
  const title = confirming ? "删除所选画布" : renaming ? "重命名画布" : "管理全部画布";
  return <Modal open={open} onClose={confirming || renaming ? back : onClose} title={title} size={confirming || renaming ? "sm" : "xl"} footer={confirming ? <><Button variant="secondary" disabled={busy} onClick={back}>取消</Button><Button variant="danger" loading={busy} onClick={() => void remove()}>确认删除</Button></> : renaming ? <><Button variant="secondary" disabled={busy} onClick={back}>取消</Button><Button disabled={busy || !renaming.name.trim()} loading={busy} onClick={() => void rename()}>保存</Button></> : undefined}>
    <div className="space-y-4">
      {error && <ErrorState title="画布管理未完成" description={error} />}
      {confirming ? <p className="text-sm leading-relaxed">确定删除 {confirm.map(canvas => `“${canvas.name}”`).join("、")}？删除画布会移除其布局，已绑定的智能体与资源仍会保留。</p> : renaming ? <Input label="画布名称" value={renaming.name} onChange={event => setRenaming({ ...renaming, name: event.target.value })} /> : <>
        <div className="flex flex-wrap items-center justify-between gap-3"><p className="text-sm text-[var(--text-muted)]">管理画布。任务图可在工作流中管理。</p><Button size="sm" variant="danger" disabled={busy || selected.length === 0} onClick={() => { setError(""); setConfirm(canvases.filter(canvas => selected.includes(canvas.id))); }}>删除选中项（{selected.length}）</Button></div>
        {canvases.length === 0 ? <p>暂无画布。</p> : canvases.map(canvas => <article key={canvas.id} className="flex flex-wrap items-center gap-3 border border-[var(--border-soft)] rounded-xl p-3"><input aria-label={`选择画布 ${canvas.name}`} type="checkbox" checked={selected.includes(canvas.id)} onChange={event => setSelected(current => event.target.checked ? [...current, canvas.id] : current.filter(id => id !== canvas.id))} /><span className="mr-auto">{canvas.name}{canvas.id === activeId ? " · 当前" : ""}</span><Button size="sm" variant="secondary" onClick={() => { setError(""); setRenaming({ id: canvas.id, name: canvas.name }); }}>重命名</Button><Button size="sm" variant="danger" disabled={busy} onClick={() => { setError(""); setConfirm([canvas]); }}>删除此画布</Button></article>)}
        {results.map(message => <p key={message} role="status" className="text-sm break-words">{message}</p>)}
      </>}
    </div>
  </Modal>;
}
