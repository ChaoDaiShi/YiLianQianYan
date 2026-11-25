import { useEffect, useMemo, useState } from "react";
import { ArrowLeft, Check, Plus, RefreshCw, Search } from "lucide-react";
import { Badge, Button, Input, Select } from "../../components/ui";
import SettingsDialogLayout from "../settings/sections/SettingsDialogLayout";
import AccountCredentialFields from "./AccountCredentialFields";
import { credentialReady } from "./accountCredentials";
import { PROVIDER_PRESETS, type ProviderPresetId } from "./llmModelUtils";
import type { UpstreamModel } from "./ModelCatalogField";
import { availablePurposes, catalogPurposeHints, modelPurposes, planModelChanges, type DraftModels, type ModelPurpose } from "./modelConfigurationState";
import { createModelPurpose, discoverProviderModels, removeModelPurpose, saveCatalogAccount, type ModelConfigurationData } from "./modelConfigurationApi";
import type { ProviderAccount } from "./modelPools";

const providerOptions = Object.entries(PROVIDER_PRESETS)
  .filter(([id]) => availablePurposes(id).length > 0)
  .map(([value, preset]) => ({ value, label: preset.label }));

interface Props {
  data: ModelConfigurationData;
  account?: ProviderAccount;
  focusedModel?: string;
  onBack: () => void;
  onClose: () => void;
  onRefresh: () => Promise<ModelConfigurationData>;
}

