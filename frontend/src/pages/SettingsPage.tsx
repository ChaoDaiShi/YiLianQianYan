import { useState, useEffect } from "react";
import { Check, Monitor, Moon, Sun, Upload, RotateCcw } from "lucide-react";
import { getSettings, updateSettings, getIsolationStatus, listSecurityGrants, type IsolationStatus, type SecurityGrant } from "../api/client";
import { useTheme } from "../theme";
import type { AppConfig } from "../types";
import { PageHeader, Button, Input, Badge } from "../components/ui";
import { GrantEditor } from "../features/security/GrantEditor";
import { isolationRows } from "../features/security/grantEditorModel";
import ModelManagerPanel from "../features/llm/ModelManagerPanel";

const defaultConfig: AppConfig = {
  agent: { name: "忆涟千言", system_prompt: "你是一个桌面AI助手...", workspace_root: "" },
  model: {
    provider: "openai",
    name: "deepseek-v4-flash",
    base_url: "https://api.deepseek.com/v1",
    api_key: "",
    api_key_env: "OPENAI_API_KEY",
    temperature: 0,
    max_tokens: 16384,
    invoke_timeout_ms: 120000,
    embedding_model: "",
    embedding_base_url: "",
    embedding_api_key: "",
    embedding_api_key_env: "",
  },
  permissions: { mode: "ask", interrupt_on: ["bash", "write_file", "edit_file", "http_request"] },
  sandbox: { profile: "workspace-write", writable_paths: [], denied_write_paths: [] },
  skills: { directories: [], progressive_loading: true },
  subagents: { directories: [] },
  compaction: { enabled: true, context_window: 200000, trigger_threshold: 0.8, keep_recent_tokens: 20000 },
};

type SectionKey = "model" | "agent" | "permissions" | "sandbox" | "compaction" | "skills" | "appearance";

const SECTIONS: { key: SectionKey; label: string }[] = [
  { key: "model", label: "模型" },
  { key: "agent", label: "智能体" },
  { key: "permissions", label: "权限与安全" },
  { key: "sandbox", label: "沙箱" },
  { key: "compaction", label: "压缩" },
  { key: "skills", label: "技能与子智能体" },
  { key: "appearance", label: "外观" },
];

const APPEARANCE_MODES = [
  {
    id: "system",
    label: "跟随系统",
    description: "自动使用系统的明暗外观",
    icon: Monitor,
  },
  {
    id: "light",
    label: "白天",
    description: "清透的月光白与淡粉紫界面",
    icon: Sun,
  },
  {
    id: "dark",
    label: "夜间",
    description: "低眩光的深紫夜色界面",
    icon: Moon,
  },
] as const;

function comparableConfig(config: AppConfig) {
  return JSON.stringify({
    ...config,
    model: {
      ...config.model,
      api_key: "",
      embedding_api_key: "",
      clear_api_key: undefined,
      clear_embedding_api_key: undefined,
    },
  });
}

function secretSourceLabel(source?: string): string {
  switch (source) {
    case "secret_store":
      return "已安全保存到系统凭据库";
    case "environment":
      return "由环境变量提供";
    case "legacy_pending":
      return "检测到旧版明文密钥，待迁移";
    case "none":
    default:
      return "未配置";
  }
}

