import { useState, useEffect } from "react";
import { Check, Save } from "lucide-react";
import { getSettings, updateSettings, getIsolationStatus, listSecurityGrants, type IsolationStatus, type SecurityGrant } from "../../../api/client";
import { verifyProviderConnection, type ProviderConnectionKind } from "../../../api/providerConnection";
import { useTheme } from "../../../theme";
import type { AppConfig, ProviderReadinessProjection } from "../../../types";
import { Button, Badge, Input, Modal } from "../../../components/ui";
import { ModuleSettingsEditor } from "../../../components/system/ModuleSetup";
import { useGlobalVoiceContext } from "../../voice/GlobalVoiceHost";
import { defaultConfig } from "../model/config";
import { comparableConfig } from "../model/mapping";
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

const editorTitles: Record<string, string> = {
  model: "对话模型与模型池", stt: "语音识别", tts: "语音合成", search: "联网搜索",
  vision: "图像识别", image: "图像生成", embedding: "向量模型", agent: "身份与工作区",
  compaction: "上下文压缩", permissions: "权限与审批", sandbox: "执行沙箱",
  extensions: "技能目录", appearance: "外观与背景", navigation: "系统模块与导航",
};
interface SettingsEditorDialogProps {
  nodeKey: string | null;
  onClose: () => void;
  onSaved?: () => void;
}

/** Opens one real settings form; a new node receives a fresh settings draft. */
export default function SettingsEditorDialog(props: SettingsEditorDialogProps) {
  return props.nodeKey === null ? null : <SettingsEditor key={props.nodeKey} {...props} nodeKey={props.nodeKey} />;
}

function SettingsEditor({ nodeKey, onClose, onSaved }: SettingsEditorDialogProps & { nodeKey: string }) {
  const usesSettingsDraft = ["model", "stt", "tts", "embedding", "agent", "compaction", "permissions", "sandbox", "extensions"].includes(nodeKey);
  const [loaded, setLoaded] = useState(false);
  const [config, setConfig] = useState<ProofLoopSettingsConfig>(defaultConfig);
  const [saved, setSaved] = useState(false);
  const [savedSnapshot, setSavedSnapshot] = useState(() => comparableConfig(defaultConfig));
  const [lastSavedConfig, setLastSavedConfig] = useState<ProofLoopSettingsConfig>(defaultConfig);
  const [saveError, setSaveError] = useState("");
  const [saving, setSaving] = useState(false);
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
    getIsolationStatus().then((s) => { if (s) setIsolation(s); });
    listSecurityGrants().then((items) => { if (items) setGrants(items); });
  }, [usesSettingsDraft]);

  const refreshGrants = async () => {
    const items = await listSecurityGrants();
    if (items) setGrants(items);
  };

  const handleSave = async () => {
    if (!loaded) return;
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
      onSaved?.();
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
    switch (nodeKey) {
      case "model":
        return <ModelSettings dirty={isUnsaved} config={config} updateField={updateField} clearSecret={clearSecret} secretRefreshToken={secretRefreshToken} readiness={providerReadiness} testing={testingProvider} results={providerTestResults} onVerify={(kind) => void testProvider(kind)} />;
      case "stt":
      case "tts":
        return <VoiceSettings side={nodeKey} dirty={isUnsaved} config={config} updateVoiceField={updateVoiceField} replaceVoiceKey={replaceVoiceKey} onReplaceVoiceKey={(side) => setReplaceVoiceKey((current) => ({ ...current, [side]: true }))} onClearVoiceSecret={clearVoiceSecret} readiness={providerReadiness} testing={testingProvider} results={providerTestResults} onVerify={(kind) => void testProvider(kind)} onPreviewTts={() => void previewTts()} />;
      case "search":
        return <SearchPanel />;
      case "vision":
      case "image":
        return <BasicServicePanel kind={nodeKey} />;
      case "embedding":
        return <div className="space-y-4">
          <Input label="Embedding API 地址" value={config.model.embedding_base_url} onChange={(event) => updateField("model", "embedding_base_url", event.target.value)} />
          <Input label="Embedding 模型名称" value={config.model.embedding_model} onChange={(event) => updateField("model", "embedding_model", event.target.value)} />
          <Input type="password" label="Embedding API Key" value={config.model.embedding_api_key ?? ""} onChange={(event) => updateField("model", "embedding_api_key", event.target.value)} />
          <Input label="凭据环境变量" value={config.model.embedding_api_key_env ?? ""} onChange={(event) => updateField("model", "embedding_api_key_env", event.target.value)} />
          <div className="flex gap-2">
            <Button onClick={() => void testProvider("embedding")} disabled={isUnsaved || testingProvider !== null}>测试连接</Button>
            <Button variant="secondary" onClick={() => void clearSecret("embedding_api_key")}>清除凭据</Button>
          </div>
          {providerTestResults.embedding && <p>{providerTestResults.embedding}</p>}
        </div>;
      case "agent":
        return <AgentSettings config={config} updateField={updateField} onAiRepairChange={(enabled) => setConfig((previous) => ({ ...previous, ai_repair_enabled: enabled }))} />;
      case "compaction":
        return <CompactionSettings config={config} updateField={updateField} />;
      case "permissions":
        return <PermissionSettings config={config} updateField={updateField} isolation={isolation} grants={grants} onGrantsChanged={refreshGrants} />;
      case "sandbox":
        return <SandboxSettings config={config} updateField={updateField} />;
      case "extensions":
        return <SkillPathSettings config={config} updateField={updateField} />;
      case "appearance":
        return <AppearanceSettings theme={theme} onBackgroundUpload={handleBgUpload} />;
      case "navigation":
        return <ModuleSettingsEditor onClose={onClose} onSaved={onSaved} />;
      default:
        return null;
    }
  };

  const saveStatus = saveError ? "保存失败" : usesSettingsDraft && !loaded ? "正在读取设置" : saved ? "已保存" : isUnsaved ? "有未保存修改" : "所有更改已保存";
  return <Modal open onClose={onClose} title={editorTitles[nodeKey]} size={nodeKey === "model" ? "lg" : "md"} className="settings-node-editor" footer={
    <div className="settings-header-actions flex w-full flex-wrap items-center justify-end gap-2">
      {usesSettingsDraft && <Badge tone={saveError ? "danger" : saved ? "success" : isUnsaved ? "warning" : "default"}>
        {saved ? <Check className="h-3.5 w-3.5" /> : null}{saveStatus}
      </Badge>}
      {usesSettingsDraft && isUnsaved && <Button variant="secondary" onClick={handleDiscard} disabled={saving || testingProvider !== null}>放弃修改</Button>}
      <Button variant="secondary" onClick={onClose}>关闭</Button>
      {usesSettingsDraft && <Button onClick={() => void handleSave()} disabled={!loaded || !isUnsaved || saving || testingProvider !== null} aria-label="保存设置">
        <Save className="h-4 w-4" />{saving ? "保存中…" : "保存设置"}
      </Button>}
    </div>
  }>
    <div className="settings-form space-y-4">
      {saveError && <p className="settings-save-error" role="alert">{saveError}</p>}
      {usesSettingsDraft && !loaded ? (!saveError && <p role="status">正在读取已保存设置…</p>) : renderSection()}
    </div>
  </Modal>;
}
