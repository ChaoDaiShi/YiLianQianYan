import type { ProviderConnectionKind } from "../../../api/providerConnection";
import { Badge, Button } from "../../../components/ui";
import type { ProviderReadinessProjection } from "../../../types";

interface ProviderReadinessCardProps {
  readiness: ProviderReadinessProjection | null;
  kinds: ProviderConnectionKind[];
  testing: ProviderConnectionKind | null;
  results: Partial<Record<ProviderConnectionKind, string>>;
  onVerify: (kind: ProviderConnectionKind) => void;
  onPreviewTts?: () => void;
}

/**
 * Readiness is a local statement — it reflects the running configuration and
 * whether a credential can be resolved. It is deliberately not presented as
 * proof that the provider is reachable.
 */
export default function ProviderReadinessCard({
  readiness,
  kinds,
  testing,
  results,
  onVerify,
  onPreviewTts,
}: ProviderReadinessCardProps) {
  if (!readiness) return null;
  const labels: Record<keyof ProviderReadinessProjection, string> = { model: "模型", stt: "STT", tts: "TTS" };
  return (
    <section className="rounded-xl border border-[var(--border-soft)] bg-[var(--surface-muted)] p-4" aria-label="Provider 就绪状态">
      <h3 className="text-sm font-semibold">Provider 就绪状态</h3>
      <p className="mt-1 text-xs text-[var(--text-muted)]">仅反映当前运行配置与凭据可解析性，不将其视为外部服务连通性证明。</p>
      <div className="mt-3 grid gap-2 sm:grid-cols-3">
        {kinds.map((kind) => {
          const item = readiness[kind];
          return <div key={kind} className="min-w-0 rounded-lg border border-[var(--border-soft)] bg-[var(--surface-solid)] px-3 py-2">
            <div className="flex items-center justify-between gap-2"><span className="text-xs font-medium">{labels[kind]}</span><Badge tone={item.available ? "success" : "default"}>{item.available ? "已就绪" : "未就绪"}</Badge></div>
            <p className="mt-1 truncate text-meta text-[var(--text-muted)]">{item.provider || "未指定 Provider"} · {item.model || "未指定模型"}</p>
            <div className="mt-1 flex items-center justify-between gap-2"><p className="text-meta text-[var(--text-faint)]">凭据：{item.configured ? "已配置" : "未配置"}</p><Button variant="secondary" size="sm" disabled={testing !== null} onClick={() => kind === "tts" && onPreviewTts ? onPreviewTts() : onVerify(kind)}>{testing === kind ? "测试中…" : kind === "tts" ? "试听声音" : "测试连接"}</Button></div>
            {results[kind] ? <p className="mt-1 text-meta text-[var(--text-muted)]" role="status">{results[kind]}</p> : null}
          </div>;
        })}
      </div>
    </section>
  );
}
