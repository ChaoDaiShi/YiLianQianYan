import { useEffect, useState } from "react";
import { ArrowLeft, Box, ChevronRight, Database, Image, MessageSquare, Mic, Plus, Settings2, Volume2 } from "lucide-react";
import { Badge, Button } from "../../components/ui";
import type { AppConfig } from "../../types";
import SettingsDialogLayout from "../settings/sections/SettingsDialogLayout";
import { credentialLabels } from "./accountCredentials";
import { providerLabel } from "./llmModelUtils";
import { modelPurposes, type ConfiguredModel, type ModelPurpose } from "./modelConfigurationState";
import { loadModelConfiguration, useAsDefault, verifyConfiguredModel, type ModelConfigurationData } from "./modelConfigurationApi";
import ProviderCatalogEditor from "./ProviderCatalogEditor";
import type { ProviderAccount } from "./modelPools";
import "./modelConfiguration.css";

const purposeIcons = { chat: MessageSquare, vision: Image, image: Image, stt: Mic, tts: Volume2, embedding: Database };
const purposes = Object.keys(modelPurposes) as ModelPurpose[];
type View = { kind: "list" } | { kind: "catalog"; account?: ProviderAccount; focusedModel?: string } | { kind: "model"; rowKey: string };

function defaultDescription(data: ModelConfigurationData, purpose: ModelPurpose) {
  const source = data.defaults[purpose];
  if (source.source === "pool" && source.id) return { title: data.pools.find(pool => pool.id === source.id)?.label ?? "模型池不可用", subtitle: "高级模型池" };
  if (source.id) {
    const row = data.rows.find(model => model.profiles[purpose]?.id === source.id);
    if (row) return { title: row.model, subtitle: row.account?.label ?? providerLabel(row.provider), rowKey: row.key };
  }
  const config = data.config;
  if (purpose === "chat" && (config.provider_readiness?.model.configured ?? config.model.api_key_configured)) return { title: config.model.name, subtitle: "基础对话配置" };
  if (purpose === "embedding" && config.model.embedding_model) return { title: config.model.embedding_model, subtitle: "向量配置" };
  if ((purpose === "stt" || purpose === "tts") && config.provider_readiness?.[purpose].configured) return { title: config.voice[purpose].model, subtitle: "语音配置" };
  return { title: "未配置" };
}

function ModelRows({ rows, defaults, onOpen }: { rows: ConfiguredModel[]; defaults: ModelConfigurationData["defaults"]; onOpen: (rowKey: string) => void }) {
  return <ul className="model-config-saved-list">
    {rows.map(row => <li key={row.key}>
      <button type="button" className="model-config-saved-row focus-ring-token" onClick={() => onOpen(row.key)}>
        <div className="model-config-saved-identity"><strong>{row.name}</strong>{row.name !== row.model && <span>{row.model}</span>}</div>
        <div className="model-config-tags">{purposes.filter(purpose => row.profiles[purpose]).map(purpose => <Badge key={purpose} tone={defaults[purpose].id === row.profiles[purpose]?.id ? "success" : "default"}>{modelPurposes[purpose]}{defaults[purpose].id === row.profiles[purpose]?.id ? " · 默认" : ""}</Badge>)}</div>
        <ChevronRight size={16} aria-hidden="true" />
      </button>
    </li>)}
  </ul>;
}

