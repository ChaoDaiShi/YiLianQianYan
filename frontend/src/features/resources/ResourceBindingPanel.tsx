import { useCallback, useEffect, useRef, useState } from "react";
import { bindResources, getResource, listResourceBindings, listResources, unbindResource, type Resource, type ResourceBinding, type ResourceTarget } from "../../api/resources";
import { ResourcePreviewDisclosure } from "./ResourceAttachments";

export default function ResourceBindingPanel({ target }: { target: ResourceTarget }) {
  const key = JSON.stringify(target);
  const latestKey = useRef(key); latestKey.current = key;
  const [resources, setResources] = useState<Resource[]>([]);
  const [bindings, setBindings] = useState<Array<ResourceBinding & { resource?: Resource | null }>>([]);
  const [selected, setSelected] = useState<string[]>([]); const [error, setError] = useState(""); const [busy, setBusy] = useState(false);
  const reload = useCallback(async () => {
    try {
      const current = await listResourceBindings(JSON.parse(key) as ResourceTarget);
      const enriched = await Promise.all(current.map(async binding => ({ ...binding, resource: await getResource(binding.resource_id) })));
      const resources = await listResources();
      if (latestKey.current !== key) return;
      setBindings(enriched); setResources(resources); setError("");
    } catch (error) { if (latestKey.current === key) setError(error instanceof Error ? error.message : "资源绑定不可用"); }
  }, [key]);
  useEffect(() => { setSelected([]); setBindings([]); void reload(); }, [reload]);
  const run = async (operation: () => Promise<unknown>) => { setBusy(true); try { await operation(); setSelected([]); await reload(); } catch (error) { setError(error instanceof Error ? error.message : "操作失败"); } finally { setBusy(false); } };
  return <section className="space-y-2 rounded border border-[var(--border)] p-3 text-xs" aria-label="绑定的输入资源">
    <h3 className="font-medium">输入资源</h3><p>只使用明确绑定的资源；变更用于下一次执行，不回写已有执行记录。</p>
    {error ? <p role="alert" className="text-[var(--danger)]">{error}</p> : null}
    <select multiple aria-label="选择要绑定的资源" value={selected} onChange={event => setSelected(Array.from(event.currentTarget.selectedOptions, option => option.value))} disabled={busy} className="w-full bg-[var(--surface)]">
      {resources.map(resource => <option key={resource.id} value={resource.id}>{resource.name}</option>)}
    </select>
    <button type="button" disabled={busy || !selected.length || selected.length > 8} onClick={() => void run(() => bindResources(target, selected))}>绑定所选资源</button>
    {!bindings.length ? <p>尚未绑定输入资源</p> : <ul className="space-y-2">{bindings.map(binding => <li key={binding.id}>
      <div className="flex justify-between"><span>{binding.resource?.name || "资源记录缺失"}</span><button type="button" disabled={busy} onClick={() => void run(() => unbindResource(binding.id))}>解除绑定</button></div>
      <ResourcePreviewDisclosure resourceId={binding.resource_id} />
    </li>)}</ul>}
  </section>;
}
