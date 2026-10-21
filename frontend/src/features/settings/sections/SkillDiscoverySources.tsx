import { useEffect, useState } from "react";
import { RefreshCw } from "lucide-react";
import { getSkillDiscovery, refreshSkillDiscovery, type SkillDiscoveryReport, type SkillDiscoverySource } from "../../../api/skillDiscovery";

export function SkillSourceList({ sources }: { sources: SkillDiscoverySource[] }) {
  return <div className="space-y-2 max-h-80 overflow-auto" aria-label="技能发现来源">{sources.map(source => <div key={source.id} className="rounded-xl border border-[var(--border-soft)] bg-[var(--surface-solid)] p-3">
    <div className="flex items-center justify-between gap-3"><strong className="text-xs font-medium">{source.label}</strong><span className="text-[11px] text-[var(--text-muted)]">{source.exists ? `${source.skill_count} 项技能` : "目录不存在"}</span></div>
    <p className="text-[11px] text-[var(--text-muted)] break-all mt-1 font-mono">{source.path}</p>
    {source.error && <p role="alert" className="text-xs text-[var(--danger)] mt-2">{source.error}</p>}
  </div>)}</div>;
}
export default function SkillDiscoverySources() {
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
    if (result.ok) setReport(result.data); else setError(result.error);
    setBusy(false);
  }
  return <section className="settings-card" aria-label="自动发现技能">
    <header className="settings-card-header"><div><h3>自动发现本机技能</h3><p>读取 Codex、Claude Code、通用 Agent、GitHub / VS Code 与 Copilot 的技能目录。</p></div><button type="button" className="settings-action-button flex items-center gap-2" onClick={() => void refresh()} disabled={busy}><RefreshCw size={14} className={busy ? "animate-spin" : ""}/>{busy ? "正在刷新" : "刷新发现"}</button></header>
    <div className="settings-card-body space-y-3">
      <p className="settings-help">自动扫描工作空间和用户目录中的 .agents/skills、.codex/skills、.claude/skills、.github/skills、.copilot/skills，以及 Codex / Claude Code 插件缓存中已有的 skills 目录。自定义目录会同时保留；外部文件保持只读，scripts 与 references 使用原始技能根目录。</p>
      <p className="settings-help">自定义目录保存后，点击刷新发现以应用。技能是流程说明，插件宿主函数不会随 SKILL.md 出现；缺少宿主工具的流程仅供查看，不进入 AI 可选技能。可在技能页查看适配说明，改写为本项目工具后新增工作区技能。</p>
      {error && <p role="alert" className="text-xs text-[var(--danger)]">{error}</p>}
      {report ? <><p className="text-xs text-[var(--text-muted)]" role="status">已发现 {report.skill_count} 项技能 · {report.sources.filter(source => source.exists).length} 个已有来源</p><SkillSourceList sources={report.sources}/></> : !error && <p role="status" className="settings-help">正在读取技能发现来源…</p>}
    </div>
  </section>;
}