function ModelDetail({ data, row, onBack, onClose, onRefresh, onEdit, onLegacy }: {
  data: ModelConfigurationData; row: ConfiguredModel; onBack: () => void; onClose: () => void;
  onRefresh: () => Promise<ModelConfigurationData>; onEdit: (account: ProviderAccount, model: string) => void; onLegacy: () => void;
}) {
  const [busy, setBusy] = useState<ModelPurpose>();
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const [image, setImage] = useState<string>();

  async function operate(purpose: ModelPurpose, action: () => Promise<unknown>, success: string) {
    setBusy(purpose);
    setError("");
    setNotice("");
    try {
      await action();
      setNotice(success);
    } catch (cause) {
      setError((cause as Error).message);
    }
    try { await onRefresh(); } catch (cause) { setError((cause as Error).message); }
    setBusy(undefined);
  }

  function attachImage(file: File) {
    setImage(undefined);
    const reader = new FileReader();
    reader.onload = () => setImage(String(reader.result));
    reader.onerror = () => setError("无法读取测试图片");
    reader.readAsDataURL(file);
  }

  return <SettingsDialogLayout title="模型详情" size="lg" onClose={busy ? () => {} : onClose} closeDisabled={Boolean(busy)} error={error} notice={notice}
    secondaryActions={<Button variant="ghost" disabled={Boolean(busy)} onClick={onBack}><ArrowLeft size={15} />返回已配置模型</Button>}
    action={<Button disabled={Boolean(busy)} onClick={() => row.account ? onEdit(row.account, row.model) : onLegacy()}><Settings2 size={15} />{row.account ? "编辑用途" : "管理独立配置"}</Button>}>
    <div className="model-config-detail-header"><strong>{row.name}</strong><p>{row.model}</p><p>{row.account?.label ?? providerLabel(row.provider)} · {row.baseUrl}</p></div>
    <ul className="model-config-detail-list">
      {purposes.map(purpose => {
        const profile = row.profiles[purpose];
        if (!profile) return null;
        const isDefault = data.defaults[purpose].id === profile.id;
        const profileVerified = "verified_at" in profile ? Boolean(profile.verified_at) : "test" in profile ? profile.test?.ok : undefined;
        const failed = "last_error" in profile ? Boolean(profile.last_error) : "test" in profile && profile.test?.ok === false;
        const canVerify = purpose === "chat" || purpose === "image" || purpose === "vision" || isDefault;
        const Icon = purposeIcons[purpose];
        return <li key={purpose} className="model-config-purpose-detail">
          <div className="model-config-section-heading">
            <div className="model-config-tags"><Icon size={16} aria-hidden="true" /><strong>{modelPurposes[purpose]}</strong>
              {isDefault && <Badge tone="success">默认</Badge>}
              {profileVerified ? <Badge tone="info">已验证</Badge> : failed ? <Badge tone="warning">测试失败</Badge> : <Badge>待验证</Badge>}
            </div>
            <div className="model-config-tags">
              {canVerify && <Button size="sm" variant="secondary" disabled={Boolean(busy) || (purpose === "vision" && !image)}
                onClick={() => void operate(purpose, () => verifyConfiguredModel(purpose, profile.id, image), `${modelPurposes[purpose]}连接测试成功`)}>{busy === purpose ? "正在处理…" : "测试连接"}</Button>}
              {!isDefault && <Button size="sm" variant="secondary" disabled={Boolean(busy) || (purpose === "chat" && !profileVerified)}
                onClick={() => void operate(purpose, () => useAsDefault(purpose, profile.id, data.defaults[purpose]), `已设为默认${modelPurposes[purpose]}模型`)}>设为默认</Button>}
            </div>
          </div>
          {purpose === "vision" && <label className="model-config-test-image">选择测试图片 <input type="file" accept="image/*" disabled={Boolean(busy)} onChange={event => {
            const file = event.target.files?.[0];
            if (file) attachImage(file);
          }} /></label>}
          {purpose === "chat" && !profileVerified && !isDefault && <p className="model-config-hint">通过连接测试后可设为默认。</p>}
          {!canVerify && <p className="model-config-hint">设为默认后可测试此用途。</p>}
        </li>;
      })}
    </ul>
  </SettingsDialogLayout>;
}

