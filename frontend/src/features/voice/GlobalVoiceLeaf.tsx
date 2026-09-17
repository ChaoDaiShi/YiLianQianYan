export type GlobalVoiceProductState =
  | "listening"
  | "understanding"
  | "speaking"
  | "confirmation"
  | "error";

export interface GlobalVoiceLeafProps {
  state: GlobalVoiceProductState;
  expanded: boolean;
  heardText?: string;
  currentContext?: string;
  interpretedAction?: string;
  microphoneLabel: string;
  soundLabel: string;
  onToggleExpanded: () => void;
  onMicrophone: () => void;
  onSound: () => void;
  onEnd: () => void;
  microphoneDisabled?: boolean;
  soundDisabled?: boolean;
  endDisabled?: boolean;
  notice?: string | null;
}

const STATE_LABELS: Record<GlobalVoiceProductState, string> = {
  listening: "正在听取",
  understanding: "正在理解",
  speaking: "正在播报",
  confirmation: "等待确认",
  error: "语音暂不可用",
};

/**
 * Product-only voice surface. The host owns capture, playback and server
 * lifecycle; this leaf intentionally receives only user-visible state/actions.
 */
export function GlobalVoiceLeaf({
  state,
  expanded,
  heardText = "等待新的语音输入",
  currentContext = "当前页面",
  interpretedAction = "尚未解析操作",
  microphoneLabel,
  soundLabel,
  onToggleExpanded,
  onMicrophone,
  onSound,
  onEnd,
  microphoneDisabled = false,
  soundDisabled = false,
  endDisabled = false,
  notice = null,
}: GlobalVoiceLeafProps) {
  const label = STATE_LABELS[state];
  return (
    <aside className="global-voice-dock" aria-label="与小涟语音对话">
      <div className="global-voice-pill" data-testid="voice-pill" data-state={state}>
        <span className="global-voice-dot" data-state={state} aria-hidden="true" />
        <div className="global-voice-pill-copy">
          <strong>{label}</strong>
          <span>与小涟语音对话</span>
        </div>
        <button
          type="button"
          className="global-voice-expand"
          aria-expanded={expanded}
          aria-controls="global-voice-panel"
          onClick={onToggleExpanded}
        >
          {expanded ? "收起" : "展开"}
        </button>
      </div>

      {expanded ? (
        <section className="global-voice-panel" id="global-voice-panel" data-testid="voice-panel">
          <div className="global-voice-panel-heading">
            <div>
              <span className="global-voice-eyebrow">GLOBAL VOICE</span>
              <h2>与小涟语音对话</h2>
            </div>
            <span className="global-voice-presence" data-state={state}>{label}</span>
          </div>

          <dl className="global-voice-context-grid">
            <div><dt>当前上下文</dt><dd>{currentContext}</dd></div>
            <div><dt>理解到的操作</dt><dd>{interpretedAction}</dd></div>
          </dl>

          <div className="global-voice-transcript" aria-live="polite">
            <div><span>听到的话</span><p>{heardText}</p></div>
          </div>

          <div className="global-voice-controls">
            <button
              type="button"
              className="global-voice-primary"
              data-testid="voice-mic"
              onClick={onMicrophone}
              disabled={microphoneDisabled}
            >
              {microphoneLabel}
            </button>
            <button type="button" onClick={onSound} disabled={soundDisabled}>{soundLabel}</button>
            <button type="button" className="global-voice-end" onClick={onEnd} disabled={endDisabled}>
              结束会话
            </button>
          </div>
          {notice ? <p className="global-voice-notice" role="status">{notice}</p> : null}
        </section>
      ) : null}
    </aside>
  );
}
