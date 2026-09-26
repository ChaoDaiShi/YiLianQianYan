import { useState, useEffect } from "react";
import { useSearchParams } from "react-router-dom";
import { Check } from "lucide-react";
import { getSettings, updateSettings, getIsolationStatus, listSecurityGrants, type IsolationStatus, type SecurityGrant } from "../api/client";
import { verifyProviderConnection, type ProviderConnectionKind } from "../api/providerConnection";
import { useTheme } from "../theme";
import type { AppConfig, ProviderReadinessProjection } from "../types";
import { PageHeader, Button, Badge } from "../components/ui";
import { useGlobalVoiceContext } from "../features/voice/GlobalVoiceHost";
import { defaultConfig, SECTIONS } from "../features/settings/model/config";
import { comparableConfig } from "../features/settings/model/mapping";
import type { SectionKey } from "../features/settings/model/types";
import AgentSettings from "../features/settings/sections/AgentSettings";
import AppearanceSettings from "../features/settings/sections/AppearanceSettings";
import CompactionSettings from "../features/settings/sections/CompactionSettings";
import ModelSettings from "../features/settings/sections/ModelSettings";
import PermissionSettings from "../features/settings/sections/PermissionSettings";
import SandboxSettings from "../features/settings/sections/SandboxSettings";
import SkillPathSettings from "../features/settings/sections/SkillPathSettings";
import VoiceSettings from "../features/settings/sections/VoiceSettings";

/**
 * Settings page.
 *
 * Composition only: it owns the config being edited, the save/clear flows that
 * carry the rc.2 secret semantics, the deep link (`?section=`) and the shell.
 * Every section is a separate component under `features/settings/sections/`,
 * and the config shape, defaults and mappings live in
 * `features/settings/model/`.
 *
 * The credential flows deliberately stay here rather than in a `useSettings`
 * hook: they are the safety-sensitive part of this surface, and the contract
 * tests for them are source-text assertions that cannot catch a behavioural
 * regression introduced by extraction.
 */
