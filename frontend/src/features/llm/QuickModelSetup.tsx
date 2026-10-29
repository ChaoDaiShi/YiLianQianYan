import { useEffect, useState } from "react";
import { Button, Input } from "../../components/ui";
import { activateLlmModel, createLlmModel, listLlmModels, updateLlmModel, verifyLlmModel } from "../../api/client";
import type { LlmModel } from "../../types";
import { PROVIDER_PRESETS, type ProviderPresetId } from "./llmModelUtils";
import { listProviderAccounts, poolRequest, type ProviderAccount } from "./modelPools";
import AccountCredentialFields from "./AccountCredentialFields";
import { credentialChoice, credentialReady } from "./accountCredentials";
import { finishQuickModelSetup } from "./quickModelCompletion";

const selectClass = "w-full rounded-xl border border-[var(--border-soft)] bg-[var(--surface-solid)] p-2 text-sm";
export default function QuickModelSetup({ onSaved }: { onSaved?: () => void }) {
  const [accounts, setAccounts] = useState<ProviderAccount[]>([]);
  const [provider, setProvider] = useState<ProviderPresetId>("deepseek");
  const [accountId, setAccountId] = useState("");
  const [base, setBase] = useState(PROVIDER_PRESETS.deepseek.baseUrl);
  const [model, setModel] = useState("deepseek-flash");
  const [label, setLabel] = useState("DeepSeek Flash");
  const [key, setKey] = useState("");
  const [credentialModelId, setCredentialModelId] = useState("");
  const [models, setModels] = useState<LlmModel[]>([]);
  const [useVision, setUseVision] = useState(false);
  const [visionModel, setVisionModel] = useState("deepseek-flash");
  const [linkedVision, setLinkedVision] = useState(false);
  const [savedModelId, setSavedModelId] = useState<string | null>(null);
  const [savedVision, setSavedVision] = useState<{id:string;revision:number} | null>(null);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState("");
  useEffect(() => { void Promise.all([listProviderAccounts(), listLlmModels()]).then(([accounts, models]) => { setAccounts(accounts); if (models) setModels(models); else setMessage("无法读取已保存的模型凭据来源。"); }).catch(e => setMessage(String(e))); }, []);
  useEffect(() => {
    if (!accountId) { setLinkedVision(false); return; }
    let active = true;
    setLinkedVision(false);
    void Promise.all([poolRequest<Array<{id:string;provider_account_id:string;model:string;service_kind:string}>>("GET", "/api/service-profiles"), poolRequest<{source:string|null;id:string|null}>("GET", "/api/service-defaults/vision")]).then(([profiles, selected]) => { if (active) setLinkedVision(profiles.some(p=>p.id===selected.id && selected.source==="profile" && p.provider_account_id===accountId && p.model===visionModel)); }).catch(e=>{ if (active) setMessage(String(e)); });
    return () => { active = false; };
  }, [accountId, visionModel]);
  async function save() {
    setBusy(true); setMessage("");
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
      const payload = {provider_account_id:account.id, provider:account.provider_id, label, model, base_url:account.base_url, api_format:"openai" as const, api_key:"", api_key_env:"", temperature:0, max_tokens:16384, invoke_timeout_ms:120000};
      const saved = savedModelId ? await updateLlmModel(savedModelId, payload) : await createLlmModel(payload);
      if (!saved) throw new Error("模型保存失败，账户已保留，可重试。");
      savedModel = true; setSavedModelId(saved.id); onSaved?.();
      const completion = await finishQuickModelSetup({
        verify: () => verifyLlmModel(saved.id),
        activate: () => activateLlmModel(saved.id),
        configureVision: async () => {
        if (useVision && (!linkedVision || savedVision)) {
          const vision = await poolRequest<{id:string;revision:number}>(savedVision ? "PUT" : "POST", savedVision ? `/api/service-profiles/${savedVision.id}` : "/api/service-profiles", {label:`${label} · 识图`, provider_account_id:accountIdForVision, service_kind:"vision", model:visionModel, adapter:"openai", parameters:{}, expected_revision:savedVision?.revision ?? null});
          setSavedVision(vision);
          await poolRequest("PUT", "/api/service-defaults/vision", {source:"profile",id:vision.id}); setLinkedVision(true);
        }
        },
      });
      if (completion === "verify_failed") { setMessage("模型已保存，连接验证未通过；识图默认配置未更改。"); return; }
      if (completion === "activate_failed") { setMessage("模型已保存并验证，但启用失败；识图默认配置未更改。"); return; }
      onSaved?.();
      await poolRequest("PUT", "/api/model-pool-bindings/default", {pool_id:null});
      setMessage(`已保存并启用 ${label}，默认对话模型池绑定已解除。`);
    } catch (e) { setMessage(`${savedModel ? "对话模型已保存；后续步骤未完成。" : ""}${e instanceof Error ? e.message : String(e)}`); } finally { setBusy(false); }
  }
  return <section className="settings-subcard space-y-3"><div><h3 className="font-semibold">添加并使用对话模型</h3><p className="text-sm text-[var(--text-muted)]">选择厂商和模型，可复用已保存账户。保存后验证连接并启用，使用固定模型会解除默认对话模型池绑定。</p></div>
    <label className="block text-sm">服务商<select className={selectClass} value={provider} onChange={e => { const id=e.target.value as ProviderPresetId; setProvider(id); setAccountId(""); setKey(""); setCredentialModelId(""); setSavedModelId(null); setSavedVision(null); setUseVision(false); setLinkedVision(false); setVisionModel(id==="deepseek"?"deepseek-flash":PROVIDER_PRESETS[id].model); setBase(PROVIDER_PRESETS[id].baseUrl); setModel(PROVIDER_PRESETS[id].model); setLabel(PROVIDER_PRESETS[id].label); }}>
      {Object.entries(PROVIDER_PRESETS).map(([id,p]) => <option key={id} value={id}>{p.label}</option>)}
    </select></label>
    <label className="block text-sm">账户<select className={selectClass} value={accountId} onChange={e=>{setAccountId(e.target.value);setKey("");setCredentialModelId("");setLinkedVision(false);setSavedModelId(null);setSavedVision(null);}}><option value="">创建新账户</option>{accounts.filter(a=>a.provider_id===provider).map(a=><option key={a.id} value={a.id}>{a.label} · {a.credential_status === "configured" ? "已配置" : a.credential_status === "not_required" ? "无需凭据" : a.credential_status === "missing" ? "缺少凭据" : "凭据不可用"}</option>)}</select></label>
    {!accountId && <Input label="API 地址" value={base} onChange={e=>{setBase(e.target.value);setCredentialModelId("");}}/>}
    <AccountCredentialFields account={accounts.find(a=>a.id===accountId)} provider={provider} baseUrl={accounts.find(a=>a.id===accountId)?.base_url ?? base} models={models} apiKey={key} modelId={credentialModelId} onKey={setKey} onModel={setCredentialModelId}/>
    <Input label="模型 ID（可选择或输入）" list="quick-chat-models" value={model} onChange={e=>{setModel(e.target.value);setVisionModel(provider==="deepseek"?"deepseek-flash":e.target.value)}}/><datalist id="quick-chat-models">{(provider==="deepseek"?["deepseek-flash","deepseek-v4-pro"]:[PROVIDER_PRESETS[provider].model]).filter(Boolean).map(id=><option key={id} value={id}/>)}</datalist><Input label="显示名称" value={label} onChange={e=>setLabel(e.target.value)}/>
    {(PROVIDER_PRESETS[provider].services as string[]).includes("vision")&&<><label className="flex gap-2 text-sm"><input type="checkbox" checked={useVision} onChange={e=>setUseVision(e.target.checked)}/>同时使用此账户配置默认识图服务</label>{useVision&&<Input label="识图模型 ID" value={visionModel} readOnly={provider==="deepseek"} onChange={e=>setVisionModel(e.target.value)}/>}{linkedVision&&<p className="text-sm">识图服务已关联此账户并设为默认。</p>}</>}
    {savedModelId&&<p className="text-sm text-[var(--text-muted)]">本次模型档案已保存；再次提交会更新同一档案。</p>}
    <Button disabled={busy||!model.trim()||!label.trim()||(useVision&&!visionModel.trim())||(!accountId&&!base.trim())||!credentialReady(accounts.find(a=>a.id===accountId),provider,key,credentialModelId)} onClick={()=>void save()}>{busy?"正在保存并验证…":"保存并使用"}</Button>{message&&<p role="status" className="text-sm">{message}</p>}
  </section>;
}
