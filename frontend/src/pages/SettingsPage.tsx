import { useState, useEffect } from "react";
import { useSearchParams } from "react-router-dom";
import { Check, HardDrive, Save } from "lucide-react";
import { getSettings, updateSettings, getIsolationStatus, listSecurityGrants, type IsolationStatus, type SecurityGrant } from "../api/client";
import { verifyProviderConnection, type ProviderConnectionKind } from "../api/providerConnection";
import { useTheme } from "../theme";
import type { AppConfig, ProviderReadinessProjection } from "../types";
import { PageHeader, Button, Badge } from "../components/ui";
import { useGlobalVoiceContext } from "../features/voice/GlobalVoiceHost";
import { defaultConfig } from "../features/settings/model/config";
import { comparableConfig } from "../features/settings/model/mapping";
import type { SectionKey, ProofLoopSettingsConfig } from "../features/settings/model/types";
import { SETTINGS_GROUPS, resolveSettingsGroup, settingsGroup } from "../features/settings/model/settingsNavigation";
import SettingsSectionFrame from "../features/settings/components/SettingsSectionFrame";
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
  const [config, setConfig] = useState<ProofLoopSettingsConfig>(defaultConfig);
  const [saved, setSaved] = useState(false);
  const [savedSnapshot, setSavedSnapshot] = useState(() => comparableConfig(defaultConfig));
  const [lastSavedConfig, setLastSavedConfig] = useState<ProofLoopSettingsConfig>(defaultConfig);
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
    setActiveSection(resolveSettingsGroup(searchParams.get("section")));
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
        setLastSavedConfig(nextConfig);
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
    setProviderTestResults({});
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
      setLastSavedConfig(nextConfig);
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

  const handleDiscard = () => {
    setConfig({
      ...lastSavedConfig,
      model: {
        ...lastSavedConfig.model,
        api_key: "",
        embedding_api_key: "",
        clear_api_key: undefined,
        clear_embedding_api_key: undefined,
      },
      voice: {
        stt: { ...lastSavedConfig.voice.stt, api_key: "", clear_api_key: undefined },
        tts: { ...lastSavedConfig.voice.tts, api_key: "", clear_api_key: undefined },
      },
    });
    setReplaceVoiceKey({ stt: false, tts: false });
    setSecretRefreshToken((value) => value + 1);
    setSaveError("");
    setProviderTestResults({});
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
      setLastSavedConfig(nextConfig);
      setSecretRefreshToken((value) => value + 1);
    }
    setSaved(true);
    setSaveError("");
    setProviderTestResults({});
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
      setLastSavedConfig(nextConfig);
      setSecretRefreshToken((value) => value + 1);
      setReplaceVoiceKey((value) => ({ ...value, [side]: false }));
    }
    setSaved(true);
    setSaveError("");
    setProviderTestResults({});
    window.setTimeout(() => setSaved(false), 2500);
  };

  const updateField = (section: keyof AppConfig, key: string, value: any) => {
    setSaveError("");
    setProviderTestResults({});
    setSaved(false);
    setConfig((prev: any) => ({
      ...prev,
      [section]: { ...prev[section], [key]: value },
    }));
  };

  const updateVoiceField = (side: "stt" | "tts", key: string, value: string | number) => {
    setSaveError("");
    setProviderTestResults({});
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
      [kind]: result.ok ? "连接正常" : result.error,
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
  const hasPendingSecretInput = Boolean(
    (config.model.api_key && config.model.api_key.trim().length > 0) ||
    (config.model.embedding_api_key && config.model.embedding_api_key.trim().length > 0) ||
    (config.voice.stt.api_key && config.voice.stt.api_key.trim().length > 0) ||
    (config.voice.tts.api_key && config.voice.tts.api_key.trim().length > 0)
  );
  const isUnsaved = dirty || hasPendingSecretInput;

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
    const frame = settingsGroup(activeSection);
    switch (activeSection) {
      case "model":
        return (
          <SettingsSectionFrame group={frame}>
            <div className="settings-card settings-card--wide settings-card--flush">
              <ModelSettings
                dirty={isUnsaved}
                config={config}
                updateField={updateField}
                clearSecret={clearSecret}
                secretRefreshToken={secretRefreshToken}
                readiness={providerReadiness}
                testing={testingProvider}
                results={providerTestResults}
                onVerify={(kind) => void testProvider(kind)}
              />
            </div>
          </SettingsSectionFrame>
        );

      case "voice":
        return (
          <SettingsSectionFrame group={frame}>
            <div className="settings-card settings-card--wide settings-card--flush">
              <VoiceSettings
                dirty={isUnsaved}
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
            </div>
          </SettingsSectionFrame>
        );

      case "agent":
        return <SettingsSectionFrame group={frame}>
          <AgentSettings config={config} updateField={updateField} onAiRepairChange={(enabled) => setConfig((previous) => ({ ...previous, ai_repair_enabled: enabled }))} />
          <CompactionSettings config={config} updateField={updateField} />
        </SettingsSectionFrame>;

      case "security":
        return (
          <SettingsSectionFrame group={frame}>
            <PermissionSettings
              config={config}
              updateField={updateField}
              isolation={isolation}
              grants={grants}
              onGrantsChanged={refreshGrants}
            />
            <SandboxSettings config={config} updateField={updateField} />
          </SettingsSectionFrame>
        );

      case "extensions":
        return <SettingsSectionFrame group={frame}>
          <SkillPathSettings config={config} updateField={updateField} />
        </SettingsSectionFrame>;

      case "appearance":
        return <SettingsSectionFrame group={frame}>
          <AppearanceSettings theme={theme} onBackgroundUpload={handleBgUpload} />
        </SettingsSectionFrame>;

      default:
        return null;
    }
  };

  const activeGroup = settingsGroup(activeSection);
  const saveStatus = saveError ? "保存失败" : saved ? "已保存" : isUnsaved ? "有未保存修改" : "所有更改已保存";

  return (
    <div className="ui-opt-settings system-settings-page page-canvas flex h-full min-h-0 flex-col">
      <PageHeader
        title="设置"
        description={activeGroup.description}
        actions={
          <div className="settings-header-actions">
            <Badge tone={saveError ? "danger" : saved ? "success" : isUnsaved ? "warning" : "default"}>
              {saved ? <Check className="h-3.5 w-3.5" /> : null}
              {saveStatus}
            </Badge>
            <Button onClick={handleSave} disabled={!isUnsaved || saving || testingProvider !== null} aria-label="保存设置">
              <Save className="h-4 w-4" />
              {saving ? "保存中…" : "保存设置"}
            </Button>
          </div>
        }
      />

      {saveError ? <div className="settings-save-error" role="alert">设置暂时无法保存，请检查连接后重试。</div> : null}

      <div className="system-settings-layout min-h-0 flex-1 overflow-hidden">
        {/* Section tabs */}
        <nav className="system-settings-nav overflow-y-auto scrollbar-thin" aria-label="设置分类">
          <div className="settings-nav-list">
          {SETTINGS_GROUPS.map((s) => {
            const Icon = s.icon;
            return (
            <button
              key={s.key}
              type="button"
              onClick={() => setActiveSection(s.key)}
              aria-current={activeSection === s.key ? "page" : undefined}
              className="settings-nav-item"
            >
              <span className="settings-nav-icon" data-accent={s.accent} aria-hidden="true"><Icon /></span>
              <span className="min-w-0">
                <span className="settings-nav-label">{s.label}</span>
                <span className="settings-nav-description">{s.navDescription}</span>
              </span>
            </button>
            );
          })}
          </div>
          <div className="settings-nav-note">
            <HardDrive aria-hidden="true" />
            <span><strong>本地优先</strong>密钥保存到系统凭据库，配置保留在本机。</span>
          </div>
        </nav>

        {/* Section content */}
        <div className="system-settings-content min-h-0 min-w-0 flex-1 overflow-x-hidden overflow-y-auto scrollbar-thin">
          <div className="settings-form">
            {renderSection()}
            {isUnsaved ? (
              <div className="ui-opt-settings-savebar" role="region" aria-label="未保存更改操作栏">
                <div className="ui-opt-settings-savebar-content">
                  <div className="ui-opt-settings-savebar-info">
                    <span className="ui-opt-savebar-dot" aria-hidden="true" />
                    <span className="ui-opt-settings-savebar-text">有未保存更改</span>
                  </div>
                  <div className="ui-opt-settings-savebar-actions">
                    <Button
                      variant="secondary"
                      size="sm"
                      onClick={handleDiscard}
                      disabled={saving || testingProvider !== null}
                    >
                      放弃修改
                    </Button>
                    <Button
                      size="sm"
                      onClick={handleSave}
                      disabled={saving || testingProvider !== null}
                      aria-label="保存设置"
                    >
                      <Save className="h-4 w-4" />
                      {saving ? "保存中…" : "保存设置"}
                    </Button>
                  </div>
                </div>
              </div>
            ) : null}
            {isUnsaved ? <div className="ui-opt-savebar-spacer" aria-hidden="true" /> : null}
          </div>
        </div>
      </div>
    </div>
  );
}
