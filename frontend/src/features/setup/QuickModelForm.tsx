import { useState } from "react";
import { CheckCircle2, Eye, EyeOff } from "lucide-react";
import { Button, Input } from "../../components/ui";
import { PROVIDER_PRESETS, type ProviderPresetId } from "../llm/llmModelUtils";
import { CONNECTION_LABELS, type useQuickModelSetup } from "./useQuickModelSetup";

const QUICK_PROVIDERS = Object.values(PROVIDER_PRESETS).filter(preset => preset.services.includes("chat") && preset.category !== "local");

export default function QuickModelForm({ setup, onConnected }: {
  setup: ReturnType<typeof useQuickModelSetup>;
  onConnected: () => void;
}) {
  const [showKey, setShowKey] = useState(false);
  const { draft, profile, loading, loadError, stage, ready, message, changeField, changeProvider } = setup;
  const busy = stage !== null;
  const currentIsLocal = Object.values(PROVIDER_PRESETS).some(preset => preset.id === draft.provider && preset.category === "local");
  return <form className="quick-model-form" onSubmit={event => {
    event.preventDefault();
    void setup.connect().then(connected => { if (connected) onConnected(); });
  }}>
    {loading ? <p role="status">正在读取模型配置…</p> : loadError ? <div role="alert" className="quick-setup-error"><p>{loadError}</p><Button variant="secondary" onClick={() => void setup.load()}>重新加载配置</Button></div> : <>
      {ready && <p className="quick-setup-connected"><CheckCircle2 size={18} />当前模型连接可用，可以直接继续。</p>}
      {currentIsLocal && <p className="quick-setup-note">已保留当前本地模型配置。免密本地连接请通过“稍后配置”进入工作台后，在模型设置中管理。</p>}
      <fieldset disabled={busy}>
        <label className="quick-model-provider"><span>模型服务商</span><select aria-label="模型服务商" value={draft.provider} onChange={event => changeProvider(event.target.value as ProviderPresetId)}>
          {!QUICK_PROVIDERS.some(preset => preset.id === draft.provider) && <option value={draft.provider}>{draft.provider}</option>}
          {QUICK_PROVIDERS.map(preset => <option key={preset.id} value={preset.id}>{preset.label}</option>)}
        </select></label>
        <div className="quick-model-fields">
          <Input label="模型名称" value={draft.name} onChange={event => changeField("name", event.target.value)} placeholder="填写服务商支持的模型 ID" required autoComplete="off" />
          <Input label="服务地址" type="url" value={draft.base_url} onChange={event => changeField("base_url", event.target.value)} required autoComplete="off" spellCheck={false} />
        </div>
        <div className="quick-model-key"><Input label="API Key" type={showKey ? "text" : "password"} value={draft.api_key} onChange={event => changeField("api_key", event.target.value)} autoComplete="off" spellCheck={false} placeholder={profile?.api_key_configured ? "已保存；留空保留现有密钥" : "粘贴你的 API Key"} hint={profile?.api_key_configured ? "密钥已保存在本机，无需重复填写。" : "密钥交由现有模型服务保存到本机，请按服务商要求填写。"} />
          <Button variant="ghost" iconOnly aria-label={showKey ? "隐藏 API Key" : "显示 API Key"} onClick={() => setShowKey(value => !value)}>{showKey ? <EyeOff size={17} /> : <Eye size={17} />}</Button>
        </div>
      </fieldset>
      <p className="quick-setup-note">快速配置支持现有 OpenAI-compatible 模型接口。模型名称和地址均可修改，本地模型与高级参数可在设置中配置。</p>
      {message && <p className="quick-setup-error" role="alert">{message}</p>}
      {busy && <p className="quick-setup-progress" role="status">{CONNECTION_LABELS[stage]}</p>}
      {ready ? <Button size="lg" onClick={onConnected}>继续使用当前模型<CheckCircle2 size={17} /></Button> : <Button type="submit" size="lg" loading={busy} loadingLabel={stage ? CONNECTION_LABELS[stage] : undefined} disabled={busy || !draft.name.trim() || !draft.base_url.trim()}>保存并测试连接</Button>}
    </>}
  </form>;
}
