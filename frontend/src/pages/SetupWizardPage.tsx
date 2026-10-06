import { useEffect, useMemo, useState } from "react";
import { Check, ChevronLeft, ChevronRight, Cloud, Mic, Search, Sparkles, UserCircle } from "lucide-react";
import { useNavigate } from "react-router-dom";
import { getCloudIdentity, getSearchConfig, updateSearchConfig } from "../api/cloud";
import { getProviderReadiness, verifyProviderConnection } from "../api/providerConnection";
import { getSettings, updateSettings } from "../api/client";
import { Badge, Button, Input } from "../components/ui";
import { useProductPreferences } from "../components/system/useProductPreferences";
import { defaultConfig } from "../features/settings/model/config";
import { wizardCanFinish } from "../features/cloud/cloudPresentation";
import type { ProviderReadinessProjection } from "../types";

const STEPS = [
  { id: "identity", label: "身份", icon: UserCircle },
  { id: "model", label: "模型", icon: Sparkles },
  { id: "search", label: "搜索", icon: Search },
  { id: "voice", label: "语音", icon: Mic },
  { id: "complete", label: "完成", icon: Check },
] as const;

export default function SetupWizardPage() {
  const navigate = useNavigate();
  const { preferences, loaded, save } = useProductPreferences();
  const [step, setStep] = useState(0);
  const [readiness, setReadiness] = useState<ProviderReadinessProjection | null>(null);
  const [identityStatus, setIdentityStatus] = useState("guest");
  const [searchConfigured, setSearchConfigured] = useState(false);
  const [model, setModel] = useState({ provider: "openai", base_url: "https://api.deepseek.com/v1", name: "deepseek-chat", api_key: "" });
  const [searchConfig, setSearchConfig] = useState({ base_url: "https://api.anysearch.com", api_key: "" });
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState("");

  const refresh = async () => {
    const [nextReadiness, identity, search] = await Promise.all([getProviderReadiness(), getCloudIdentity(), getSearchConfig()]);
    setReadiness(nextReadiness);
    if (identity) setIdentityStatus(identity.identity.status);
    if (search) { setSearchConfigured(search.configured); setSearchConfig((current) => ({ ...current, base_url: search.base_url })); }
  };
  useEffect(() => { void refresh(); }, []);

  const ready = useMemo(() => ({
    identity: identityStatus !== "guest",
    model: Boolean(readiness?.model.available),
    search: searchConfigured,
    stt: Boolean(readiness?.stt.available),
    tts: Boolean(readiness?.tts.available),
  }), [identityStatus, readiness, searchConfigured]);

  const saveModel = async () => {
    setBusy(true); setMessage("");
    const current = await getSettings();
    const config = current ? { ...defaultConfig, ...current, model: { ...defaultConfig.model, ...current.model } } : defaultConfig;
    const result = await updateSettings({ ...config, model: { ...config.model, ...model, api_key_env: "" } });
    if (!result || result.error) setMessage("模型配置未能保存。");
    else {
      const verified = await verifyProviderConnection("model");
      setMessage(verified.ok ? "模型连接正常。" : verified.error);
    }
    await refresh(); setBusy(false);
  };

  const saveSearch = async () => {
    setBusy(true); setMessage("");
    const result = await updateSearchConfig({ base_url: searchConfig.base_url, api_key: searchConfig.api_key || undefined });
    if (result.ok) { setSearchConfigured(result.data.configured); setSearchConfig((current) => ({ ...current, api_key: "" })); setMessage(result.data.configured ? "搜索密钥已保存到本机。" : "将使用匿名搜索额度。"); }
    else setMessage(result.error);
    setBusy(false);
  };

  const finish = async () => {
    if (!loaded || !wizardCanFinish(ready)) return;
    setBusy(true);
    await save({ ...preferences, setup_completed: true });
    window.sessionStorage.removeItem("yilian.setup.deferred");
    navigate("/chat");
  };
  const defer = () => { window.sessionStorage.setItem("yilian.setup.deferred", "true"); navigate("/chat"); };
  const current = STEPS[step];

  return <div className="setup-wizard-page page-canvas">
    <aside className="setup-wizard-steps" aria-label="首次配置步骤"><div className="setup-brand"><img src="/favicon.png" alt="" /><div><strong>忆涟千言</strong><span>v1.1 首次配置</span></div></div>{STEPS.map((item, index) => { const Icon = item.icon; return <button key={item.id} type="button" aria-current={step === index ? "step" : undefined} onClick={() => setStep(index)}><span><Icon /></span><div><strong>{item.label}</strong><small>{index < step || (item.id === "model" && ready.model) ? "已完成" : index === step ? "当前步骤" : item.id === "identity" || item.id === "search" || item.id === "voice" ? "可跳过" : "待完成"}</small></div></button>; })}<p>高级 URL、采样、Embedding 与语音参数仍可在设置页调整。</p></aside>
    <main className="setup-wizard-main"><div className="setup-wizard-card"><header><span>STEP {step + 1} / {STEPS.length}</span><h1>{current.label}</h1></header>
      {current.id === "identity" ? <section><div className="setup-icon"><Cloud /></div><h2>{ready.identity ? "账号已连接" : "先以本地模式开始"}</h2><p>账号只用于同步与跨设备恢复。暂不登录不会限制本地 Agent、任务、画布、MCP、Voice 或资料。</p><div className="setup-choice-row"><button type="button" onClick={() => setStep(step + 1)}><strong>继续使用本地身份</strong><span>以后可在账号中心登录</span></button><button type="button" onClick={() => navigate("/account")}><strong>登录或注册</strong><span>进入账号中心安全连接</span></button></div></section> : null}
      {current.id === "model" ? <section><div className="setup-status-line"><Badge tone={ready.model ? "success" : "warning"}>{ready.model ? "连接正常" : "需要配置"}</Badge><span>文字智能功能需要一个可用模型。</span></div><div className="setup-form-grid"><Input label="Provider" value={model.provider} onChange={(event) => setModel({ ...model, provider: event.target.value })} /><Input label="模型" value={model.name} onChange={(event) => setModel({ ...model, name: event.target.value })} /><Input label="Base URL" value={model.base_url} onChange={(event) => setModel({ ...model, base_url: event.target.value })} /><Input label="API Key" type="password" value={model.api_key} onChange={(event) => setModel({ ...model, api_key: event.target.value })} /></div><Button disabled={busy || !model.name || !model.base_url} onClick={() => void saveModel()}>{busy ? "正在验证…" : "保存并测试连接"}</Button></section> : null}
      {current.id === "search" ? <section><div className="setup-status-line"><Badge tone={ready.search ? "success" : "default"}>{ready.search ? "本机 BYOK" : "可使用匿名模式"}</Badge><span>AnySearch 为首个 Provider，API Key 可跳过。</span></div><div className="setup-form-grid"><Input label="API 地址" value={searchConfig.base_url} onChange={(event) => setSearchConfig({ ...searchConfig, base_url: event.target.value })} /><Input label="API Key（可选）" type="password" value={searchConfig.api_key} onChange={(event) => setSearchConfig({ ...searchConfig, api_key: event.target.value })} /></div><Button disabled={busy} onClick={() => void saveSearch()}>{busy ? "保存中…" : "保存搜索配置"}</Button></section> : null}
      {current.id === "voice" ? <section><div className="setup-icon"><Mic /></div><h2>语音能力可以稍后配置</h2><p>STT 和 TTS 都是可选能力，不会阻止进入工作台。当前状态：STT {ready.stt ? "可用" : "未配置"}，TTS {ready.tts ? "可用" : "未配置"}。</p><Button variant="secondary" onClick={() => navigate("/settings?section=voice")}>前往语音设置</Button></section> : null}
      {current.id === "complete" ? <section><div className="setup-icon setup-icon--success"><Check /></div><h2>{ready.model ? "可以开始使用" : "还需要可用的模型连接"}</h2><p>身份：{ready.identity ? "已登录" : "仅本地"} · 搜索：{ready.search ? "本机 BYOK" : "匿名/稍后"} · 语音：{ready.stt || ready.tts ? "部分可用" : "稍后配置"}</p><Button disabled={busy || !wizardCanFinish(ready)} onClick={() => void finish()}>完成并进入工作台</Button></section> : null}
      {message ? <p className="setup-message" role="status">{message}</p> : null}
      <footer><Button variant="ghost" disabled={step === 0 || busy} onClick={() => setStep((value) => value - 1)}><ChevronLeft />上一步</Button><button type="button" className="setup-defer" onClick={defer}>稍后完成</button><Button variant="secondary" disabled={step === STEPS.length - 1 || busy} onClick={() => setStep((value) => value + 1)}>下一步<ChevronRight /></Button></footer>
    </div></main>
  </div>;
}
