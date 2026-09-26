import type { ProviderConnectionKind } from "../../../api/providerConnection";
import { Button, Input } from "../../../components/ui";
import type { AppConfig, ProviderReadinessProjection } from "../../../types";
import { secretSourceLabel } from "../model/mapping";
import ProviderReadinessCard from "./ProviderReadinessCard";

interface VoiceSettingsProps {
  config: AppConfig;
  updateVoiceField: (side: "stt" | "tts", key: string, value: string | number) => void;
  replaceVoiceKey: { stt: boolean; tts: boolean };
  onReplaceVoiceKey: (side: "stt" | "tts") => void;
  onClearVoiceSecret: (side: "stt" | "tts") => Promise<void>;
  readiness: ProviderReadinessProjection | null;
  testing: ProviderConnectionKind | null;
  results: Partial<Record<ProviderConnectionKind, string>>;
  onVerify: (kind: ProviderConnectionKind) => void;
  onPreviewTts: () => void;
}

/**
 * STT and TTS are configured independently and each keeps its own credential.
 * A key is only accepted through the write-only password field; a configured
 * key is never echoed back, only its source label.
 */
export default function VoiceSettings({
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
  return (
    <div className="space-y-6">
      <ProviderReadinessCard readiness={readiness} kinds={["stt", "tts"]} testing={testing} results={results} onVerify={onVerify} onPreviewTts={onPreviewTts} />
      <p className="text-xs leading-5 text-[var(--text-faint)]">
        语音识别与语音合成独立配置；两个密钥均只写入系统凭据库，不会在页面中回显。
      </p>

      <section className="space-y-4 rounded-xl border border-[var(--border-soft)] p-4">
        <div>
          <h3 className="font-semibold text-sm text-[var(--text)]">语音识别（STT）</h3>
          <p className="mt-1 text-xs text-[var(--text-muted)]">将真实麦克风音频转换为最终文本。</p>
        </div>
        <Input label="Provider" value={config.voice.stt.provider} onChange={(e) => updateVoiceField("stt", "provider", e.target.value)} placeholder="openai-compatible" />
        <Input label="API 地址" value={config.voice.stt.base_url} onChange={(e) => updateVoiceField("stt", "base_url", e.target.value)} placeholder="https://api.openai.com/v1" />
        <Input label="模型" value={config.voice.stt.model} onChange={(e) => updateVoiceField("stt", "model", e.target.value)} placeholder="gpt-4o-mini-transcribe" />
        <Input label="语言" value={config.voice.stt.language} onChange={(e) => updateVoiceField("stt", "language", e.target.value)} placeholder="zh" />
        <div className="flex items-center justify-between gap-3 rounded-lg border border-[var(--border-soft)] bg-[var(--surface-muted)] px-3 py-2">
          <div>
            <p className="text-xs font-medium text-[var(--text)]">{config.voice.stt.api_key_configured ? "STT 凭据已配置" : "STT 凭据未配置"}</p>
            <p className="mt-0.5 text-meta text-[var(--text-muted)]">{secretSourceLabel(config.voice.stt.api_key_source)}</p>
          </div>
          <div className="flex gap-2">{config.voice.stt.api_key_configured ? <Button variant="secondary" size="sm" onClick={() => onReplaceVoiceKey("stt")}>替换密钥</Button> : null}{config.voice.stt.api_key_configured ? <Button variant="secondary" size="sm" onClick={() => void onClearVoiceSecret("stt")}>清除密钥</Button> : null}</div>
        </div>
        {(!config.voice.stt.api_key_configured || replaceVoiceKey.stt) && <Input label="STT API 密钥" type="password" value={config.voice.stt.api_key} onChange={(e) => updateVoiceField("stt", "api_key", e.target.value)} placeholder="输入后安全保存" />}
        <Input label="密钥环境变量名" value={config.voice.stt.api_key_env} onChange={(e) => updateVoiceField("stt", "api_key_env", e.target.value)} placeholder="OPENAI_API_KEY" />
        <Input label="请求超时（毫秒）" type="number" value={String(config.voice.stt.timeout_ms)} onChange={(e) => updateVoiceField("stt", "timeout_ms", Math.max(1, parseInt(e.target.value) || 0))} />
      </section>

      <section className="space-y-4 rounded-xl border border-[var(--border-soft)] p-4">
        <div>
          <h3 className="font-semibold text-sm text-[var(--text)]">语音合成（TTS）</h3>
          <p className="mt-1 text-xs text-[var(--text-muted)]">默认使用 MiniMax 原生语音接口与当前默认音色。</p>
        </div>
        <Input label="Provider" value={config.voice.tts.provider} onChange={(e) => updateVoiceField("tts", "provider", e.target.value)} placeholder="minimax" />
        <Input label="API 地址" value={config.voice.tts.base_url} onChange={(e) => updateVoiceField("tts", "base_url", e.target.value)} placeholder="https://api.minimax.io" />
        <Input label="模型" value={config.voice.tts.model} onChange={(e) => updateVoiceField("tts", "model", e.target.value)} placeholder="speech-2.8-turbo" />
        <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
          <Input label="默认音色" value={config.voice.tts.voice} onChange={(e) => updateVoiceField("tts", "voice", e.target.value)} placeholder="voice_id" />
          <Input label="语言" value={config.voice.tts.language} onChange={(e) => updateVoiceField("tts", "language", e.target.value)} placeholder="zh" />
        </div>
        <div className="flex items-center justify-between gap-3 rounded-lg border border-[var(--border-soft)] bg-[var(--surface-muted)] px-3 py-2">
          <div>
            <p className="text-xs font-medium text-[var(--text)]">{config.voice.tts.api_key_configured ? "TTS 凭据已配置" : "TTS 凭据未配置"}</p>
            <p className="mt-0.5 text-meta text-[var(--text-muted)]">{secretSourceLabel(config.voice.tts.api_key_source)}</p>
          </div>
          <div className="flex gap-2">{config.voice.tts.api_key_configured ? <Button variant="secondary" size="sm" onClick={() => onReplaceVoiceKey("tts")}>替换密钥</Button> : null}{config.voice.tts.api_key_configured ? <Button variant="secondary" size="sm" onClick={() => void onClearVoiceSecret("tts")}>清除密钥</Button> : null}</div>
        </div>
        {(!config.voice.tts.api_key_configured || replaceVoiceKey.tts) && <Input label="TTS API 密钥" type="password" value={config.voice.tts.api_key} onChange={(e) => updateVoiceField("tts", "api_key", e.target.value)} placeholder="输入后安全保存" />}
        <Input label="密钥环境变量名" value={config.voice.tts.api_key_env} onChange={(e) => updateVoiceField("tts", "api_key_env", e.target.value)} placeholder="留空时使用系统凭据库" />
        <Input label="请求超时（毫秒）" type="number" value={String(config.voice.tts.timeout_ms)} onChange={(e) => updateVoiceField("tts", "timeout_ms", Math.max(1, parseInt(e.target.value) || 0))} />
      </section>

      <p className="text-xs leading-5 text-[var(--text-muted)]">
        每个 Provider 独立判断可用性；任一侧配置不完整时，该侧保持 fail closed。
      </p>
    </div>
  );
}
