import { Input } from "../../components/ui";
import type { LlmModel } from "../../types";
import type { ProviderAccount } from "./modelPools";
import { credentialLabels, eligibleCredentialModels } from "./accountCredentials";

export default function AccountCredentialFields({ account, provider, baseUrl, models, apiKey, modelId, onKey, onModel }: {
  account?: ProviderAccount; provider: string; baseUrl: string; models: LlmModel[]; apiKey: string; modelId: string;
  onKey: (value: string) => void; onModel: (value: string) => void;
}) {
  const eligible = eligibleCredentialModels(models, provider, baseUrl);
  const status = account ? account.credential_status : (provider === "ollama" || provider === "lmstudio" ? "not_required" : "missing");
  return <div className="space-y-2"><p className="text-sm text-[var(--text-muted)]">{status ? credentialLabels[status] : "凭据状态不可用"}</p>
    <Input label={account?.credential_configured ? "替换 API Key（可选）" : "API Key"} type="password" autoComplete="new-password" value={apiKey} onChange={event => { onKey(event.target.value); if (event.target.value) onModel(""); }} />
    {eligible.length > 0 && <label className="block text-sm">复用已配置模型凭据<select aria-label="复用已配置模型凭据" className="w-full rounded-xl border border-[var(--border-soft)] bg-[var(--surface-solid)] p-2 text-sm" value={modelId} onChange={event => { onModel(event.target.value); if (event.target.value) onKey(""); }}><option value="">不复用</option>{eligible.map(model => <option key={model.id} value={model.id}>{model.label} · {model.model}</option>)}</select></label>}
    {(modelId || account?.credential_configured) && <p className="text-xs text-[var(--text-muted)]">复用会共享同一凭据引用；后续修改共享凭据会影响使用该引用的模型和账户。</p>}
  </div>;
}
