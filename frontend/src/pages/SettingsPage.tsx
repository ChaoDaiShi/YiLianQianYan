import { useState, useEffect } from "react";
import { Check, Monitor, Upload, Download, RotateCcw } from "lucide-react";
import { getSettings, updateSettings } from "../api/client";
import { useTheme } from "../theme";
import { PRESET_META } from "../theme/presets";
import type { AppConfig } from "../types";
import type { PresetId } from "../theme/types";
import { PageHeader, Button, Input, Badge } from "../components/ui";

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

type SectionKey = "model" | "agent" | "permissions" | "sandbox" | "compaction" | "skills" | "subagents" | "appearance";

const SECTIONS: { key: SectionKey; label: string }[] = [
  { key: "model", label: "模型" },
  { key: "agent", label: "智能体" },
  { key: "permissions", label: "权限" },
  { key: "sandbox", label: "沙箱" },
  { key: "compaction", label: "压缩" },
  { key: "skills", label: "技能" },
  { key: "subagents", label: "子智能体" },
  { key: "appearance", label: "外观" },
];

export default function SettingsPage() {
  const [config, setConfig] = useState<AppConfig>(defaultConfig);
  const [saved, setSaved] = useState(false);
  const [activeSection, setActiveSection] = useState<SectionKey>("model");
  const theme = useTheme();

  useEffect(() => {
    getSettings().then((c) => {
      if (c) {
        setConfig({
          ...defaultConfig,
          ...c,
          model: { ...defaultConfig.model, ...c.model, api_key: c.model.api_key || "", embedding_api_key: c.model.embedding_api_key || "" },
        });
      }
    });
  }, []);

  const handleSave = async () => {
    await updateSettings(config);
    setSaved(true);
    setTimeout(() => setSaved(false), 2500);
  };

  const updateField = (section: keyof AppConfig, key: string, value: any) => {
    setConfig((prev: any) => ({
      ...prev,
      [section]: { ...prev[section], [key]: value },
    }));
  };

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

  const handleExport = () => {
    const json = theme.exportTheme();
    const blob = new Blob([json], { type: "application/json" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = "ylqy-theme.json";
    a.click();
    URL.revokeObjectURL(url);
  };

  const handleImport = () => {
    const input = document.createElement("input");
    input.type = "file";
    input.accept = ".json";
    input.onchange = (e) => {
      const file = (e.target as HTMLInputElement).files?.[0];
      if (!file) return;
      const reader = new FileReader();
      reader.onload = () => {
        const ok = theme.importTheme(reader.result as string);
        if (!ok) alert("导入失败：主题文件格式不正确");
      };
      reader.readAsText(file);
    };
    input.click();
  };

  // ── Render sections ──

  const renderSection = () => {
    switch (activeSection) {
      case "model":
        return (
          <div className="space-y-4">
            <h3 className="font-semibold text-sm uppercase tracking-wider text-[var(--text-muted)]">模型配置</h3>
            <Input label="API 地址" value={config.model.base_url} onChange={(e) => updateField("model", "base_url", e.target.value)} />
            <Input label="模型名称" value={config.model.name} onChange={(e) => updateField("model", "name", e.target.value)} />
            <Input label="API 密钥" type="password" value={config.model.api_key} onChange={(e) => updateField("model", "api_key", e.target.value)} placeholder={config.model.api_key_configured ? "已配置；输入新 key 可替换" : "sk-..."} />
            {config.model.api_key_configured && <p className="text-xs text-[var(--success)]">已配置；页面不会回显原始 key，留空保存会保留现有 key。</p>}
            <Input label="环境变量名" value={config.model.api_key_env} onChange={(e) => updateField("model", "api_key_env", e.target.value)} />
            <div className="border-t border-[var(--border)] pt-4 space-y-4">
              <h4 className="font-semibold text-sm text-[var(--text-muted)]">Embedding 配置</h4>
              <Input label="Embedding API 地址" value={config.model.embedding_base_url} onChange={(e) => updateField("model", "embedding_base_url", e.target.value)} placeholder="https://api.openai.com/v1" />
              <Input label="Embedding 模型名称" value={config.model.embedding_model} onChange={(e) => updateField("model", "embedding_model", e.target.value)} placeholder="text-embedding-3-small" />
              <Input label="Embedding API 密钥" type="password" value={config.model.embedding_api_key} onChange={(e) => updateField("model", "embedding_api_key", e.target.value)} placeholder={config.model.embedding_api_key_configured ? "已配置；输入新 key 可替换" : "留空表示未配置"} />
              {config.model.embedding_api_key_configured && <p className="text-xs text-[var(--success)]">已配置；页面不会回显原始 key，留空保存会保留现有 key。</p>}
              <Input label="Embedding 环境变量名" value={config.model.embedding_api_key_env} onChange={(e) => updateField("model", "embedding_api_key_env", e.target.value)} />
            </div>
            <div className="grid grid-cols-2 gap-3">
              <Input label="温度" type="number" value={String(config.model.temperature)} onChange={(e) => updateField("model", "temperature", parseFloat(e.target.value) || 0)} />
              <Input label="最大 Token" type="number" value={String(config.model.max_tokens)} onChange={(e) => updateField("model", "max_tokens", parseInt(e.target.value) || 0)} />
            </div>
            <Input label="超时 (ms)" type="number" value={String(config.model.invoke_timeout_ms)} onChange={(e) => updateField("model", "invoke_timeout_ms", parseInt(e.target.value) || 0)} />
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
                checked={config.skills.progressive_loading}
                onChange={(e) => updateField("skills", "progressive_loading", e.target.checked)}
                className="w-4 h-4 rounded accent-[var(--accent)]"
              />
            </div>
          </div>
        );

      case "subagents":
        return (
          <div className="space-y-4">
            <h3 className="font-semibold text-sm uppercase tracking-wider text-[var(--text-muted)]">子智能体配置</h3>
            <div>
              <label className="block text-sm font-medium mb-1">子智能体目录 (一行一个)</label>
              <textarea
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
            <h3 className="font-semibold text-sm uppercase tracking-wider text-[var(--text-muted)]">主题预设</h3>
            <div className="grid grid-cols-2 gap-3">
              {PRESET_META.map((p) => (
                <button
                  key={p.id}
                  onClick={() => theme.setPreset(p.id as PresetId)}
                  className={`text-left p-4 rounded-xl border transition-all ${
                    theme.theme.presetId === p.id
                      ? "border-[var(--accent)] bg-[var(--accent)]/10 ring-1 ring-[var(--accent)]/30"
                      : "border-[var(--border)] hover:border-[var(--text-faint)]/40"
                  }`}
                >
                  <div className="flex items-center gap-2 mb-1">
                    <Monitor className="w-4 h-4 text-[var(--accent)]" />
                    <span className="font-medium text-sm">{p.name}</span>
                  </div>
                  <p className="text-xs text-[var(--text-muted)]">{p.description}</p>
                </button>
              ))}
            </div>

            {/* Background */}
            <div>
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
              <div className="flex items-center gap-4 mt-3">
                <div className="flex items-center gap-2">
                  <label className="text-xs text-[var(--text-muted)]">模糊</label>
                  <input
                    type="range"
                    min="0"
                    max="40"
                    value={theme.theme.blur}
                    onChange={(e) => theme.updateTheme({ blur: parseInt(e.target.value) })}
                    className="w-24 accent-[var(--accent)]"
                  />
                  <span className="text-xs text-[var(--text-faint)] font-mono">{theme.theme.blur}px</span>
                </div>
                <div className="flex items-center gap-2">
                  <label className="text-xs text-[var(--text-muted)]">面板透明度</label>
                  <input
                    type="range"
                    min="30"
                    max="100"
                    value={Math.round(theme.theme.panelOpacity * 100)}
                    onChange={(e) => theme.updateTheme({ panelOpacity: parseInt(e.target.value) / 100 })}
                    className="w-24 accent-[var(--accent)]"
                  />
                  <span className="text-xs text-[var(--text-faint)] font-mono">{Math.round(theme.theme.panelOpacity * 100)}%</span>
                </div>
              </div>
              <div className="flex items-center gap-4 mt-2">
                <div className="flex items-center gap-2">
                  <label className="text-xs text-[var(--text-muted)]">字号</label>
                  <input
                    type="range"
                    min="11"
                    max="20"
                    value={theme.theme.fontSize}
                    onChange={(e) => theme.updateTheme({ fontSize: parseInt(e.target.value) })}
                    className="w-24 accent-[var(--accent)]"
                  />
                  <span className="text-xs text-[var(--text-faint)] font-mono">{theme.theme.fontSize}px</span>
                </div>
              </div>
            </div>

            {/* Custom CSS Vars */}
            <div>
              <h4 className="text-sm font-medium mb-2">自定义 CSS 变量 (JSON)</h4>
              <textarea
                value={JSON.stringify(theme.theme.customVars, null, 2)}
                onChange={(e) => {
                  try { theme.updateTheme({ customVars: JSON.parse(e.target.value) }); } catch { /* invalid JSON, ignore */ }
                }}
                rows={6}
                placeholder='{"--accent": "#b86135", "--success": "#527c62"}'
                className="w-full rounded-lg border border-[var(--border)] bg-[var(--input-bg)] px-3 py-2 text-xs text-[var(--text)] placeholder:text-[var(--text-faint)] focus:outline-none focus:ring-2 focus:ring-[var(--accent)]/40 resize-none font-mono"
              />
              <p className="mt-1 text-xs text-[var(--text-muted)]">
                白名单变量：{[
                  "--bg", "--bg-2", "--panel", "--panel-2", "--panel-hover",
                  "--text", "--text-muted", "--text-faint", "--border",
                  "--accent", "--accent-fg", "--input-bg",
                  "--success", "--warning", "--danger", "--nav", "--nav-text",
                  "--info", "--focus-ring", "--backdrop",
                ].join(", ")}
              </p>
            </div>

            {/* Import / Export / Reset */}
            <div className="flex items-center gap-3">
              <Button variant="secondary" size="sm" onClick={handleExport}>
                <Download className="w-4 h-4" />
                导出主题
              </Button>
              <Button variant="secondary" size="sm" onClick={handleImport}>
                <Upload className="w-4 h-4" />
                导入主题
              </Button>
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
    <div className="flex flex-col h-full">
      <PageHeader
        title="设置"
        description={saved ? "设置已保存" : "配置应用参数"}
        actions={
          saved ? (
            <Badge tone="success">
              <Check className="w-3.5 h-3.5" />
              已保存
            </Badge>
          ) : null
        }
      />

      <div className="flex-1 flex overflow-hidden">
        {/* Section tabs */}
        <div className="w-40 border-r border-[var(--border)] overflow-y-auto scrollbar-thin py-2 bg-[var(--panel)]/30">
          {SECTIONS.map((s) => (
            <button
              key={s.key}
              onClick={() => setActiveSection(s.key)}
              className={`w-full text-left px-4 py-2.5 text-sm transition-colors ${
                activeSection === s.key
                  ? "bg-[var(--accent)]/15 text-[var(--accent)] border-r-2 border-r-[var(--accent)] font-medium"
                  : "text-[var(--text-muted)] hover:bg-[var(--panel-hover)] hover:text-[var(--text)]"
              }`}
            >
              {s.label}
            </button>
          ))}
        </div>

        {/* Section content */}
        <div className="flex-1 overflow-y-auto p-6 scrollbar-thin">
          <div className="max-w-2xl">
            {renderSection()}
          </div>
        </div>
      </div>

      {/* Save bar */}
      <div className="border-t border-[var(--border)] px-6 py-3 bg-[var(--panel)]/60 flex items-center justify-between">
        <p className="text-xs text-[var(--text-faint)]">
          配置自动保存到数据库 (settings 表)
        </p>
        <Button onClick={handleSave}>
          <Check className="w-4 h-4" />
          保存设置
        </Button>
      </div>
    </div>
  );
}
