export class SpeechActivity {
  private armed = false;
  private speaking = false;
  private quietSince: number | null = null;
  private loudSince: number | null = null;
  private noiseFloor = 0.003;
  private startedAt = 0;
  private needsNearSpeech = false;
  private nearSpeechSince: number | null = null;
  private nearSpeechConfirmed = false;

  sample(rms: number, time: number, playing: boolean, ready: boolean): "start" | "end" | "discard" | null {
    if (!ready && !this.speaking) {
      this.armed = false;
      this.quietSince = null;
      this.loudSince = null;
      return null;
    }
    const threshold = Math.max(playing ? 0.09 : 0.025, this.noiseFloor * 4);
    const loud = rms >= threshold;
    if (this.speaking && !playing && time - this.startedAt >= 160) {
      if (loud) {
        this.nearSpeechSince ??= time;
        if (time - this.nearSpeechSince >= 80) this.nearSpeechConfirmed = true;
      } else {
        this.nearSpeechSince = null;
      }
    }
    if (!loud) {
      this.loudSince = null;
      this.quietSince ??= time;
      if (!playing && rms < 0.025) this.noiseFloor = this.noiseFloor * 0.98 + rms * 0.02;
      if (this.speaking && time - this.quietSince >= 800) {
        this.speaking = false;
        this.armed = false;
        this.quietSince = null;
        return this.needsNearSpeech && !this.nearSpeechConfirmed ? "discard" : "end";
      }
      if (!this.speaking && time - this.quietSince >= 500) this.armed = true;
    } else {
      this.quietSince = null;
      this.loudSince ??= time;
      if (this.armed && !this.speaking && time - this.loudSince >= 120) {
        this.speaking = true;
        this.armed = false;
        this.startedAt = time;
        this.needsNearSpeech = playing;
        this.nearSpeechSince = null;
        this.nearSpeechConfirmed = false;
        return "start";
      }
    }
    return null;
  }
}

export function isPlaybackEcho(transcript: string, spoken: string): boolean {
  const normalize = (text: string) => text.toLocaleLowerCase().replace(/[\p{P}\p{S}\s]/gu, "");
  const heard = normalize(transcript);
  const said = normalize(spoken);
  if (!heard || !said) return false;
  if (said.includes(heard) || (said.length >= 4 && heard.includes(said))) return true;
  const pairs = new Set(Array.from({ length: Math.max(0, said.length - 1) }, (_, i) => said.slice(i, i + 2)));
  const overlap = Array.from({ length: Math.max(0, heard.length - 1) }, (_, i) => heard.slice(i, i + 2))
    .filter((pair) => pairs.has(pair)).length;
  return heard.length >= 4 && overlap / (heard.length - 1) >= 0.7;
}