export default function SettingsPage() {
  const [searchParams] = useSearchParams();
  const [config, setConfig] = useState<AppConfig>(defaultConfig);
  const [saved, setSaved] = useState(false);
  const [savedSnapshot, setSavedSnapshot] = useState(() => comparableConfig(defaultConfig));
  const [saveError, setSaveError] = useState("");
  const [saving, setSaving] = useState(false);
  const [activeSection, setActiveSection] = useState<SectionKey>("model");
  const [isolation, setIsolation] = useState<IsolationStatus | null>(null);
  const [grants, setGrants] = useState<SecurityGrant[]>([]);
  const [providerReadiness, setProviderReadiness] = useState<ProviderReadinessProjection | null>(null);
  const [secretRefreshToken, setSecretRefreshToken] = useState(0);
  const [replaceVoiceKey, setReplaceVoiceKey] = useState<{ stt: boolean; tts: boolean }>({ stt: false, tts: false });
  const [testingProvider, setTestingProvider] = useState<ProviderConnectionKind | null>(null);
  const [providerTestResults, setProviderTestResults] = useState<Partial<Record<ProviderConnectionKind, string>>>({});
  const { speakAssistantMessage } = useGlobalVoiceContext();
  const theme = useTheme();

  useEffect(() => {
    const requested = searchParams.get("section") as SectionKey | null;
    if (requested && SECTIONS.some((section) => section.key === requested)) {
      setActiveSection(requested);
    }
  }, [searchParams]);

  useEffect(() => {
    getSettings().then((c) => {
      if (c) {
        const nextConfig = {
          ...defaultConfig,
          ...c,
          model: { ...defaultConfig.model, ...c.model, api_key: c.model.api_key || "", embedding_api_key: c.model.embedding_api_key || "" },
          voice: {
            stt: { ...defaultConfig.voice.stt, ...c.voice?.stt, api_key: "" },
            tts: { ...defaultConfig.voice.tts, ...c.voice?.tts, api_key: "" },
          },
        };
        setConfig(nextConfig);
        setProviderReadiness(c.provider_readiness ?? null);
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
      const fresh = await getSettings();
      const nextConfig = fresh
        ? {
            ...defaultConfig,
            ...fresh,
            model: { ...defaultConfig.model, ...fresh.model, api_key: "", embedding_api_key: "" },
            voice: {
              stt: { ...defaultConfig.voice.stt, ...fresh.voice?.stt, api_key: "" },
              tts: { ...defaultConfig.voice.tts, ...fresh.voice?.tts, api_key: "" },
            },
          }
        : {
            ...config,
            voice: {
              stt: { ...config.voice.stt, api_key: "", clear_api_key: undefined },
              tts: { ...config.voice.tts, api_key: "", clear_api_key: undefined },
            },
          };
      setConfig(nextConfig);
      setProviderReadiness(fresh?.provider_readiness ?? nextConfig.provider_readiness ?? null);
      setSavedSnapshot(comparableConfig(nextConfig));
      setSecretRefreshToken((value) => value + 1);
      setReplaceVoiceKey({ stt: false, tts: false });
      setSaved(true);
      window.setTimeout(() => setSaved(false), 2500);
    } else {
      setSaved(false);
      setSaveError("设置暂时无法保存");
    }
    setSaving(false);
  };

  const clearSecret = async (field: "api_key" | "embedding_api_key") => {
    if (!window.confirm("确定清除已保存的 API 密钥？此操作会从系统凭据库删除该密钥。")) return;
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
      setProviderReadiness(fresh.provider_readiness ?? null);
      setSavedSnapshot(comparableConfig(nextConfig));
      setSecretRefreshToken((value) => value + 1);
    }
    setSaved(true);
    setSaveError("");
    window.setTimeout(() => setSaved(false), 2500);
  };

  const clearVoiceSecret = async (side: "stt" | "tts") => {
    if (!window.confirm(`确定清除已保存的 ${side.toUpperCase()} API 密钥？此操作会从系统凭据库删除该密钥。`)) return;
    const next = {
      ...config,
      voice: {
        ...config.voice,
        [side]: { ...config.voice[side], clear_api_key: true, api_key: "" },
      },
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
        voice: {
          stt: { ...defaultConfig.voice.stt, ...fresh.voice?.stt, api_key: "" },
          tts: { ...defaultConfig.voice.tts, ...fresh.voice?.tts, api_key: "" },
        },
      };
      setConfig(nextConfig);
      setProviderReadiness(fresh.provider_readiness ?? null);
      setSavedSnapshot(comparableConfig(nextConfig));
      setSecretRefreshToken((value) => value + 1);
      setReplaceVoiceKey((value) => ({ ...value, [side]: false }));
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

  const updateVoiceField = (side: "stt" | "tts", key: string, value: string | number) => {
    setSaveError("");
    setSaved(false);
    setConfig((prev) => ({
      ...prev,
      voice: {
        ...prev.voice,
        [side]: { ...prev.voice[side], [key]: value },
      },
    }));
  };

  const testProvider = async (kind: ProviderConnectionKind) => {
    setTestingProvider(kind);
    const result = await verifyProviderConnection(kind);
    setProviderTestResults((current) => ({
      ...current,
      [kind]: result.ok ? "连接测试完成" : result.error,
    }));
    setTestingProvider(null);
  };

  const previewTts = async () => {
    setTestingProvider("tts");
    const result = await verifyProviderConnection("tts");
    if (result.ok) {
      speakAssistantMessage("你好，我是小涟。");
      setProviderTestResults((current) => ({ ...current, tts: "试听已开始" }));
    } else {
      setProviderTestResults((current) => ({ ...current, tts: result.error }));
    }
    setTestingProvider(null);
  };

  const dirty = comparableConfig(config) !== savedSnapshot;

  const handleBgUpload = (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (!file) return;
    const reader = new FileReader();
    reader.onload = () => {
      theme.setBackgroundImage(reader.result as string);
    };
    reader.readAsDataURL(file);
  };

  const renderSection = () => {
    switch (activeSection) {
      case "model":
        return (
          <ModelSettings
            config={config}
            updateField={updateField}
            clearSecret={clearSecret}
            secretRefreshToken={secretRefreshToken}
            readiness={providerReadiness}
            testing={testingProvider}
            results={providerTestResults}
            onVerify={(kind) => void testProvider(kind)}
          />
        );

      case "voice":
        return (
          <VoiceSettings
            config={config}
            updateVoiceField={updateVoiceField}
            replaceVoiceKey={replaceVoiceKey}
            onReplaceVoiceKey={(side) => setReplaceVoiceKey((value) => ({ ...value, [side]: true }))}
            onClearVoiceSecret={clearVoiceSecret}
            readiness={providerReadiness}
            testing={testingProvider}
            results={providerTestResults}
            onVerify={(kind) => void testProvider(kind)}
            onPreviewTts={() => void previewTts()}
          />
        );

      case "agent":
        return <AgentSettings config={config} updateField={updateField} />;

      case "permissions":
        return (
          <PermissionSettings
            config={config}
            updateField={updateField}
            isolation={isolation}
            grants={grants}
            onGrantsChanged={refreshGrants}
          />
        );

      case "sandbox":
        return <SandboxSettings config={config} updateField={updateField} />;

      case "compaction":
        return <CompactionSettings config={config} updateField={updateField} />;

      case "skills":
        return <SkillPathSettings config={config} updateField={updateField} />;

      case "appearance":
        return <AppearanceSettings theme={theme} onBackgroundUpload={handleBgUpload} />;

      default:
        return null;
    }
  };

  return (
    <div className="system-settings-page page-canvas flex h-full min-h-0 flex-col">
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
        <div className="system-settings-content min-h-0 min-w-0 flex-1 overflow-x-hidden overflow-y-auto p-6 pb-16 scrollbar-thin">
          <div className="max-w-2xl space-y-6">
            {renderSection()}
            <section className="settings-save-card rounded-xl border border-[var(--border-soft)] bg-[var(--surface-muted)] p-4">
              <div className="flex flex-wrap items-center justify-between gap-3">
                <p className="text-xs text-[var(--text-faint)]">修改后点击保存设置；密钥输入在保存后会被清空并重新读取脱敏状态。</p>
                <Button onClick={handleSave} disabled={!dirty || saving} aria-label="保存设置">
                  <Check className="w-4 h-4" />
                  {saving ? "保存中…" : "保存设置"}
                </Button>
              </div>
            </section>
          </div>
        </div>
      </div>
    </div>
  );
}
