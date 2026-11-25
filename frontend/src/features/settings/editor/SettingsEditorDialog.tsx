import { useState, useEffect } from "react";
import { getSettings, updateSettings, getIsolationStatus, listSecurityGrants, type IsolationStatus, type SecurityGrant } from "../../../api/client";
import { verifyProviderConnection, type ProviderConnectionKind } from "../../../api/providerConnection";
import type { SettingsPendingApplication } from "../../../api/system";
import { useTheme } from "../../../theme";
import type { AppConfig, ProviderReadinessProjection } from "../../../types";
import { Button, Input, Tabs } from "../../../components/ui";
import { ModuleSettingsEditor } from "../../../components/system/ModuleSetup";
import { useGlobalVoiceContext } from "../../voice/GlobalVoiceHost";
import { defaultConfig } from "../model/config";
import { comparableConfig, rebaseConfigDraft } from "../model/mapping";
import type { ProofLoopSettingsConfig } from "../model/types";
import AgentSettings from "../sections/AgentSettings";
import AppearanceSettings from "../sections/AppearanceSettings";
import CompactionSettings from "../sections/CompactionSettings";
import ModelSettings from "../sections/ModelSettings";
import PermissionSettings from "../sections/PermissionSettings";
import SandboxSettings from "../sections/SandboxSettings";
import SkillPathSettings from "../sections/SkillPathSettings";
import VoiceSettings from "../sections/VoiceSettings";
import { SearchPanel } from "../sections/BasicConfiguration";
import BasicServicePanel from "../sections/BasicServicePanel";
import SettingsDialogLayout from "../sections/SettingsDialogLayout";
import ModelPoolPanel from "../../llm/ModelPoolPanel";
import type { ModelPool } from "../../llm/modelPools";

const editorTitles: Record<string, string> = {
  model: "模型配置", "model-pools": "高级模型池", stt: "语音识别", tts: "语音合成",
  embedding: "向量模型", agent: "智能体行为",
  compaction: "上下文管理", permissions: "权限与审批", sandbox: "执行沙箱",
  extensions: "扩展目录", appearance: "界面外观",
};
interface SettingsEditorDialogProps {
  nodeKey: string | null;
  onClose: () => void;
  onSaved?: () => void;
}

const modelPoolTabs: { id: string; label: string; kind: ModelPool["service_kind"] }[] = [
  { id: "model-pool-chat", label: "对话", kind: "chat" },
  { id: "model-pool-vision", label: "识图", kind: "vision" },
  { id: "model-pool-image", label: "生图", kind: "image" },
];

function ModelPoolSettings() {
  const [activeTab, setActiveTab] = useState(modelPoolTabs[0].id);
  return <div className="space-y-3">
    <Tabs tabs={modelPoolTabs} activeId={activeTab} onChange={setActiveTab} size="sm" aria-label="模型池服务类型" />
    {modelPoolTabs.map(({ id, kind }) => <div key={id} id={`panel-${id}`} role="tabpanel" aria-labelledby={`tab-${id}`} hidden={id !== activeTab}>
      <ModelPoolPanel serviceKind={kind} />
    </div>)}
  </div>;
}

/** Opens one real settings form; a new node receives a fresh settings draft. */
export default function SettingsEditorDialog(props: SettingsEditorDialogProps) {
  if (props.nodeKey === null) return null;
  if (props.nodeKey === "navigation") return <ModuleSettingsEditor onClose={props.onClose} onSaved={props.onSaved} />;
  if (props.nodeKey === "search") return <SearchPanel onClose={props.onClose} />;
  if (props.nodeKey === "vision" || props.nodeKey === "image") return <BasicServicePanel key={props.nodeKey} kind={props.nodeKey} onClose={props.onClose} />;
  return <SettingsEditor key={props.nodeKey} {...props} nodeKey={props.nodeKey} />;
}

