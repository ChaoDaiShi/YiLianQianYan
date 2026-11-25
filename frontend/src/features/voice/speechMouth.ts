export const SPEECH_MOUTH_CHANNEL = "yilian:desktop-pet-speech-level";
export const SPEECH_MOUTH_ACTIVE = "yilian:desktop-pet-lip-sync";

/** Publish peak-normalized RMS from synthesized speech, without sharing audio. */
export function observeSpeechMouth(audio: HTMLAudioElement, bytes: ArrayBuffer): () => void {
  if (!window.__TAURI_INTERNALS__ || localStorage.getItem(SPEECH_MOUTH_ACTIVE) !== "true" ||
      typeof AudioContext === "undefined") return () => undefined;
  let context: AudioContext;
  try { context = new AudioContext(); } catch { return () => undefined; }
  const channel = new BroadcastChannel(SPEECH_MOUTH_CHANNEL);
  let levels: number[] = [];
  let timer: number | undefined;
  let stopped = false;
  const stop = () => {
    window.clearInterval(timer);
    timer = undefined;
    channel.postMessage(0);
  };
  const sample = () => channel.postMessage(audio.muted ? 0 : (levels[Math.floor(audio.currentTime * 20)] ?? 0));
  const start = () => {
    if (!stopped && timer === undefined) {
      sample();
      timer = window.setInterval(sample, 50);
    }
  };
  void context.decodeAudioData(bytes.slice(0)).then(decoded => {
    if (stopped) return;
    const samples = decoded.getChannelData(0);
    const frame = Math.max(1, Math.floor(decoded.sampleRate / 20));
    for (let begin = 0; begin < samples.length; begin += frame) {
      const end = Math.min(begin + frame, samples.length);
      let squares = 0;
      for (let index = begin; index < end; index++) squares += samples[index] ** 2;
      levels.push(Math.min(1, Math.sqrt(squares / (end - begin)) * 5));
    }
    if (!audio.paused && !audio.ended) start();
  }).catch(() => undefined).finally(() => { void context.close().catch(() => undefined); });
  audio.addEventListener("playing", start);
  audio.addEventListener("pause", stop);
  audio.addEventListener("ended", stop);
  audio.addEventListener("error", stop);
  return () => {
    stopped = true;
    audio.removeEventListener("playing", start);
    audio.removeEventListener("pause", stop);
    audio.removeEventListener("ended", stop);
    audio.removeEventListener("error", stop);
    stop();
    channel.close();
    levels = [];
  };
}