export default function ModelConfiguration({ onClose, onLegacy, onChanged }: { onClose: () => void; onLegacy: () => void; onChanged?: (config: AppConfig) => void }) {
  const [data, setData] = useState<ModelConfigurationData>();
  const [view, setView] = useState<View>({ kind: "list" });
  const [error, setError] = useState("");
  const [revision, setRevision] = useState(0);

  useEffect(() => {
    let active = true;
    void loadModelConfiguration().then(result => { if (active) setData(result); })
      .catch(cause => { if (active) setError((cause as Error).message); });
    return () => { active = false; };
  }, [revision]);

  const refresh = async () => {
    const updated = await loadModelConfiguration();
    setData(updated);
    onChanged?.(updated.config);
    return updated;
  };
  const back = () => setView({ kind: "list" });
  const open = (rowKey: string) => setView({ kind: "model", rowKey });

  if (data && view.kind === "catalog") return <ProviderCatalogEditor data={data} account={view.account} focusedModel={view.focusedModel}
    onBack={back} onClose={onClose} onRefresh={refresh} />;
  if (data && view.kind === "model") {
    const row = data.rows.find(model => model.key === view.rowKey);
    if (row) return <ModelDetail data={data} row={row} onBack={back} onClose={onClose} onRefresh={refresh} onLegacy={onLegacy}
      onEdit={(account, focusedModel) => setView({ kind: "catalog", account, focusedModel })} />;
  }

  const independent = data?.rows.filter(row => !row.account) ?? [];
  return <SettingsDialogLayout title="模型配置" size="xl" onClose={onClose} error={error}
    secondaryActions={<Button variant="ghost" onClick={onLegacy}>基础配置与用量</Button>}
    action={data ? <Button onClick={() => setView({ kind: "catalog" })}><Plus size={16} />添加厂商账户</Button> : undefined}>
    {!data ? <div className="model-config-loading">{error ? <Button variant="secondary" onClick={() => { setError(""); setRevision(value => value + 1); }}>重试</Button>
      : <p role="status" className="model-config-hint">正在读取已配置模型…</p>}</div> : <div className="model-config-overview">
      <section aria-labelledby="model-config-defaults-title">
        <div className="model-config-section-heading"><h4 id="model-config-defaults-title">当前默认用途</h4></div>
        <p className="model-config-hint">每个用途可选独立默认模型。支持多个用途的模型可同时设为这些用途的默认。</p>
        <div className="model-config-defaults">{purposes.map(purpose => {
          const Icon = purposeIcons[purpose];
          const { title, subtitle, rowKey } = defaultDescription(data, purpose);
          const contents = <><span className="model-config-default-label"><Icon size={14} aria-hidden="true" />{modelPurposes[purpose]}</span><strong>{title}</strong>{subtitle && <small>{subtitle}</small>}</>;
          return rowKey ? <button type="button" key={purpose} className="model-config-default focus-ring-token" onClick={() => open(rowKey)}>{contents}</button>
            : <div key={purpose} className="model-config-default">{contents}</div>;
        })}</div>
      </section>
      <section aria-labelledby="model-config-accounts-title">
        <div className="model-config-section-heading"><div><h4 id="model-config-accounts-title">已配置模型</h4><p>{data.rows.length} 个模型 · {data.accounts.length} 个厂商账户</p></div></div>
        {!data.accounts.length && !independent.length && <div className="model-config-empty"><Box size={22} aria-hidden="true" /><p>还没有模型档案。添加厂商账户后可从目录选择模型。</p></div>}
        {data.accounts.map(account => {
          const rows = data.rows.filter(row => row.account?.id === account.id);
          return <div key={account.id} className="model-config-account">
            <div className="model-config-account-header">
              <div><div className="model-config-tags"><strong>{account.label}</strong>{account.credential_status && <Badge tone={account.credential_status === "configured" ? "success" : "default"}>{credentialLabels[account.credential_status]}</Badge>}</div>
                <p>{providerLabel(account.provider_id)} · {account.base_url}</p></div>
              <Button variant="secondary" size="sm" onClick={() => setView({ kind: "catalog", account })}><Settings2 size={14} />配置模型</Button>
            </div>
            {rows.length ? <ModelRows rows={rows} defaults={data.defaults} onOpen={open} /> : <p className="model-config-account-empty">此账户还没有配置模型。</p>}
          </div>;
        })}
        {independent.length > 0 && <div className="model-config-account"><div className="model-config-account-header"><strong>独立配置</strong></div>
          <ModelRows rows={independent} defaults={data.defaults} onOpen={open} /></div>}
      </section>
    </div>}
  </SettingsDialogLayout>;
}
