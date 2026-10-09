import { getVoiceSession, updateVoiceContext, type GlobalVoiceSession } from "../../api/voice";
import type { GlobalVoiceContextSnapshot } from "./GlobalVoiceHost";

interface ContextSyncOptions {
  session: GlobalVoiceSession;
  getContext: () => GlobalVoiceContextSnapshot;
  isCurrent: () => boolean;
  signal: AbortSignal;
  update?: typeof updateVoiceContext;
  reload?: typeof getVoiceSession;
}

export async function synchronizeVoiceContext(options: ContextSyncOptions): Promise<GlobalVoiceSession | null> {
  const update = options.update ?? updateVoiceContext;
  const reload = options.reload ?? getVoiceSession;
  const current = () => !options.signal.aborted && options.isCurrent();
  let authoritative = options.session;
  // Bounded recovery: a superseded request may have committed before abort.
  for (let attempt = 0; attempt < 3 && current(); attempt += 1) {
    const desired = options.getContext();
    const updated = await update({ session_id: authoritative.voice_session_id, generation: authoritative.generation, ...desired }, options.signal)
      .catch(() => null);
    if (!current()) return null;
    if (updated) {
      if (updated.voice_session_id !== authoritative.voice_session_id || updated.generation < authoritative.generation || updated.state === "ended") return null;
      authoritative = updated;
      if (options.getContext() === desired) return updated;
      continue;
    }
    const refreshed = await reload(options.signal).catch(() => null);
    if (!current()) return null;
    if (!refreshed?.session) throw new Error("语音上下文同步失败，请检查连接后重试。");
    if (refreshed.session.voice_session_id !== authoritative.voice_session_id
      || refreshed.session.generation < authoritative.generation || refreshed.session.state === "ended") return null;
    authoritative = refreshed.session;
  }
  if (current()) throw new Error("语音上下文仍在变化，请稍后重新开始输入。");
  return null;
}
