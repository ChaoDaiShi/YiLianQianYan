import type { ProviderConnectionKind } from "../../../api/providerConnection";
import { Badge, Button } from "../../../components/ui";
import type { ProviderReadinessProjection } from "../../../types";
import { connectionPresentation } from "../model/workbench";

interface ProviderReadinessCardProps {
  dirty?: boolean;
  readiness: ProviderReadinessProjection | null;
  kind: Exclude<ProviderConnectionKind, "embedding">;
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
  dirty = false,
  readiness,
  kind,
  testing,
  results,
  onVerify,
  onPreviewTts,
}: ProviderReadinessCardProps) {
  if (!readiness) return null;
  const item = readiness[kind];
  const result = results[kind];
  const presentation = connectionPresentation(item.configured, item.available, result, dirty);
  return (
    <section className="space-y-2" aria-label="连接状态">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <Badge tone={presentation.tone}>{presentation.label}</Badge>
        <Button variant="secondary" size="sm" disabled={testing !== null || dirty} title={dirty ? "保存后可测试" : undefined} onClick={() => kind === "tts" && onPreviewTts ? onPreviewTts() : onVerify(kind)}>
          {testing === kind ? "测试中…" : kind === "tts" ? "试听声音" : "测试连接"}
        </Button>
      </div>
      {result && <p role={presentation.tone === "danger" ? "alert" : "status"} className={presentation.tone === "danger" ? "settings-save-error" : "text-sm"}>{result}</p>}
    </section>
  );
}
