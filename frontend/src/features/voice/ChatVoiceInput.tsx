import { Mic, PhoneCall, Square, X, Loader2 } from "lucide-react";
import type { CaptureStatus } from "./useVoiceCapture";

export interface ChatVoiceControls {
  status: CaptureStatus;
  /** RMS-derived percentage from the active MediaRecorder source stream. */
  volume: number;
  start: () => void;
  stop: () => void;
  cancel: () => void;
  /** Host invokes this only after final MiniMax STT; cancellation never invokes it. */
  subscribeFinal: (listener: (text: string) => void) => () => void;
}

export interface ChatVoiceInputProps {
  controls: ChatVoiceControls | null;
  /** Requests the already-mounted GlobalVoiceSession; it never creates an alternate agent. */
  onRequestGlobalVoiceSession?: () => void;
}

export function ChatVoiceInput({ controls, onRequestGlobalVoiceSession }: ChatVoiceInputProps) {
  const globalEntry = (
    <button
      type="button"
      className="chat-voice-entry"
      onClick={onRequestGlobalVoiceSession}
      disabled={!onRequestGlobalVoiceSession}
      aria-label="与小涟语音对话"
      title={onRequestGlobalVoiceSession ? "与小涟通话" : "语音会话正在连接"}
    >
      <PhoneCall className="h-4 w-4" aria-hidden="true" />
    </button>
  );
  if (!controls) {
    return globalEntry;
  }

  const listening = controls.status === "listening";
  const busy = controls.status === "acquiring" || controls.status === "transcribing";
  const activeCapture = listening || busy;
  return (
    <div className="chat-voice-input" aria-live="polite">
      <button
        type="button"
        className="chat-voice-microphone"
        onClick={listening ? controls.stop : controls.start}
        disabled={busy}
        aria-label={listening ? "停止录音" : busy ? "正在处理语音" : "开始语音输入"}
        title={listening ? "停止录音" : busy ? "正在处理语音" : "开始语音输入"}
      >
        {listening ? <Square className="h-4 w-4" aria-hidden="true" /> : busy ? <Loader2 className="h-4 w-4 animate-spin" aria-hidden="true" /> : <Mic className="h-4 w-4" aria-hidden="true" />}
      </button>
      {listening ? (
        <>
          <span className="chat-voice-listening">正在听取</span>
          <span className="chat-voice-meter" aria-label={`音量 ${controls.volume}%`}>
            <span className="chat-voice-meter-bar">
              <span style={{ width: `${Math.max(0, Math.min(100, controls.volume))}%` }} />
            </span>
            <output>{controls.volume}%</output>
          </span>
        </>
      ) : null}
      {activeCapture ? (
        <button type="button" className="chat-voice-cancel" aria-label="取消语音输入" title="取消语音输入" onClick={controls.cancel}><X className="h-4 w-4" aria-hidden="true" /></button>
      ) : null}
      {globalEntry}
    </div>
  );
}