function SettingsEditor({ nodeKey, onClose, onSaved }: SettingsEditorDialogProps & { nodeKey: string }) {
  const usesSettingsDraft = ["model", "stt", "tts", "embedding", "agent", "compaction", "permissions", "sandbox", "extensions"].includes(nodeKey);
  const [loaded, setLoaded] = useState(false);
  const [config, setConfig] = useState<ProofLoopSettingsConfig>(defaultConfig);
  const [saved, setSaved] = useState(false);
  const [pendingDirectoryRevisions, setPendingDirectoryRevisions] = useState<
    Partial<Record<SettingsPendingApplication["field"], number>>
  >({});
  const [savedSnapshot, setSavedSnapshot] = useState(() => comparableConfig(defaultConfig));
  const [lastSavedConfig, setLastSavedConfig] = useState<ProofLoopSettingsConfig>(defaultConfig);
  const [saveError, setSaveError] = useState("");
  const [saving, setSaving] = useState(false);
  const [needsReload, setNeedsReload] = useState(false);
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
    if (usesSettingsDraft) void getSettings().then((c) => {
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
        setLoaded(true);
      } else {
        setSaveError("无法读取设置，请关闭后重新打开此节点。");
      }
    }).catch(() => setSaveError("无法读取设置，请关闭后重新打开此节点。"));
    if (nodeKey === "permissions") {
      void getIsolationStatus().then((s) => { if (s) setIsolation(s); });
      void listSecurityGrants().then((items) => { if (items) setGrants(items); });
    }
  }, [nodeKey, usesSettingsDraft]);

  const refreshGrants = async () => {
    const items = await listSecurityGrants();
    if (items) setGrants(items);
  };

  const showSaveFailure = (result: Awaited<ReturnType<typeof updateSettings>> | null) => {
    setSaved(false);
    if (result?.code === "revision_conflict") {
      setNeedsReload(true);
      setSaveError("全局设置已在其他位置更新。重新加载后会保留你的修改，请核对后保存。");
    } else setSaveError(result?.error || "设置暂时无法保存");
  };

  const reloadSettings = async () => {
    setSaving(true);
    const fresh = await getSettings();
    if (fresh) {
      const latest = { ...defaultConfig, ...fresh };
      setConfig(rebaseConfigDraft(lastSavedConfig, config, latest));
      setLastSavedConfig(latest);
      setSavedSnapshot(comparableConfig(latest));
      setProviderReadiness(fresh.provider_readiness ?? null);
      setNeedsReload(false);
      setSaveError("");
      setSaved(false);
    } else setSaveError("无法读取最新设置，你的修改仍已保留。");
    setSaving(false);
  };

  const handleSave = async () => {
    if (!loaded || needsReload) return;
    setSaving(true);
    setSaveError("");
    setProviderTestResults({});
    // Keep line breaks editable in the draft and omit empty entries from the saved path lists.
    let submittedConfig = config;
    if (nodeKey === "sandbox") {
      submittedConfig = { ...config, sandbox: {
        ...config.sandbox,
        writable_paths: config.sandbox.writable_paths.filter(Boolean),
        denied_write_paths: config.sandbox.denied_write_paths.filter(Boolean),
      } };
    } else if (nodeKey === "extensions") {
      submittedConfig = {
        ...config,
        skills: { ...config.skills, directories: config.skills.directories.filter(Boolean) },
        subagents: { ...config.subagents, directories: config.subagents.directories.filter(Boolean) },
      };
    }
    const result = await updateSettings(submittedConfig);
    if (result && !result.error) {
      if (result.pending_application?.length && typeof result.revision === "number") {
        const revision = result.revision;
        const pending = result.pending_application;
        setPendingDirectoryRevisions(current => ({
          ...current,
          ...Object.fromEntries(pending.map(action => [action.field, revision])),
        }));
      }
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
            ...submittedConfig,
            revision: result.revision,
            voice: {
              stt: { ...submittedConfig.voice.stt, api_key: "", clear_api_key: undefined },
              tts: { ...submittedConfig.voice.tts, api_key: "", clear_api_key: undefined },
            },
          };
      setConfig(nextConfig);
      setProviderReadiness(fresh?.provider_readiness ?? nextConfig.provider_readiness ?? null);
      setSavedSnapshot(comparableConfig(nextConfig));
      setLastSavedConfig(nextConfig);
      setSecretRefreshToken((value) => value + 1);
      setReplaceVoiceKey({ stt: false, tts: false });
      setSaved(true);
      setNeedsReload(false);
      onSaved?.();
      window.setTimeout(() => setSaved(false), 2500);
    } else {
      showSaveFailure(result);
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
      showSaveFailure(result);
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
    onSaved?.();
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
      showSaveFailure(result);
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
    onSaved?.();
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

  const renderSection = () => {
    switch (nodeKey) {
      case "stt":
      case "tts":
        return <VoiceSettings side={nodeKey} dirty={isUnsaved} config={config} updateVoiceField={updateVoiceField} replaceVoiceKey={replaceVoiceKey} onReplaceVoiceKey={(side) => setReplaceVoiceKey((current) => ({ ...current, [side]: true }))} onClearVoiceSecret={clearVoiceSecret} readiness={providerReadiness} testing={testingProvider} results={providerTestResults} onVerify={(kind) => void testProvider(kind)} onPreviewTts={() => void previewTts()} />;
      case "model-pools":
        return <ModelPoolSettings />;
      case "embedding": {
        const result = providerTestResults.embedding;
        const testFailed = Boolean(result && result !== "连接正常");
        return <div className="space-y-3">
          <div className="flex flex-wrap gap-2" aria-label="本地向量模型预设">
            {["BAAI/bge-small-zh-v1.5", "Qwen/Qwen3-Embedding-0.6B"].map(model =>
              <Button key={model} size="sm" variant="secondary" onClick={() => {
                updateField("model", "embedding_model", model);
                updateField("model", "embedding_base_url", "http://127.0.0.1:8765/v1");
              }}>{model.startsWith("BAAI/") ? "本地 BGE 中文" : "本地 Qwen 多语"}</Button>
            )}
          </div>
          <p className="settings-help">本地预设需要先启动对应的向量服务，可脱离网络使用。</p>
          <Input label="模型名称" value={config.model.embedding_model} onChange={(event) => updateField("model", "embedding_model", event.target.value)} />
          <Input label="API 地址" value={config.model.embedding_base_url} onChange={(event) => updateField("model", "embedding_base_url", event.target.value)} />
          <Input type="password" autoComplete="new-password" label="API Key" value={config.model.embedding_api_key ?? ""}
            hint={config.model.embedding_api_key_configured ? "密钥已配置，留空沿用。" : undefined}
            onChange={(event) => updateField("model", "embedding_api_key", event.target.value)} />
          <div className="flex flex-wrap items-center gap-2">
            <Button size="sm" variant="secondary" onClick={() => void testProvider("embedding")} disabled={isUnsaved || testingProvider !== null || saving}>
              {testingProvider === "embedding" ? "正在测试…" : "测试连接"}
            </Button>
            {isUnsaved && <span className="settings-help">保存后可测试。</span>}
          </div>
          {result && (testFailed ? <p className="settings-save-error" role="alert">连接测试失败，请检查已保存的模型、地址和凭据。</p>
            : <p className="settings-help" role="status">{result}</p>)}
          <details className="u2-disclosure">
            <summary>高级选项</summary>
            <div className="space-y-3 pt-2">
              <Input label="凭据环境变量" value={config.model.embedding_api_key_env ?? ""} onChange={(event) => updateField("model", "embedding_api_key_env", event.target.value)} />
              <Button variant="secondary" size="sm" disabled={saving || testingProvider !== null} onClick={() => void clearSecret("embedding_api_key")}>清除凭据</Button>
              {testFailed && <p className="settings-help">测试错误代码：{result}</p>}
            </div>
          </details>
        </div>;
      }
      case "agent":
        return <AgentSettings config={config} linksDisabled={isUnsaved || saving} onAiRepairChange={(enabled) => {
          setSaveError("");
          setSaved(false);
          setConfig((previous) => ({ ...previous, ai_repair_enabled: enabled }));
        }} />;
      case "compaction":
        return <CompactionSettings config={config} updateField={updateField} />;
      case "permissions":
        return <PermissionSettings config={config} updateField={updateField} isolation={isolation} grants={grants} onGrantsChanged={refreshGrants} />;
      case "sandbox":
        return <SandboxSettings config={config} updateField={updateField} />;
      case "extensions": {
        const skillRevision = pendingDirectoryRevisions["skills.directories"];
        return <SkillPathSettings config={config} updateField={updateField}
          skillRefreshPending={skillRevision !== undefined}
          subagentRestartPending={pendingDirectoryRevisions["subagents.directories"] !== undefined}
          onSkillsRefreshed={() => setPendingDirectoryRevisions(current =>
            current["skills.directories"] === skillRevision
              ? { ...current, "skills.directories": undefined }
              : current
          )} />;
      }
      case "appearance":
        return <AppearanceSettings theme={theme} />;
      default:
        return null;
    }
  };

  const notice = usesSettingsDraft && loaded ? (saved ? "已保存" : isUnsaved ? "有未保存修改" : "") : "";
  const action = usesSettingsDraft && (nodeKey !== "model" || isUnsaved) ? (
    <Button onClick={() => void handleSave()} disabled={!loaded || !isUnsaved || needsReload || saving || testingProvider !== null} aria-label="保存设置">
      {saving ? "保存中…" : "保存设置"}
    </Button>
  ) : undefined;
  const secondaryActions = usesSettingsDraft ? <>
    {needsReload && <Button variant="secondary" onClick={() => void reloadSettings()} disabled={saving}>重新加载</Button>}
    {isUnsaved && <Button variant="secondary" onClick={handleDiscard} disabled={saving || testingProvider !== null}>放弃修改</Button>}
  </> : undefined;

  if (nodeKey === "model" && loaded) {
    return <ModelSettings onClose={onClose} error={saveError} notice={notice} action={action} secondaryActions={secondaryActions}
      dirty={isUnsaved} config={config} updateField={updateField} clearSecret={clearSecret} secretRefreshToken={secretRefreshToken}
      readiness={providerReadiness} testing={testingProvider} results={providerTestResults} onVerify={(kind) => void testProvider(kind)}
      onConfigurationChanged={(fresh) => {
        const nextConfig = { ...defaultConfig, ...fresh };
        setConfig(nextConfig);
        setLastSavedConfig(nextConfig);
        setSavedSnapshot(comparableConfig(nextConfig));
        setProviderReadiness(fresh.provider_readiness ?? null);
        setSecretRefreshToken(value => value + 1);
        onSaved?.();
      }} />;
  }

  return <SettingsDialogLayout title={editorTitles[nodeKey]} size={nodeKey === "model" ? "lg" : "md"} onClose={onClose} closeDisabled={saving} unsaved={isUnsaved}
    error={saveError} notice={notice} action={action} secondaryActions={secondaryActions}>
    {usesSettingsDraft && !loaded ? (!saveError && <p role="status" className="settings-help">正在读取已保存设置…</p>) : renderSection()}
  </SettingsDialogLayout>;
}
