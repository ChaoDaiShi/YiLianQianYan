import { useEffect, useState } from "react";
import { Button } from "../../../components/ui";
import { getSkillDiscovery, refreshSkillDiscovery, type SkillDiscoveryReport, type SkillDiscoverySource } from "../../../api/skillDiscovery";

export function SkillSourceList({ sources }: { sources: SkillDiscoverySource[] }) {
  return <div className="space-y-2" aria-label="技能发现来源">{sources.map(source => <div key={source.id} className="rounded-xl border border-[var(--border-soft)] bg-[var(--surface-solid)] p-3">
    <div className="flex items-center justify-between gap-3"><strong className="text-xs font-medium">{source.label}</strong><span className="text-[11px] text-[var(--text-muted)]">{source.exists ? `${source.skill_count} 项技能` : "目录不存在"}</span></div>
    <p className="text-[11px] text-[var(--text-muted)] break-all mt-1 font-mono">{source.path}</p>
    {source.error && <p role="alert" className="text-xs text-[var(--danger)] mt-2">{source.error}</p>}
  </div>)}</div>;
}
export default function SkillDiscoverySources({ onRefreshed }: { onRefreshed?: () => void }) {
  const [report, setReport] = useState<SkillDiscoveryReport | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  useEffect(() => {
    let active = true;
    void getSkillDiscovery().then(result => {
      if (!active) return;
      if (result.ok) setReport(result.data); else setError(result.error);
    });
    return () => { active = false; };
  }, []);
  async function refresh() {
    setBusy(true); setError("");
    const result = await refreshSkillDiscovery();
    if (result.ok) {
      setReport(result.data);
      onRefreshed?.();
    } else setError(result.error);
    setBusy(false);
  }
  const failedSources = report?.sources.filter(source => source.error).length ?? 0;
  return <section className="space-y-3" aria-label="自动发现技能">
    <div className="flex flex-wrap items-center justify-between gap-2">
      {report ? <p className="settings-help" role="status">已发现 {report.skill_count} 项技能 · {report.sources.filter(source => source.exists).length} 个来源</p>
        : !error && <p className="settings-help" role="status">正在读取技能发现来源…</p>}
      <Button variant="secondary" size="sm" onClick={() => void refresh()} disabled={busy}>{busy ? "正在刷新" : "刷新发现"}</Button>
    </div>
    {error && <p role="alert" className="settings-save-error">{error}</p>}
    {failedSources > 0 && <p role="alert" className="settings-save-error">{failedSources} 个技能来源读取失败，可展开下方查看。</p>}
    <details className="u2-disclosure">
      <summary>发现来源与说明</summary>
      <div className="space-y-3 pt-2">
        <p className="settings-help">会扫描工作空间、用户目录和插件缓存中的 Codex、Claude Code、通用 Agent、GitHub / VS Code 与 Copilot 技能目录。外部技能只读，脚本和参考资料从原目录读取。</p>
        <p className="settings-help">缺少插件宿主工具的技能可在技能页查看和适配。</p>
        {report && <SkillSourceList sources={report.sources} />}
      </div>
    </details>
  </section>;
}
