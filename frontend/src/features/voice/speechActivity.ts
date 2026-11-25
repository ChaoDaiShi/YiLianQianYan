export class SpeechActivity {
  private armed = true;
  private speaking = false;
  private quietSince: number | null = null;
  private loudSince: number | null = null;
  private noiseFloor = 0.003;
  private speechLevel = 0;
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
    // Starting needs a clear rise above the room noise. Ending can also use
    // the drop from the current speaker's level, even over audible background.
    let threshold = Math.max(0.018, this.noiseFloor * 2.2);
    if (playing) {
      threshold = Math.max(0.09, this.noiseFloor * 4);
    } else if (this.speaking) {
      threshold = this.needsNearSpeech && !this.nearSpeechConfirmed
        ? Math.max(0.025, this.noiseFloor * 4)
        : Math.max(0.008, this.noiseFloor * 1.6, this.speechLevel * 0.3);
    }
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
      if (!playing && !this.speaking) this.noiseFloor = this.noiseFloor * 0.92 + rms * 0.08;
      if (this.speaking && time - this.quietSince >= 800) {
        this.speaking = false;
        this.armed = false;
        this.quietSince = null;
        if (!playing) this.noiseFloor = Math.max(this.noiseFloor, rms);
        return this.needsNearSpeech && !this.nearSpeechConfirmed ? "discard" : "end";
      }
      if (!this.speaking && time - this.quietSince >= 500) this.armed = true;
    } else {
      this.quietSince = null;
      this.loudSince ??= time;
      if (this.speaking && !playing) {
        this.speechLevel = Math.max(rms, this.speechLevel * 0.97);
      }
      if (this.armed && !this.speaking && time - this.loudSince >= 120) {
        this.speaking = true;
        this.armed = false;
        this.startedAt = time;
        this.speechLevel = playing ? 0 : rms;
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
