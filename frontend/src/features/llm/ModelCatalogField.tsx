import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { RefreshCw } from "lucide-react";
import { requestResult } from "../../core/api/http";
import { Button, Input, Select } from "../../components/ui";

export interface UpstreamModel {
  id: string;
  name: string;
  context_window?: number | null;
  max_output_tokens?: number | null;
  input_modalities?: string[] | null;
  output_modalities?: string[] | null;
}

export default function ModelCatalogField({ provider, baseUrl, apiKey = "", accountId, modelId, apiKeyEnv = "", enabled = true, preserveValue = false, preferredId, value, onChange, onLoaded, advancedFields }: {
  provider: string; baseUrl: string; apiKey?: string; accountId?: string; modelId?: string; apiKeyEnv?: string;
  enabled?: boolean; preserveValue?: boolean; preferredId?: string; value: string;
  onChange: (id: string, model?: UpstreamModel) => void;
  onLoaded?: (models: UpstreamModel[]) => void;
  advancedFields?: ReactNode;
}) {
  const request = useMemo(() => ({ provider, base_url: baseUrl.trim(), api_key: apiKey, provider_account_id: accountId || undefined, model_id: modelId || undefined, api_key_env: apiKeyEnv }), [provider, baseUrl, apiKey, accountId, modelId, apiKeyEnv]);
  const [result, setResult] = useState<{ request: typeof request; models: UpstreamModel[]; loading: boolean; error: string }>();
  const [revision, setRevision] = useState(0);
  const selection = useRef({ value, preserveValue, preferredId, onChange, onLoaded });
  selection.current = { value, preserveValue, preferredId, onChange, onLoaded };
  const manualRequest = useRef<typeof request>();
  const ready = enabled && Boolean(baseUrl.trim()) && Boolean(apiKey.trim() || accountId || modelId || apiKeyEnv.trim() || provider === "ollama" || provider === "lmstudio" || provider === "llamacpp");
  const current = result?.request === request ? result : undefined;

  useEffect(() => {
    if (!ready) return;
    let active = true;
    const timer = window.setTimeout(() => {
      setResult({ request, models: [], loading: true, error: "" });
      void requestResult<{ models: UpstreamModel[] }>("POST", "/api/llm/models/discover", request).then(response => {
        if (!active) return;
        if (!response.ok) {
          setResult({ request, models: [], loading: false, error: response.error });
          return;
        }
        const models = response.data.models;
        setResult({ request, models, loading: false, error: models.length ? "" : "上游暂未返回可用模型" });
        const selected = selection.current;
        selected.onLoaded?.(models);
        if (models.length && manualRequest.current !== request && !models.some(model => model.id === selected.value) && (!selected.preserveValue || !selected.value)) {
          const preferred = models.find(model => model.id === selected.preferredId) ?? (models.length === 1 ? models[0] : undefined);
          selected.onChange(preferred?.id ?? "", preferred);
        }
      });
    }, 650);
    return () => { active = false; window.clearTimeout(timer); };
  }, [request, ready, revision]);

  const models = current?.models ?? [];
  const options = models.map(model => ({ value: model.id, label: model.name === model.id ? model.id : `${model.name} · ${model.id}` }));
  if (value && !models.some(model => model.id === value)) options.unshift({ value, label: `${value}（当前配置）` });
  return <div className="space-y-2" aria-label="上游模型选择">
    <div className="flex items-end gap-2">
      <div className="min-w-0 flex-1"><Select label="选择模型" value={value} options={[{ value: "", label: current?.loading ? "正在读取上游模型…" : "选择上游模型", disabled: true }, ...options]}
        disabled={current?.loading || (!models.length && !value)} onChange={event => onChange(event.target.value, models.find(model => model.id === event.target.value))} /></div>
      <Button variant="secondary" size="sm" iconOnly aria-label="刷新上游模型" title="刷新上游模型" disabled={!ready || current?.loading} loading={current?.loading}
        onClick={() => setRevision(count => count + 1)}><RefreshCw size={15} /></Button>
    </div>
    {!ready && <p className="text-xs text-[var(--text-secondary)]">填写密钥后自动读取可用模型。</p>}
    {ready && !current && <p role="status" className="text-xs text-[var(--text-secondary)]">即将读取上游模型…</p>}
    {current?.loading && <p role="status" className="text-xs text-[var(--text-secondary)]">正在读取上游模型…</p>}
    {current?.error && <p role="alert" className="text-xs text-[var(--danger-fg)]">{current.error}</p>}
    <details className={advancedFields ? "u2-disclosure" : "text-xs text-[var(--text-secondary)]"}>
      <summary className="cursor-pointer">{advancedFields ? "高级选项" : "手动填写模型"}</summary>
      <div className="space-y-3 pt-2">
        <Input label="手动模型 ID" value={value} onChange={event => { manualRequest.current = request; onChange(event.target.value); }} placeholder="用于未提供模型列表的服务" />
        {advancedFields}
      </div>
    </details>
  </div>;
}
