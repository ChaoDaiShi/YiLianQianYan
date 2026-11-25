import { useEffect, useState, type ReactNode } from "react";
import { Button, Input, Select } from "../../components/ui";
import { activateLlmModel, createLlmModel, listLlmModels, updateLlmModel, verifyLlmModel } from "../../api/client";
import type { LlmModel } from "../../types";
import { PROVIDER_PRESETS, type ProviderPresetId } from "./llmModelUtils";
import { listProviderAccounts, poolRequest, type ProviderAccount } from "./modelPools";
import AccountCredentialFields from "./AccountCredentialFields";
import { credentialChoice, credentialReady } from "./accountCredentials";
import { finishQuickModelSetup } from "./quickModelCompletion";
import ModelCatalogField, { type UpstreamModel } from "./ModelCatalogField";

const providerOptions = Object.entries(PROVIDER_PRESETS).map(([value, preset]) => ({ value, label: preset.label }));
export default function QuickModelSetup({ onSaved, children }: {
  onSaved?: () => void;
  children: (form: { fields: ReactNode; action: ReactNode; error: string; notice: string }) => ReactNode;
}) {
  const [accounts, setAccounts] = useState<ProviderAccount[]>([]);
  const [provider, setProvider] = useState<ProviderPresetId>("deepseek");
  const [accountId, setAccountId] = useState("");
  const [base, setBase] = useState(PROVIDER_PRESETS.deepseek.baseUrl);
  const [model, setModel] = useState("");
  const [label, setLabel] = useState("");
  const [catalogModels, setCatalogModels] = useState<UpstreamModel[]>([]);
  const [key, setKey] = useState("");
  const [credentialModelId, setCredentialModelId] = useState("");
  const [models, setModels] = useState<LlmModel[]>([]);
  const [useVision, setUseVision] = useState(false);
  const [visionModel, setVisionModel] = useState("deepseek-flash");
  const [linkedVision, setLinkedVision] = useState(false);
  const [savedModelId, setSavedModelId] = useState<string | null>(null);
  const [savedVision, setSavedVision] = useState<{id:string;revision:number} | null>(null);
  const [busy, setBusy] = useState(false);
  const [feedback, setFeedback] = useState<{ text: string; error: boolean } | null>(null);
  const currentAccount = accounts.find(account => account.id === accountId);
  const providerAccounts = accounts.filter(account => account.provider_id === provider);
  const displayLabel = label.trim() || catalogModels.find(item => item.id === model)?.name || model;
  const effectiveBase = currentAccount?.base_url ?? base;
  const visionOptions = catalogModels.filter(item => !item.input_modalities || item.input_modalities.includes("image"));
  const needsAddress = ["local", "custom"].includes(PROVIDER_PRESETS[provider].category);
  useEffect(() => { void Promise.all([listProviderAccounts(), listLlmModels()]).then(([accounts, models]) => { setAccounts(accounts); if (models) setModels(models); else setFeedback({ text: "无法读取已保存的模型凭据来源。", error: true }); }).catch(e => setFeedback({ text: String(e), error: true })); }, []);
  useEffect(() => {
    if (!accountId) { setLinkedVision(false); return; }
    let active = true;
    setLinkedVision(false);
    void Promise.all([poolRequest<Array<{id:string;provider_account_id:string;model:string;service_kind:string}>>("GET", "/api/service-profiles"), poolRequest<{source:string|null;id:string|null}>("GET", "/api/service-defaults/vision")]).then(([profiles, selected]) => { if (active) setLinkedVision(profiles.some(p=>p.id===selected.id && selected.source==="profile" && p.provider_account_id===accountId && p.model===visionModel)); }).catch(e=>{ if (active) setFeedback({ text: String(e), error: true }); });
    return () => { active = false; };
  }, [accountId, visionModel]);
  async function save() {
    setBusy(true); setFeedback(null);
    let savedModel = false;
    try {
      // Model creation updates the account's profile list on the server. Read
      // that list before saving credentials so a retry keeps the saved model linked.
      const currentAccounts = await listProviderAccounts();
      setAccounts(currentAccounts);
      let account = currentAccounts.find(a => a.id === accountId);
      if (accountId && !account) throw new Error("所选账户已不存在，请重新选择账户。");
      if (!credentialReady(account, provider, key, credentialModelId)) throw new Error("请先输入 API Key 或明确选择已配置模型凭据。");
      if (!account) {
        account = await poolRequest<ProviderAccount>("POST", "/api/provider-accounts", { id:"", provider_id:provider, label:PROVIDER_PRESETS[provider].label, base_url:base, credential_id:null, profile_ids:[], ...credentialChoice(key, credentialModelId) });
        setAccounts(current => [...current, account!]); setAccountId(account.id); setKey(""); setCredentialModelId("");
      } else if (key.trim() || credentialModelId) {
        account = await poolRequest<ProviderAccount>("PUT", `/api/provider-accounts/${encodeURIComponent(account.id)}`, { id:account.id, provider_id:account.provider_id, label:account.label, base_url:account.base_url, credential_id:account.credential_id, profile_ids:account.profile_ids, ...credentialChoice(key, credentialModelId) });
        setAccounts(current => current.map(item => item.id === account!.id ? account! : item)); setKey(""); setCredentialModelId("");
      }
      if (!credentialReady(account, account.provider_id, "", "")) throw new Error("账户凭据尚不可用，请检查 API Key 或凭据存储。");
      const accountIdForVision = account.id;
      const upstreamLimit = catalogModels.find(item => item.id === model)?.max_output_tokens;
      const payload = {provider_account_id:account.id, provider:account.provider_id, label:displayLabel, model, base_url:account.base_url, api_format:"openai" as const, api_key:"", api_key_env:"", temperature:0, max_tokens:upstreamLimit ? Math.min(16384, upstreamLimit) : 16384, invoke_timeout_ms:120000};
      const saved = savedModelId ? await updateLlmModel(savedModelId, payload) : await createLlmModel(payload);
      if (!saved) throw new Error("模型保存失败，账户已保留，可重试。");
      savedModel = true; setSavedModelId(saved.id); onSaved?.();
      const completion = await finishQuickModelSetup({
        verify: () => verifyLlmModel(saved.id),
        activate: () => activateLlmModel(saved.id),
        configureVision: async () => {
        if (useVision && (!linkedVision || savedVision)) {
          const vision = await poolRequest<{id:string;revision:number}>(savedVision ? "PUT" : "POST", savedVision ? `/api/service-profiles/${savedVision.id}` : "/api/service-profiles", {label:`${displayLabel} · 识图`, provider_account_id:accountIdForVision, service_kind:"vision", model:visionModel, adapter:"openai", parameters:{}, expected_revision:savedVision?.revision ?? null});
          setSavedVision(vision);
          await poolRequest("PUT", "/api/service-defaults/vision", {source:"profile",id:vision.id}); setLinkedVision(true);
        }
        },
      });
      if (completion === "verify_failed") { setFeedback({ text: "模型已保存，连接验证未通过。", error: true }); return; }
      if (completion === "activate_failed") { setFeedback({ text: "模型已验证，但启用失败。", error: true }); return; }
      onSaved?.();
      await poolRequest("PUT", "/api/model-pool-bindings/default", {pool_id:null});
      setFeedback({ text: `已保存并启用 ${displayLabel}`, error: false });
    } catch (e) { setFeedback({ text: `${savedModel ? "对话模型已保存，后续步骤未完成。" : ""}${e instanceof Error ? e.message : String(e)}`, error: true }); } finally { setBusy(false); }
  }
  const fields = <section className="space-y-3" aria-label="对话模型快速配置">
    <Select label="服务商" value={provider} options={providerOptions} onChange={e => { const id=e.target.value as ProviderPresetId; setProvider(id); setAccountId(""); setKey(""); setCredentialModelId(""); setSavedModelId(null); setSavedVision(null); setUseVision(false); setLinkedVision(false); setVisionModel(""); setBase(PROVIDER_PRESETS[id].baseUrl); setModel(""); setLabel(""); setCatalogModels([]); }} />
    {providerAccounts.length > 0 && <Select label="账户" value={accountId} options={[{ value: "", label: "创建新账户" }, ...providerAccounts.map(account => ({ value: account.id, label: account.label }))]} onChange={e=>{setAccountId(e.target.value);setKey("");setCredentialModelId("");setLinkedVision(false);setSavedModelId(null);setSavedVision(null);}} />}
    {!accountId && needsAddress && <Input label="API 地址" value={base} onChange={event => { setBase(event.target.value); setCredentialModelId(""); }} />}
    <AccountCredentialFields key={provider + accountId} account={currentAccount} provider={provider} baseUrl={effectiveBase} models={models} apiKey={key} modelId={credentialModelId} onKey={setKey} onModel={setCredentialModelId}/>
    <ModelCatalogField provider={provider} baseUrl={effectiveBase} apiKey={key} accountId={credentialModelId ? undefined : accountId} modelId={credentialModelId} enabled={credentialReady(currentAccount, provider, key, credentialModelId)}
      preserveValue={Boolean(savedModelId)} preferredId={PROVIDER_PRESETS[provider].model} value={model} onChange={id => setModel(id)} onLoaded={items => {
        setCatalogModels(items);
        const candidates = items.filter(item => !item.input_modalities || item.input_modalities.includes("image"));
        setVisionModel(current => candidates.some(item => item.id === current) ? current : candidates[0]?.id ?? "");
      }} advancedFields={<>
      {!accountId && !needsAddress && <Input label="API 地址" value={base} onChange={event => { setBase(event.target.value); setCredentialModelId(""); }} />}
      <Input label="显示名称（可选）" value={label} onChange={event => setLabel(event.target.value)} placeholder="默认使用上游模型名称" />
      {(PROVIDER_PRESETS[provider].services as string[]).includes("vision") && <>
        <label className="flex gap-2 text-sm"><input type="checkbox" checked={useVision} onChange={event => setUseVision(event.target.checked)} />同时配置默认识图服务</label>
        {useVision && <Select label="识图模型" value={visionModel} onChange={event => setVisionModel(event.target.value)} options={visionOptions.map(item => ({ value: item.id, label: item.name }))} />}
        {linkedVision && <p className="text-sm">识图服务已关联此账户。</p>}
      </>}
      <p className="text-xs text-[var(--text-secondary)]">保存并使用固定模型会解除默认对话模型池绑定。</p>
      </>} />
  </section>;
  const action = <Button disabled={busy || !model.trim() || (useVision && !visionModel.trim()) || (!accountId && !base.trim()) || !credentialReady(currentAccount, provider, key, credentialModelId)}
    onClick={() => void save()}>{busy ? "正在保存并验证…" : "保存并使用"}</Button>;
  return children({ fields, action, error: feedback?.error ? feedback.text : "", notice: feedback && !feedback.error ? feedback.text : "" });
}