export default function SettingsPage() {
  const [config, setConfig] = useState<AppConfig>(defaultConfig);
  const [saved, setSaved] = useState(false);
  const [savedSnapshot, setSavedSnapshot] = useState(() => comparableConfig(defaultConfig));
  const [saveError, setSaveError] = useState("");
  const [saving, setSaving] = useState(false);
  const [activeSection, setActiveSection] = useState<SectionKey>("model");
  const [isolation, setIsolation] = useState<IsolationStatus | null>(null);
  const [grants, setGrants] = useState<SecurityGrant[]>([]);
  const theme = useTheme();

  useEffect(() => {
    getSettings().then((c) => {
      if (c) {
        const nextConfig = {
          ...defaultConfig,
          ...c,
          model: { ...defaultConfig.model, ...c.model, api_key: c.model.api_key || "", embedding_api_key: c.model.embedding_api_key || "" },
        };
        setConfig(nextConfig);
        setSavedSnapshot(comparableConfig(nextConfig));
      }
    });
    getIsolationStatus().then((s) => { if (s) setIsolation(s); });
    listSecurityGrants().then((items) => { if (items) setGrants(items); });
  }, []);

  const refreshGrants = async () => {
    const items = await listSecurityGrants();
    if (items) setGrants(items);
  };

  const handleSave = async () => {
    setSaving(true);
    setSaveError("");
    const result = await updateSettings(config);
    if (result && !result.error) {
      setSavedSnapshot(comparableConfig(config));
      setSaved(true);
      window.setTimeout(() => setSaved(false), 2500);
    } else {
      setSaved(false);
      setSaveError("设置暂时无法保存");
    }
    setSaving(false);
  };

  const clearSecret = async (field: "api_key" | "embedding_api_key") => {
    const clearField = field === "api_key" ? "clear_api_key" : "clear_embedding_api_key";
    const next = {
      ...config,
      model: { ...config.model, [clearField]: true, [field]: "" },
    };
    const result = await updateSettings(next);
    if (!result || result.error) {
      setSaveError("设置暂时无法保存");
      return;
    }
    const fresh = await getSettings();
    if (fresh) {
      const nextConfig = {
        ...defaultConfig,
        ...fresh,
        model: { ...defaultConfig.model, ...fresh.model, api_key: "", embedding_api_key: "" },
      };
      setConfig(nextConfig);
      setSavedSnapshot(comparableConfig(nextConfig));
    }
    setSaved(true);
    setSaveError("");
    window.setTimeout(() => setSaved(false), 2500);
  };

  const updateField = (section: keyof AppConfig, key: string, value: any) => {
    setSaveError("");
    setSaved(false);
    setConfig((prev: any) => ({
      ...prev,
      [section]: { ...prev[section], [key]: value },
    }));
  };

  const dirty = comparableConfig(config) !== savedSnapshot;

  // ── Appearance helpers ──

  const handleBgUpload = (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (!file) return;
    const reader = new FileReader();
    reader.onload = () => {
      theme.setBackgroundImage(reader.result as string);
    };
    reader.readAsDataURL(file);
  };

  // ── Render sections ──

  const renderSection = () => {
    switch (activeSection) {
      case "model":
        return (
          <div className="space-y-4">
            <ModelManagerPanel
              legacyModel={{ config, updateField, clearSecret, secretSourceLabel }}
            />
          </div>
        );

      case "agent":
        return (
          <div className="space-y-4">
            <h3 className="font-semibold text-sm uppercase tracking-wider text-[var(--text-muted)]">智能体配置</h3>
            <Input label="名称" value={config.agent.name} onChange={(e) => updateField("agent", "name", e.target.value)} />
            <Input label="工作区根目录" value={config.agent.workspace_root} onChange={(e) => updateField("agent", "workspace_root", e.target.value)} placeholder="默认当前目录" />
            <div>
              <label className="block text-sm font-medium mb-1">系统提示词</label>
              <textarea
                value={config.agent.system_prompt}
                onChange={(e) => updateField("agent", "system_prompt", e.target.value)}
                rows={10}
                className="w-full rounded-lg border border-[var(--border)] bg-[var(--input-bg)] px-3 py-2 text-sm text-[var(--text)] placeholder:text-[var(--text-faint)] focus:outline-none focus:ring-2 focus:ring-[var(--accent)]/40 resize-none font-mono"
              />
            </div>
          </div>
        );

      case "permissions":
        return (
          <div className="space-y-4">
            <h3 className="font-semibold text-sm uppercase tracking-wider text-[var(--text-muted)]">权限配置</h3>
            {isolation && (
              <div className="rounded-lg border border-[var(--border)] px-3 py-3 space-y-1">
                <h4 className="text-sm font-medium">Windows 隔离能力</h4>
                {isolationRows(isolation).map(([label, active]) => <div key={label} className="flex items-center justify-between text-xs"><span className="text-[var(--text-muted)]">{label}</span><Badge tone={active ? "success" : "default"}>{active ? "Active" : "Not enabled"}</Badge></div>)}
                <p className="text-[11px] text-[var(--text-faint)] mt-2">当前安全边界由 Windows 令牌降权、Job Object 和应用层资源授权共同组成；Restricting-SID、OS 命名空间、GUI 沙箱与容器级隔离尚未启用。</p>
              </div>
            )}
            <div>
              <label className="block text-sm font-medium mb-1">模式</label>
              <select
                value={config.permissions.mode}
                onChange={(e) => updateField("permissions", "mode", e.target.value)}
                className="w-full rounded-lg border border-[var(--border)] bg-[var(--input-bg)] px-3 py-2 text-sm text-[var(--text)] focus:outline-none focus:ring-2 focus:ring-[var(--accent)]/40"
              >
                <option value="yolo">YOLO — 自动执行所有操作</option>
                <option value="ask">Ask — 每次操作前确认</option>
                <option value="plan">Plan — 仅执行已批准的计划</option>
              </select>
            </div>
            <div>
              <label className="block text-sm font-medium mb-1">需确认的工具</label>
              <input
                type="text"
                value={config.permissions.interrupt_on.join(", ")}
                onChange={(e) => updateField("permissions", "interrupt_on", e.target.value.split(",").map((s) => s.trim()).filter(Boolean))}
                placeholder="bash, write_file, edit_file"
                className="w-full rounded-lg border border-[var(--border)] bg-[var(--input-bg)] px-3 py-2 text-sm text-[var(--text)] focus:outline-none focus:ring-2 focus:ring-[var(--accent)]/40"
              />
              <p className="mt-1 text-xs text-[var(--text-muted)]">逗号分隔的工具名称列表</p>
            </div>
            <GrantEditor grants={grants} onChanged={refreshGrants} />
          </div>
        );

      case "sandbox":
        return (
          <div className="space-y-4">
            <h3 className="font-semibold text-sm uppercase tracking-wider text-[var(--text-muted)]">沙箱配置</h3>
            <div>
              <label className="block text-sm font-medium mb-1">配置文件</label>
              <select
                value={config.sandbox.profile}
                onChange={(e) => updateField("sandbox", "profile", e.target.value)}
                className="w-full rounded-lg border border-[var(--border)] bg-[var(--input-bg)] px-3 py-2 text-sm text-[var(--text)] focus:outline-none focus:ring-2 focus:ring-[var(--accent)]/40"
              >
                <option value="custom">自定义</option>
                <option value="workspace-write">Workspace Write — 仅工作区可写</option>
                <option value="read-only">Read Only — 只读</option>
                <option value="open">Open — 无限制</option>
              </select>
            </div>
            <div>
              <label className="block text-sm font-medium mb-1">可写路径 (一行一个)</label>
              <textarea
                value={config.sandbox.writable_paths.join("\n")}
                onChange={(e) => updateField("sandbox", "writable_paths", e.target.value.split("\n").filter(Boolean))}
                rows={3}
                className="w-full rounded-lg border border-[var(--border)] bg-[var(--input-bg)] px-3 py-2 text-sm text-[var(--text)] focus:outline-none focus:ring-2 focus:ring-[var(--accent)]/40 resize-none"
              />
            </div>
            <div>
              <label className="block text-sm font-medium mb-1">禁止写入路径 (一行一个)</label>
              <textarea
                value={config.sandbox.denied_write_paths.join("\n")}
                onChange={(e) => updateField("sandbox", "denied_write_paths", e.target.value.split("\n").filter(Boolean))}
                rows={3}
                className="w-full rounded-lg border border-[var(--border)] bg-[var(--input-bg)] px-3 py-2 text-sm text-[var(--text)] focus:outline-none focus:ring-2 focus:ring-[var(--accent)]/40 resize-none"
              />
            </div>
          </div>
        );

      case "compaction":
        return (
          <div className="space-y-4">
            <h3 className="font-semibold text-sm uppercase tracking-wider text-[var(--text-muted)]">对话压缩</h3>
            <div className="flex items-center gap-3">
              <label className="text-sm">启用压缩</label>
              <input
                type="checkbox"
                aria-label="启用对话压缩"
                checked={config.compaction.enabled}
                onChange={(e) => updateField("compaction", "enabled", e.target.checked)}
                className="w-4 h-4 rounded accent-[var(--accent)]"
              />
            </div>
            <Input label="上下文窗口 (tokens)" type="number" value={String(config.compaction.context_window)} onChange={(e) => updateField("compaction", "context_window", parseInt(e.target.value) || 0)} />
            <Input label="触发阈值" type="number" value={String(config.compaction.trigger_threshold)} onChange={(e) => updateField("compaction", "trigger_threshold", parseFloat(e.target.value) || 0)} hint="上下文占用比例超过此值时触发压缩 (0–1)" />
            <Input label="保留最近 (tokens)" type="number" value={String(config.compaction.keep_recent_tokens)} onChange={(e) => updateField("compaction", "keep_recent_tokens", parseInt(e.target.value) || 0)} hint="压缩时保留最近的 token 数量" />
          </div>
        );

      case "skills":
        return (
          <div className="space-y-4">
            <h3 className="font-semibold text-sm uppercase tracking-wider text-[var(--text-muted)]">技能路径</h3>
            <div>
              <label className="block text-sm font-medium mb-1">技能目录 (一行一个)</label>
              <textarea
                value={config.skills.directories.join("\n")}
                onChange={(e) => updateField("skills", "directories", e.target.value.split("\n").filter(Boolean))}
                rows={4}
                placeholder="./skills"
                className="w-full rounded-lg border border-[var(--border)] bg-[var(--input-bg)] px-3 py-2 text-sm text-[var(--text)] focus:outline-none focus:ring-2 focus:ring-[var(--accent)]/40 resize-none"
              />
            </div>
            <div className="flex items-center gap-3">
              <label className="text-sm">渐进加载</label>
              <input
                type="checkbox"
                aria-label="启用渐进加载"
                checked={config.skills.progressive_loading}
                onChange={(e) => updateField("skills", "progressive_loading", e.target.checked)}
                className="w-4 h-4 rounded accent-[var(--accent)]"
              />
            </div>
            <div className="border-t border-[var(--border-soft)] pt-4">
              <h3 className="font-semibold text-sm uppercase tracking-wider text-[var(--text-muted)]">子智能体目录</h3>
              <label className="mt-3 block text-sm font-medium mb-1" htmlFor="subagent-directories">目录（每行一个）</label>
              <textarea
                id="subagent-directories"
                aria-label="子智能体目录"
                value={config.subagents.directories.join("\n")}
                onChange={(e) => updateField("subagents", "directories", e.target.value.split("\n").filter(Boolean))}
                rows={4}
                placeholder="./.agents/agents"
                className="w-full rounded-lg border border-[var(--border)] bg-[var(--input-bg)] px-3 py-2 text-sm text-[var(--text)] focus:outline-none focus:ring-2 focus:ring-[var(--accent)]/40 resize-none"
              />
              <p className="mt-1 text-xs text-[var(--text-muted)]">扫描目录中的 AGENT.md 文件定义子智能体</p>
            </div>
          </div>
        );

      case "appearance":
        return (
          <div className="space-y-6">
            <div>
              <h3 className="font-semibold text-sm uppercase tracking-wider text-[var(--text-muted)]">
                昔涟外观
              </h3>
              <p className="mt-1 text-xs leading-5 text-[var(--text-faint)]">
                保留昔涟 · 涟漪的统一视觉，只切换适合环境的明暗层级。
              </p>
            </div>
            <div
              className="grid grid-cols-1 gap-3 sm:grid-cols-3"
              role="radiogroup"
              aria-label="外观模式"
            >
              {APPEARANCE_MODES.map((item) => {
                const Icon = item.icon;
                const selected = theme.mode === item.id;
                return (
                <button
                  key={item.id}
                  onClick={() => theme.setMode(item.id)}
                  type="button"
                  role="radio"
                  aria-checked={selected}
                  className={`rounded-xl border p-4 text-left transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--focus-ring-soft)] ${
                    selected
                      ? "border-[var(--accent-border)] bg-[var(--accent-soft)]"
                      : "border-[var(--border-soft)] bg-[var(--surface-muted)] hover:bg-[var(--surface-hover)]"
                  }`}
                >
                  <div className="mb-2 flex items-center justify-between gap-2">
                    <Icon className="h-4 w-4 text-[var(--accent-primary)]" />
                    {selected ? <Check className="h-4 w-4 text-[var(--accent-primary)]" /> : null}
                  </div>
                  <span className="block text-sm font-medium text-[var(--text-primary)]">
                    {item.label}
                  </span>
                  <span className="mt-1 block text-xs leading-5 text-[var(--text-secondary)]">
                    {item.description}
                  </span>
                </button>
                );
              })}
            </div>
            {theme.mode === "system" ? (
              <p className="text-xs text-[var(--text-secondary)]" role="status">
                当前跟随系统：{theme.resolvedScheme === "dark" ? "夜间" : "白天"}
              </p>
            ) : null}

            <div className="border-t border-[var(--divider)] pt-5">
              <h4 className="text-sm font-medium mb-2">背景</h4>
              <div className="flex items-center gap-3 flex-wrap">
                <label className="inline-flex items-center gap-2 px-3 py-2 rounded-lg border border-[var(--border)] hover:bg-[var(--panel-hover)] cursor-pointer text-sm transition-colors">
                  <Upload className="w-4 h-4" />
                  上传背景图
                  <input type="file" accept="image/*" onChange={handleBgUpload} className="hidden" />
                </label>
                {theme.bgImageUrl && (
                  <Button variant="secondary" size="sm" onClick={() => theme.setBackgroundImage(null)}>
                    清除背景
                  </Button>
                )}
              </div>
              <p className="mt-2 text-xs leading-5 text-[var(--text-muted)]">
                背景图片会自动叠加可读性遮罩，不改变风险和验证状态颜色。
              </p>
            </div>

            <div className="flex items-center gap-3">
              <Button variant="secondary" size="sm" onClick={theme.resetTheme}>
                <RotateCcw className="w-4 h-4" />
                重置默认
              </Button>
            </div>
          </div>
        );

      default:
        return null;
    }
  };

  return (
    <div className="system-settings-page flex h-full min-h-0 flex-col">
      <PageHeader
        title="设置"
        description={saveError ? saveError : saved ? "设置已保存" : dirty ? "有未保存修改" : "配置应用参数"}
        actions={
          saveError ? (
            <Badge tone="danger">保存失败</Badge>
          ) : saved ? (
            <Badge tone="success">
              <Check className="w-3.5 h-3.5" />
              已保存
            </Badge>
          ) : dirty ? <Badge tone="warning">未保存</Badge> : null
        }
      />

      {saveError ? <div className="mx-4 mt-3" role="alert"><p className="text-xs text-[var(--danger)]">设置暂时无法保存，请检查连接后重试。</p></div> : null}

      <div className="system-settings-layout min-h-0 flex-1 overflow-hidden">
        {/* Section tabs */}
        <nav className="system-settings-nav overflow-y-auto scrollbar-thin" aria-label="设置分类">
          {SECTIONS.map((s) => (
            <button
              key={s.key}
              type="button"
              onClick={() => setActiveSection(s.key)}
              aria-current={activeSection === s.key ? "page" : undefined}
              className={`w-full text-left px-4 py-2.5 text-sm transition-colors ${
                activeSection === s.key
                  ? "bg-[var(--accent)]/15 text-[var(--accent)] border-r-2 border-r-[var(--accent)] font-medium"
                  : "text-[var(--text-muted)] hover:bg-[var(--panel-hover)] hover:text-[var(--text)]"
              }`}
            >
              {s.label}
            </button>
          ))}
        </nav>

        {/* Section content */}
        <div className="system-settings-content min-h-0 flex-1 overflow-y-auto p-6 scrollbar-thin">
          <div className="max-w-2xl">
            {renderSection()}
          </div>
        </div>
      </div>

      {/* Save bar */}
      <div className="system-settings-savebar border-t border-[var(--border)] px-6 py-3 bg-[var(--panel)]/60 flex items-center justify-between">
        <p className="text-xs text-[var(--text-faint)]">
          修改后点击保存设置
        </p>
        <Button onClick={handleSave} disabled={!dirty || saving} aria-label="保存设置">
          <Check className="w-4 h-4" />
          {saving ? "保存中…" : "保存设置"}
        </Button>
      </div>
    </div>
  );
}
