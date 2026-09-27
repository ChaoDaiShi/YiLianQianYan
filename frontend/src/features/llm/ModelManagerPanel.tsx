import { useEffect, useMemo, useRef, useState } from "react";
import { CheckCircle2, CircleAlert, Pencil, Plus, RefreshCw, Trash2, Zap } from "lucide-react";
import {
  activateLlmModel,
  createLlmModel,
  deleteLlmModel,
  getLlmUsage,
  listLlmModels,
  updateLlmModel,
  verifyLlmModel,
} from "../../api/client";
import type { AppConfig, LlmModel, LlmModelPayload, LlmUsageReport } from "../../types";
import { Badge, Button, Input } from "../../components/ui";
import ModelPresetSelect from "./ModelPresetSelect";
import ModelTree from "./ModelTree";
import TokenUsageChart from "./TokenUsageChart";
import { buildUsageQuery, providerLabel, PROVIDER_PRESETS, type ProviderPresetId } from "./llmModelUtils";

function dateString(date: Date) {
  return date.toISOString().slice(0, 10);
}

function emptyForm(): LlmModelPayload {
  const preset = PROVIDER_PRESETS.deepseek;
  return {
    provider: "deepseek",
    label: preset.label,
    model: preset.model,
    base_url: preset.baseUrl,
    api_format: "openai",
    api_key: "",
    api_key_env: "",
    temperature: 0,
    max_tokens: 16_384,
    invoke_timeout_ms: 120_000,
  };
}

interface LegacyModelSettingsProps {
  config: AppConfig;
  updateField: (section: keyof AppConfig, key: string, value: unknown) => void;
  clearSecret: (field: "api_key" | "embedding_api_key") => Promise<void>;
  secretSourceLabel: (source?: string) => string;
  secretRefreshToken: number;
}

interface ModelManagerPanelProps {
  legacyModel: LegacyModelSettingsProps;
  actions?: React.ReactNode;
}

