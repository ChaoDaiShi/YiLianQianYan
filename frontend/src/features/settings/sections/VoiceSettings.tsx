import type { ProviderConnectionKind } from "../../../api/providerConnection";
import { Button, Input, Select } from "../../../components/ui";
import type { AppConfig, ProviderReadinessProjection } from "../../../types";
import { secretSourceLabel } from "../model/mapping";
import ProviderReadinessCard from "./ProviderReadinessCard";

type VoiceSide = "stt" | "tts";

const voiceProviders = {
  stt: [
    { value: "openai-compatible", label: "OpenAI-compatible", baseUrl: "https://api.openai.com/v1", model: "gpt-4o-mini-transcribe" },
    { value: "minimax", label: "MiniMax", baseUrl: "https://api.minimax.io/v1", model: "asr-1.0" },
    { value: "fish", label: "Fish Audio", baseUrl: "https://api.fish.audio/v1", model: "transcribe-1-pro" },
    { value: "aliyun", label: "阿里云百炼 / Qwen ASR", baseUrl: "", model: "qwen3-asr-flash" },
  ],
  tts: [
    { value: "openai-compatible", label: "OpenAI-compatible", baseUrl: "https://api.openai.com/v1", model: "gpt-4o-mini-tts" },
    { value: "minimax", label: "MiniMax", baseUrl: "https://api.minimax.io", model: "speech-2.8-turbo" },
    { value: "fish", label: "Fish Audio", baseUrl: "https://api.fish.audio/v1", model: "s2.1-pro" },
    { value: "aliyun", label: "阿里云百炼 / CosyVoice", baseUrl: "", model: "cosyvoice-v3-flash" },
    { value: "volcengine", label: "火山引擎 / 豆包语音合成", baseUrl: "https://openspeech.bytedance.com/api/v3", model: "seed-tts-2.0" },
  ],
};

interface VoiceSettingsProps {
  side: VoiceSide;
  dirty: boolean;
  config: AppConfig;
  updateVoiceField: (side: VoiceSide, key: string, value: string | number) => void;
  replaceVoiceKey: { stt: boolean; tts: boolean };
  onReplaceVoiceKey: (side: VoiceSide) => void;
  onClearVoiceSecret: (side: VoiceSide) => Promise<void>;
  readiness: ProviderReadinessProjection | null;
  testing: ProviderConnectionKind | null;
  results: Partial<Record<ProviderConnectionKind, string>>;
  onVerify: (kind: ProviderConnectionKind) => void;
  onPreviewTts: () => void;
}

export default function VoiceSettings({
  side,
  dirty,
  config,
  updateVoiceField,
  replaceVoiceKey,
  onReplaceVoiceKey,
  onClearVoiceSecret,
  readiness,
  testing,
  results,
  onVerify,
  onPreviewTts,
}: VoiceSettingsProps) {
  const voice = config.voice[side];
  const providers = voiceProviders[side];
  const synthesis = side === "tts";

  function selectProvider(id: string) {
    const preset = providers.find((item) => item.value === id);
    if (!preset) return;
    updateVoiceField(side, "provider", preset.value);
    updateVoiceField(side, "base_url", preset.baseUrl);
    updateVoiceField(side, "model", preset.model);
    if (synthesis) updateVoiceField(side, "voice", "");
  }

  return (
    <div className="space-y-4">
      <div className="grid gap-3 sm:grid-cols-2">
        <Select label={synthesis ? "语音合成厂商" : "语音识别厂商"} value={voice.provider} options={providers} onChange={(e) => selectProvider(e.target.value)} />
        <Input label="模型" value={voice.model} onChange={(e) => updateVoiceField(side, "model", e.target.value)} />
        {synthesis && <Input label="默认音色" value={config.voice.tts.voice} onChange={(e) => updateVoiceField(side, "voice", e.target.value)} placeholder="voice_id" />}
        <Input label="语言" value={voice.language} onChange={(e) => updateVoiceField(side, "language", e.target.value)} placeholder="zh" />
      </div>

      {voice.api_key_configured && (
        <div className="flex flex-wrap items-center justify-between gap-2 text-sm">
          <span>已配置 API 密钥</span>
          {!replaceVoiceKey[side] && <Button variant="secondary" size="sm" onClick={() => onReplaceVoiceKey(side)}>替换密钥</Button>}
        </div>
      )}
      {(!voice.api_key_configured || replaceVoiceKey[side]) && (
        <Input label={side.toUpperCase() + " API 密钥"} type="password" autoComplete="new-password" value={voice.api_key} onChange={(e) => updateVoiceField(side, "api_key", e.target.value)} />
      )}

      <ProviderReadinessCard kind={side} dirty={dirty} readiness={readiness} testing={testing} results={results} onVerify={onVerify} onPreviewTts={onPreviewTts} />

      <details className="u2-disclosure">
        <summary>高级选项</summary>
        <div className="space-y-3 pt-2">
          <Input label="API 地址" value={voice.base_url} onChange={(e) => updateVoiceField(side, "base_url", e.target.value)} />
          <Input label="密钥环境变量名" value={voice.api_key_env} onChange={(e) => updateVoiceField(side, "api_key_env", e.target.value)} />
          <Input label="请求超时（毫秒）" type="number" min={1} value={String(voice.timeout_ms)} onChange={(e) => updateVoiceField(side, "timeout_ms", Math.max(1, parseInt(e.target.value) || 0))} />
          {voice.api_key_configured && (
            <div className="flex flex-wrap items-center justify-between gap-2">
              <p className="settings-help">{secretSourceLabel(voice.api_key_source)}</p>
              <Button variant="secondary" size="sm" onClick={() => void onClearVoiceSecret(side)}>清除密钥</Button>
            </div>
          )}
        </div>
      </details>
    </div>
  );
}