export default function ProviderCatalogEditor({ data, account, focusedModel, onBack, onClose, onRefresh }: Props) {
  const [accountId, setAccountId] = useState(account?.id);
  const [provider, setProvider] = useState(account?.provider_id ?? "openai");
  const [baseUrl, setBaseUrl] = useState(account?.base_url ?? PROVIDER_PRESETS.openai.baseUrl);
  const [accountLabel, setAccountLabel] = useState(account?.label ?? PROVIDER_PRESETS.openai.label);
  const [apiKey, setApiKey] = useState("");
  const [credentialModelId, setCredentialModelId] = useState("");
  const [query, setQuery] = useState(focusedModel ?? "");
  const [manualId, setManualId] = useState("");
  const [draft, setDraft] = useState<DraftModels>({});
  const [manualModels, setManualModels] = useState<UpstreamModel[]>([]);
  const [catalog, setCatalog] = useState<{ connection: object; models: UpstreamModel[]; error?: string }>();
  const [refresh, setRefresh] = useState(0);
  const [saving, setSaving] = useState(false);
  const [saveError, setSaveError] = useState("");
  const [savedAccount, setSavedAccount] = useState<ProviderAccount>();
  const [needsRefresh, setNeedsRefresh] = useState(false);
  const existingAccount = data.accounts.find(item => item.id === accountId) ?? savedAccount;
  const existingRows = data.rows.filter(row => accountId && row.account?.id === accountId);
  const changes = planModelChanges(draft, existingRows);
  const normalizedBase = baseUrl.trim().replace(/\/+$/, "");
  const ready = Boolean(normalizedBase) && credentialReady(existingAccount, provider, apiKey, credentialModelId);
  const connection = useMemo(() => ({ provider, normalizedBase, apiKey, accountId, credentialModelId, refresh }), [provider, normalizedBase, apiKey, accountId, credentialModelId, refresh]);

  useEffect(() => {
    if (!ready) return;
    let current = true;
    const timer = window.setTimeout(() => {
      const { provider, normalizedBase, apiKey, accountId, credentialModelId } = connection;
      void discoverProviderModels(provider, normalizedBase, apiKey, credentialModelId ? undefined : accountId, credentialModelId || undefined)
        .then(({ models }) => { if (current) setCatalog({ connection, models }); })
        .catch(error => { if (current) setCatalog({ connection, models: [], error: (error as Error).message }); });
    }, 650);
    return () => { current = false; window.clearTimeout(timer); };
  }, [connection, ready]);

  const currentCatalog = catalog?.connection === connection ? catalog : undefined;
  const models = new Map((currentCatalog?.models ?? []).map(model => [model.id, model]));
  for (const row of existingRows) if (!models.has(row.model)) models.set(row.model, { id: row.model, name: row.name });
  for (const model of manualModels) if (!models.has(model.id)) models.set(model.id, model);
  for (const [id, row] of Object.entries(draft)) if (!models.has(id)) models.set(id, { id, name: row.name });
  const search = query.trim().toLowerCase();
  const visible = [...models.values()].filter(model => `${model.id} ${model.name}`.toLowerCase().includes(search));
  const purposes = availablePurposes(provider);
  const hasCatalogHints = visible.some(model => catalogPurposeHints(model).some(purpose => purposes.includes(purpose)));
  const credentialChanged = Boolean(apiKey.trim() || credentialModelId);
  const labelChanged = Boolean(existingAccount && existingAccount.label !== accountLabel.trim());
  const accountChanged = !accountId || credentialChanged || labelChanged;
  const canSave = ready && accountLabel.trim() && (changes.length > 0 || accountChanged || needsRefresh);

  function changeProvider(value: ProviderPresetId) {
    const preset = PROVIDER_PRESETS[value];
    setProvider(value);
    setBaseUrl(preset.baseUrl);
    setAccountLabel(preset.label);
    setApiKey("");
    setCredentialModelId("");
    setDraft({});
    setManualModels([]);
    setQuery("");
  }

  function selectPurpose(model: UpstreamModel, purpose: ModelPurpose, checked: boolean) {
    const saved = existingRows.find(row => row.model === model.id);
    setDraft(current => {
      const selected = current[model.id]?.purposes ?? Object.keys(saved?.profiles ?? {}) as ModelPurpose[];
      return { ...current, [model.id]: { name: model.name, purposes: checked ? [...selected, purpose] : selected.filter(item => item !== purpose) } };
    });
  }

  function applyVisibleCatalogHints() {
    setDraft(current => {
      const next = { ...current };
      for (const model of visible) {
        const hints = catalogPurposeHints(model).filter(purpose => purposes.includes(purpose));
        if (hints.length === 0) continue;
        const saved = existingRows.find(row => row.model === model.id);
        const selected = current[model.id]?.purposes ?? Object.keys(saved?.profiles ?? {}) as ModelPurpose[];
        next[model.id] = { name: model.name, purposes: [...new Set([...selected, ...hints])] };
      }
      return next;
    });
  }

  function addManualModel() {
    const id = manualId.trim();
    if (!id) return;
    if (!models.has(id)) setManualModels(items => [...items, { id, name: id }]);
    setQuery(id);
    setManualId("");
  }

  async function save() {
    setSaving(true);
    setSaveError("");
    let saved = existingAccount;
    let applied = 0;
    try {
      const current = needsRefresh ? await onRefresh() : data;
      saved = current.accounts.find(item => item.id === accountId) ?? saved;
      const currentChanges = planModelChanges(draft, current.rows.filter(row => accountId && row.account?.id === accountId));
      if (!saved || credentialChanged || saved.label !== accountLabel.trim()) {
        saved = await saveCatalogAccount(provider, saved?.base_url ?? normalizedBase, accountLabel.trim(), apiKey, credentialModelId, saved);
        setSavedAccount(saved);
        setAccountId(saved.id);
        setApiKey("");
        setCredentialModelId("");
        applied++;
      }
      if (!saved) return;
      for (const { model, name, add, remove } of currentChanges) {
        for (const purpose of add) {
          await createModelPurpose(saved, model, name, purpose, models.get(model)?.max_output_tokens);
          applied++;
        }
        for (const { purpose, id } of remove) {
          await removeModelPurpose(purpose, id);
          applied++;
        }
      }
      await onRefresh();
      onBack();
    } catch (error) {
      if (applied) {
        try { await onRefresh(); setNeedsRefresh(false); } catch { setNeedsRefresh(true); }
      }
      setSaveError(`${applied ? "部分变更已保存。" : "保存失败。"}${(error as Error).message}`);
    } finally {
      setSaving(false);
    }
  }

  return <SettingsDialogLayout title={existingAccount ? `配置模型 · ${existingAccount.label}` : "添加厂商账户"} size="xl"
    onClose={saving ? () => {} : onClose} closeDisabled={saving} error={saveError}
    secondaryActions={<Button variant="ghost" onClick={onBack} disabled={saving}><ArrowLeft size={15} />返回已配置模型</Button>}
    notice={changes.length ? `${changes.length} 个模型的用途待保存` : undefined}
    action={<Button disabled={!canSave || saving} loading={saving} loadingLabel="正在保存" onClick={() => void save()}><Check size={15} />保存配置</Button>}>
    <fieldset disabled={saving} className="model-config-editor">
      <section className="model-config-connection" aria-label="厂商连接">
        <h4>厂商连接</h4>
        <div className="model-config-fields">
          {!accountId && <Select label="厂商" value={provider} options={providerOptions} onChange={event => changeProvider(event.target.value as ProviderPresetId)} />}
          <Input label="账户名称" value={accountLabel} onChange={event => setAccountLabel(event.target.value)} />
          <Input label="API 地址" value={baseUrl} readOnly={Boolean(accountId)} onChange={event => { setBaseUrl(event.target.value); setCredentialModelId(""); setDraft({}); setManualModels([]); }} spellCheck={false} />
        </div>
        <AccountCredentialFields account={existingAccount} provider={provider} baseUrl={normalizedBase} models={existingAccount?.credential_configured ? [] : data.chats}
          apiKey={apiKey} modelId={credentialModelId} onKey={setApiKey} onModel={setCredentialModelId} />
      </section>
      <section className="model-config-catalog" aria-labelledby="model-config-catalog-title">
        <div className="model-config-section-heading">
          <div><h4 id="model-config-catalog-title">模型目录</h4><p>为一个模型勾选多个用途。保存后可分别测试和设为默认。</p></div>
          <Button variant="secondary" size="sm" disabled={!ready || !currentCatalog} onClick={() => setRefresh(value => value + 1)}><RefreshCw size={14} />刷新目录</Button>
        </div>
        {!ready ? <p className="model-config-hint">填写 API 地址和凭据后自动读取目录。</p> : !currentCatalog ? <p role="status" className="model-config-hint">正在读取厂商模型目录…</p>
          : currentCatalog.error ? <p role="alert" className="model-config-hint">{currentCatalog.error}</p>
          : <p role="status" className="model-config-hint">{currentCatalog.models.length ? `厂商返回 ${currentCatalog.models.length} 个模型。` : "厂商没有返回可用模型。"}</p>}
        {models.size > 0 && <div className="model-config-search"><Search size={15} aria-hidden="true" /><Input aria-label="搜索上游模型" value={query} onChange={event => setQuery(event.target.value)} placeholder="搜索模型名称或 ID" />
          {hasCatalogHints && <Button type="button" size="sm" variant="secondary" onClick={applyVisibleCatalogHints}>勾选搜索结果中厂商标注的用途</Button>}
        </div>}
        {visible.length > 0 && <ul className="model-config-model-list">
          {visible.map(model => {
            const saved = existingRows.find(row => row.model === model.id);
            const selected = draft[model.id]?.purposes ?? Object.keys(saved?.profiles ?? {});
            const hints = catalogPurposeHints(model).filter(purpose => purposes.includes(purpose));
            return <li key={model.id} className="model-config-model-row">
              <div className="model-config-row-title"><strong>{model.name}</strong>{saved && <Badge tone="success">已配置</Badge>}</div>
              {model.id !== model.name && <p className="model-config-model-id">{model.id}</p>}
              <p className="model-config-hint">{hints.length ? `厂商目录标注支持 ${hints.map(purpose => modelPurposes[purpose]).join("、")}` : "厂商目录未提供可识别的用途信息"}</p>
              <div className="model-config-purpose-options" role="group" aria-label={`${model.id} 的用途`}>
                {Array.from(new Set<ModelPurpose>([...purposes, ...Object.keys(saved?.profiles ?? {}) as ModelPurpose[]])).map(purpose => {
                  const profile = saved?.profiles[purpose];
                  const isDefault = Boolean(profile && (data.defaults[purpose].id === profile.id || ("active" in profile && profile.active)));
                  return <label key={purpose} className="model-config-purpose-option"><input type="checkbox" checked={selected.includes(purpose)} disabled={isDefault}
                    onChange={event => selectPurpose(model, purpose, event.target.checked)} />{modelPurposes[purpose]}{isDefault ? "（默认）" : ""}</label>;
                })}
              </div>
            </li>;
          })}
        </ul>}
        {models.size > 0 && visible.length === 0 && <p className="model-config-hint">没有匹配的模型。</p>}
        {ready && <details className="model-config-manual"><summary>手动填写模型 ID</summary><div className="model-config-inline-action">
          <Input label="模型 ID" value={manualId} onChange={event => setManualId(event.target.value)} placeholder="厂商目录未列出模型时使用" />
          <Button variant="secondary" onClick={addManualModel} disabled={!manualId.trim()}><Plus size={15} />加入目录</Button>
        </div></details>}
      </section>
    </fieldset>
  </SettingsDialogLayout>;
}