export default function ModelManagerPanel({ legacyModel, actions }: ModelManagerPanelProps) {
  const advancedRef = useRef<HTMLDetailsElement>(null);
  const [models, setModels] = useState<LlmModel[]>([]);
  const [form, setForm] = useState<LlmModelPayload>(emptyForm);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [feedback, setFeedback] = useState("");
  const [busyId, setBusyId] = useState<string | null>(null);
  const [from, setFrom] = useState(() => dateString(new Date(Date.now() - 29 * 86_400_000)));
  const [to, setTo] = useState(() => dateString(new Date()));
  const [usageModelId, setUsageModelId] = useState("");
  const [usage, setUsage] = useState<LlmUsageReport | null>(null);
  const [replaceApiKey, setReplaceApiKey] = useState(false);
  const [replaceEmbeddingApiKey, setReplaceEmbeddingApiKey] = useState(false);
  const [replaceProfileApiKey, setReplaceProfileApiKey] = useState(false);

  const refreshModels = async () => {
    const result = await listLlmModels();
    if (result) setModels(result);
  };

  const refreshUsage = async () => {
    const result = await getLlmUsage(buildUsageQuery(usageModelId, from, to));
    if (result) setUsage(result);
  };

  useEffect(() => {
    void refreshModels();
  }, []);

  useEffect(() => {
    void refreshUsage();
  }, [usageModelId, from, to, models.length]);

  useEffect(() => {
    setReplaceApiKey(false);
    setReplaceEmbeddingApiKey(false);
  }, [legacyModel.secretRefreshToken]);

  const activeModel = useMemo(() => models.find((model) => model.active), [models]);

  const changeProvider = (provider: ProviderPresetId) => {
    const preset = PROVIDER_PRESETS[provider];
    setForm((current) => ({
      ...current,
      provider,
      label: preset.label,
      model: preset.model,
      base_url: preset.baseUrl,
    }));
  };

  const handleSubmit = async () => {
    setFeedback("");
    const result = editingId
      ? await updateLlmModel(editingId, form)
      : await createLlmModel(form);
    if (!result) {
      setFeedback("模型保存失败，请检查地址、凭据库和输入内容");
      return;
    }
    setFeedback(editingId ? "模型已更新" : "模型已添加，请先验证连接");
    setForm(emptyForm());
    setEditingId(null);
    setReplaceProfileApiKey(false);
    await refreshModels();
  };

  const handleVerify = async (model: LlmModel) => {
    setBusyId(model.id);
    setFeedback("");
    const result = await verifyLlmModel(model.id);
    if (!result) {
      setFeedback(`${model.label} 连接验证失败`);
    } else {
      setFeedback(`${model.label} 连接验证成功`);
      await refreshModels();
      await refreshUsage();
    }
    setBusyId(null);
  };

  const handleActivate = async (model: LlmModel) => {
    setBusyId(model.id);
    const result = await activateLlmModel(model.id);
    setFeedback(result ? `已切换到 ${model.label}` : "切换失败：请先验证模型连接");
    if (result) await refreshModels();
    setBusyId(null);
  };

  const handleDelete = async (model: LlmModel) => {
    if (!window.confirm(`删除模型“${model.label}”？其 API Key 也会从系统凭据库移除。`)) return;
    setBusyId(model.id);
    const result = await deleteLlmModel(model.id);
    setFeedback(result ? "模型已删除" : "模型删除失败");
    if (result) await refreshModels();
    setBusyId(null);
  };

  const clearProfileSecret = async (model: LlmModel) => {
    if (!window.confirm(`清除模型“${model.label}”保存在系统凭据库中的 API Key？`)) return;
    setBusyId(model.id);
    setFeedback("");
    const result = await updateLlmModel(model.id, {
      provider: model.provider,
      label: model.label,
      model: model.model,
      base_url: model.base_url,
      api_format: "openai",
      api_key: "",
      api_key_env: model.api_key_env,
      clear_api_key: true,
      temperature: model.temperature,
      max_tokens: model.max_tokens,
      invoke_timeout_ms: model.invoke_timeout_ms,
    });
    setFeedback(result ? `${model.label} 的密钥已清除` : `${model.label} 的密钥清除失败`);
    if (result) await refreshModels();
    setBusyId(null);
  };

  const beginEdit = (model: LlmModel) => {
    setEditingId(model.id);
    setReplaceProfileApiKey(false);
    setForm({
      provider: model.provider,
      label: model.label,
      model: model.model,
      base_url: model.base_url,
      api_format: "openai",
      api_key: "",
      api_key_env: model.api_key_env,
      temperature: model.temperature,
      max_tokens: model.max_tokens,
      invoke_timeout_ms: model.invoke_timeout_ms,
    });
  };

  const patchForm = <K extends keyof LlmModelPayload>(key: K, value: LlmModelPayload[K]) => {
    setForm((current) => ({ ...current, [key]: value }));
  };

  return (
    <div className="space-y-4 border-b border-[var(--border-soft)] pb-6">
      <div>
        <div className="flex items-center justify-between gap-3">
          <div>
            <h3 className="font-semibold text-sm uppercase tracking-wider text-[var(--text-muted)]">模型服务</h3>
            <p className="mt-1 text-xs text-[var(--text-muted)]">选择服务，输入密钥和模型，保存后测试连接。</p>
          </div>
          {activeModel ? <Badge tone="success"><Zap className="h-3 w-3" />当前档案：{activeModel.label}</Badge> : null}
        </div>
        {feedback ? <p className="mt-2 text-xs text-[var(--accent-primary)]" role="status">{feedback}</p> : null}
      </div>

      {activeModel && <p className="u2-config-note" role="status">当前运行使用“{activeModel.label}”档案。下方默认配置仅作备用；修改当前模型请展开“多模型档案与用量”。连接测试使用当前运行档案。</p>}
      <div className="u2-model-basic rounded-[var(--radius-md)] border border-[var(--border-soft)] bg-[var(--surface-solid)] p-4 space-y-4">
        <div>
          <h4 className="text-sm font-medium">基础设置</h4>
        </div>
        {legacyModel.config.migration_pending ? (
          <div className="rounded-lg border border-[var(--warning)]/30 bg-[var(--warning)]/10 px-3 py-2">
            <p className="text-xs text-[var(--warning)]">检测到旧版明文密钥，系统安全凭据库当前不可用；未删除旧配置。恢复凭据库后将自动迁移。</p>
          </div>
        ) : null}
        {legacyModel.config.secret_store_status === "unavailable" ? (
          <div className="rounded-lg border border-[var(--danger)]/30 bg-[var(--danger)]/10 px-3 py-2">
            <p className="text-xs text-[var(--danger)]">系统安全凭据库不可用，无法新增直接密钥；环境变量方式仍可使用。</p>
          </div>
        ) : null}
        <ModelPresetSelect url={legacyModel.config.model.base_url} onField={(key,value)=>legacyModel.updateField("model",key,value)} onAdvanced={()=>{if(advancedRef.current)advancedRef.current.open=true;}} />
        {legacyModel.config.model.api_key_configured && <div className="flex items-center justify-between">
          <p className="text-xs text-[var(--text-muted)]">{legacyModel.secretSourceLabel(legacyModel.config.model.api_key_source)}</p>
          <div className="flex gap-2">
            {legacyModel.config.model.api_key_configured && <Button variant="secondary" size="sm" onClick={() => setReplaceApiKey(true)}>替换密钥</Button>}
            {legacyModel.config.model.api_key_configured && <Button variant="secondary" size="sm" onClick={() => void legacyModel.clearSecret("api_key")}>清除密钥</Button>}
          </div>
        </div>}
        {(!legacyModel.config.model.api_key_configured || replaceApiKey) && <Input label="API 密钥" type="password" value={legacyModel.config.model.api_key} onChange={(event) => legacyModel.updateField("model", "api_key", event.target.value)} placeholder="输入后安全保存" />}
        <Input label="模型名称" value={legacyModel.config.model.name} onChange={(event) => legacyModel.updateField("model", "name", event.target.value)} />
        {actions}
        <details ref={advancedRef} className="u2-disclosure"><summary>高级设置 <span>地址、采样与 Embedding</span></summary><div className="space-y-4 pt-4">
        <Input label="API 地址" value={legacyModel.config.model.base_url} onChange={(event) => legacyModel.updateField("model", "base_url", event.target.value)} />
        <Input label="环境变量名" value={legacyModel.config.model.api_key_env} onChange={(event) => legacyModel.updateField("model", "api_key_env", event.target.value)} />
        <div className="border-t border-[var(--border)] pt-4 space-y-4">
          <h5 className="font-semibold text-sm text-[var(--text-muted)]">Embedding 配置</h5>
          <Input label="Embedding API 地址" value={legacyModel.config.model.embedding_base_url} onChange={(event) => legacyModel.updateField("model", "embedding_base_url", event.target.value)} placeholder="https://api.openai.com/v1" />
          <Input label="Embedding 模型名称" value={legacyModel.config.model.embedding_model} onChange={(event) => legacyModel.updateField("model", "embedding_model", event.target.value)} placeholder="text-embedding-3-small" />
          <div className="flex items-center justify-between">
            <p className="text-xs text-[var(--text-muted)]">{legacyModel.secretSourceLabel(legacyModel.config.model.embedding_api_key_source)}</p>
            <div className="flex gap-2">
              {legacyModel.config.model.embedding_api_key_configured && <Button variant="secondary" size="sm" onClick={() => setReplaceEmbeddingApiKey(true)}>替换密钥</Button>}
              {legacyModel.config.model.embedding_api_key_configured && <Button variant="secondary" size="sm" onClick={() => void legacyModel.clearSecret("embedding_api_key")}>清除密钥</Button>}
            </div>
          </div>
          {(!legacyModel.config.model.embedding_api_key_configured || replaceEmbeddingApiKey) && <Input label="Embedding API 密钥" type="password" value={legacyModel.config.model.embedding_api_key} onChange={(event) => legacyModel.updateField("model", "embedding_api_key", event.target.value)} placeholder="输入后安全保存" />}
          <Input label="Embedding 环境变量名" value={legacyModel.config.model.embedding_api_key_env} onChange={(event) => legacyModel.updateField("model", "embedding_api_key_env", event.target.value)} />
        </div>
        <div className="grid grid-cols-2 gap-3">
          <Input label="温度" type="number" value={String(legacyModel.config.model.temperature)} onChange={(event) => legacyModel.updateField("model", "temperature", parseFloat(event.target.value) || 0)} />
          <Input label="最大 Token" type="number" value={String(legacyModel.config.model.max_tokens)} onChange={(event) => legacyModel.updateField("model", "max_tokens", parseInt(event.target.value) || 0)} />
        </div>
        <Input label="超时 (ms)" type="number" value={String(legacyModel.config.model.invoke_timeout_ms)} onChange={(event) => legacyModel.updateField("model", "invoke_timeout_ms", parseInt(event.target.value) || 0)} />
        </div></details>
      </div>

      <details className="u2-disclosure u2-profile-management"><summary>多模型档案与用量 <span>管理来源、切换模型与使用记录</span></summary><div className="space-y-4 pt-4">
      <ModelTree models={models.filter((model) => Boolean(model.verified_at))} />

      <div className="grid gap-2">
        {models.map((model) => (
          <div key={model.id} data-model-id={model.id} className="rounded-[var(--radius-md)] border border-[var(--border-soft)] bg-[var(--surface-solid)] p-3">
            <div className="flex flex-wrap items-start justify-between gap-2">
              <div className="min-w-0">
                <div className="flex items-center gap-2"><span className="truncate text-sm font-medium">{model.label}</span>{model.active ? <Badge tone="success">使用中</Badge> : null}{model.verified_at ? <Badge tone="info"><CheckCircle2 className="h-3 w-3" />已验证</Badge> : <Badge tone="default"><CircleAlert className="h-3 w-3" />未验证</Badge>}</div>
                <p className="mt-1 truncate text-xs text-[var(--text-muted)]">{providerLabel(model.provider)} · {model.model} · {model.api_key_configured ? "凭据已配置" : "等待凭据"}</p>
                {model.last_error ? <p className="mt-1 text-xs text-[var(--danger)]">{model.last_error}</p> : null}
              </div>
              <div className="flex shrink-0 gap-1">
                 <Button variant="secondary" size="sm" onClick={() => void handleVerify(model)} disabled={busyId === model.id}><RefreshCw className={`h-3.5 w-3.5 ${busyId === model.id ? "animate-spin" : ""}`} />验证</Button>
                 {!model.active ? <Button variant="secondary" size="sm" onClick={() => void handleActivate(model)} disabled={busyId === model.id}>激活</Button> : null}
                 {model.api_key_source === "secret_store" ? <Button variant="secondary" size="sm" onClick={() => void clearProfileSecret(model)} disabled={busyId === model.id}>清除密钥</Button> : null}
                 <Button variant="ghost" size="sm" aria-label={`编辑${model.label}`} onClick={() => beginEdit(model)}><Pencil className="h-3.5 w-3.5" /></Button>
                <Button variant="danger" size="sm" aria-label={`删除${model.label}`} onClick={() => void handleDelete(model)} disabled={busyId === model.id}><Trash2 className="h-3.5 w-3.5" /></Button>
              </div>
            </div>
          </div>
        ))}
      </div>

      <div data-testid="model-profile-form" className="rounded-[var(--radius-md)] border border-dashed border-[var(--border-strong)] p-3">
        <div className="mb-3 flex items-center justify-between"><h4 className="text-sm font-medium">{editingId ? "编辑模型档案" : "添加模型档案"}</h4>{editingId ? <Button variant="ghost" size="sm" onClick={() => { setEditingId(null); setReplaceProfileApiKey(false); setForm(emptyForm()); }}>取消编辑</Button> : null}</div>
        <div className="grid gap-3 sm:grid-cols-2">
          <div><label className="mb-1.5 block text-sm font-medium">服务商</label><select value={form.provider} onChange={(event) => changeProvider(event.target.value as ProviderPresetId)} className="w-full rounded-[var(--radius-md)] border border-[var(--border-soft)] bg-[var(--surface-solid)] px-3 py-2 text-sm text-[var(--text)]"><option value="openai">OpenAI</option><option value="deepseek">DeepSeek</option><option value="qwen">千问</option><option value="glm">GLM</option><option value="custom">自定义 OpenAI 兼容</option></select></div>
          <Input label="显示名称" value={form.label} onChange={(event) => patchForm("label", event.target.value)} placeholder="例如：主力模型" />
          <Input label="模型名称" value={form.model} onChange={(event) => patchForm("model", event.target.value)} placeholder="例如：deepseek-chat" />
          <Input label="API 地址" value={form.base_url} onChange={(event) => patchForm("base_url", event.target.value)} placeholder="https://.../v1" />
          {(!editingId || replaceProfileApiKey) ? <Input label="API Key" type="password" value={form.api_key ?? ""} onChange={(event) => patchForm("api_key", event.target.value)} placeholder="输入后安全保存" /> : <div className="flex items-end"><Button variant="secondary" size="sm" onClick={() => setReplaceProfileApiKey(true)}>替换密钥</Button></div>}
          <Input label="环境变量名（可选）" value={form.api_key_env ?? ""} onChange={(event) => patchForm("api_key_env", event.target.value)} placeholder="OPENAI_API_KEY" />
          <Input label="温度" type="number" min="0" max="2" step="0.1" value={String(form.temperature)} onChange={(event) => patchForm("temperature", Number(event.target.value))} />
          <Input label="最大 Token" type="number" min="1" value={String(form.max_tokens)} onChange={(event) => patchForm("max_tokens", Number(event.target.value))} />
        </div>
        <div className="mt-3 flex justify-end"><Button onClick={() => void handleSubmit()} disabled={!form.label || !form.model || !form.base_url}><Plus className="h-4 w-4" />{editingId ? "保存模型" : "添加模型"}</Button></div>
      </div>

      <div className="space-y-3">
        <div className="flex flex-wrap items-end gap-2"><div className="min-w-40 flex-1"><label className="mb-1 block text-xs text-[var(--text-muted)]">模型</label><select value={usageModelId} onChange={(event) => setUsageModelId(event.target.value)} className="w-full rounded-[var(--radius-md)] border border-[var(--border-soft)] bg-[var(--surface-solid)] px-3 py-2 text-sm text-[var(--text)]"><option value="">全部模型</option>{models.map((model) => <option key={model.id} value={model.id}>{model.label}</option>)}</select></div><div><label className="mb-1 block text-xs text-[var(--text-muted)]">开始</label><input type="date" value={from} onChange={(event) => setFrom(event.target.value)} className="rounded-[var(--radius-md)] border border-[var(--border-soft)] bg-[var(--surface-solid)] px-2.5 py-2 text-sm text-[var(--text)]" /></div><div><label className="mb-1 block text-xs text-[var(--text-muted)]">结束</label><input type="date" value={to} onChange={(event) => setTo(event.target.value)} className="rounded-[var(--radius-md)] border border-[var(--border-soft)] bg-[var(--surface-solid)] px-2.5 py-2 text-sm text-[var(--text)]" /></div></div>
        <TokenUsageChart report={usage} />
      </div>
      </div></details>
    </div>
  );
}
