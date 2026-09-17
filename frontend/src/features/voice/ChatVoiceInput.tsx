import { Mic, Radio } from "lucide-react";
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
      title={onRequestGlobalVoiceSession ? "与小涟语音对话" : "语音会话正在接线"}
    >
      <Radio className="h-3.5 w-3.5" aria-hidden="true" />
      与小涟语音对话
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
        aria-label="开始语音输入"
        title="开始语音输入"
      >
        <Mic className="h-3.5 w-3.5" aria-hidden="true" />
        {listening ? "停止录音" : busy ? "正在处理" : "开始语音输入"}
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
        <button type="button" className="chat-voice-cancel" onClick={controls.cancel}>取消语音输入</button>
      ) : null}
      {globalEntry}
    </div>
  );
}
