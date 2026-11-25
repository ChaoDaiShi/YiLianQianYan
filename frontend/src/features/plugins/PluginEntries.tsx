import { useEffect, useState } from "react";
import { Link } from "react-router-dom";
import { managementRequest } from "../../api/productManagement";
import { listPluginFeatures, type PluginFeatureEntry } from "../../api/plugins";
import Button from "../../components/ui/Button";

type SettingEntry = { plugin_id: string; node_id: string; setting: { title: string } };
type Catalog = { features: PluginFeatureEntry[]; settings: SettingEntry[]; errors: string[] };

export default function PluginEntries({ pluginId, revision }: { pluginId: string; revision: number }) {
  const [catalog, setCatalog] = useState<Catalog | null>(null);
  const [retry, setRetry] = useState(0);
  useEffect(() => {
    let active = true;
    setCatalog(null);
    void Promise.allSettled([
      listPluginFeatures(),
      managementRequest<{ settings: SettingEntry[] }>("GET", "/api/plugins/settings"),
    ]).then(([features, settings]) => {
      if (!active) return;
      const errors = [features, settings].flatMap(result => result.status === "rejected" ? [String(result.reason)] : []);
      setCatalog({
        features: features.status === "fulfilled" ? features.value : [],
        settings: settings.status === "fulfilled" ? settings.value.settings : [],
        errors,
      });
    });
    return () => { active = false; };
  }, [revision, retry]);

  const features = catalog?.features.filter(item => item.plugin_id === pluginId) ?? [];
  const settings = catalog?.settings.filter(item => item.plugin_id === pluginId) ?? [];
  return <section className="space-y-2" aria-label="插件入口">
    <h4 className="text-sm font-semibold">插件入口</h4>
    {!catalog ? <p role="status" className="text-xs text-[var(--text-secondary)]">正在读取功能和设置…</p> : <>
      {catalog.errors.length > 0 && <div role="alert" className="space-y-1 break-words text-xs text-[var(--danger-fg)]">
        {catalog.errors.map((message, index) => <p key={index}>{message}</p>)}
        <Button size="sm" variant="secondary" onClick={() => setRetry(value => value + 1)}>重试读取插件入口</Button>
      </div>}
      <ul className="flex flex-wrap gap-2">
        {features.map(feature => <li key={feature.id}><Link className="plugin-entry-link" to={`/more/plugins/${encodeURIComponent(pluginId)}/${encodeURIComponent(feature.id)}`}>{feature.title}</Link></li>)}
        {settings.map(entry => <li key={entry.node_id}><Link className="plugin-entry-link" to={`/settings?section=${encodeURIComponent(entry.node_id)}`}>{entry.setting.title}</Link></li>)}
      </ul>
      {!catalog.errors.length && !features.length && !settings.length && <p className="text-xs text-[var(--text-secondary)]">暂无独立的功能或设置页面。</p>}
    </>}
  </section>;
}
