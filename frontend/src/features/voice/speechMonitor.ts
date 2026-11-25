export interface SpeechMonitor {
  stream: MediaStream;
  close: () => void;
}

export const VOICE_MICROPHONE_CONSTRAINTS: MediaStreamConstraints = {
  audio: { echoCancellation: true, noiseSuppression: true, autoGainControl: true },
};

export function createSpeechFilter(context: AudioContext): BiquadFilterNode {
  const filter = context.createBiquadFilter();
  filter.type = "highpass";
  filter.frequency.value = 85;
  filter.Q.value = 0;
  return filter;
}

export async function openSpeechMonitor(signal: AbortSignal, sample: (rms: number, time: number) => void): Promise<SpeechMonitor | null> {
  if (signal.aborted) return null;
  const stream = await navigator.mediaDevices.getUserMedia(VOICE_MICROPHONE_CONSTRAINTS);
  if (signal.aborted) {
    stream.getTracks().forEach((track) => track.stop());
    return null;
  }
  let context: AudioContext | null = null;
  let source: MediaStreamAudioSourceNode | null = null;
  let filter: BiquadFilterNode | null = null;
  let analyser: AnalyserNode | null = null;
  let sampleTimer:ReturnType<typeof setInterval>|null=null;
  let closed = false;
  const close = () => {
    if (closed) return;
    closed = true;
    signal.removeEventListener("abort", close);
    if(sampleTimer!==null)clearInterval(sampleTimer);
    source?.disconnect();
    filter?.disconnect();
    analyser?.disconnect();
    stream.getTracks().forEach((track) => track.stop());
    if (context) void context.close().catch(() => undefined);
  };
  signal.addEventListener("abort", close, { once: true });
  try {
    context = new AudioContext();
    source = context.createMediaStreamSource(stream);
    filter = createSpeechFilter(context);
    analyser = context.createAnalyser();
    analyser.fftSize = 1024;
    source.connect(filter);
    filter.connect(analyser);
    // Deliberately never connect the microphone to context.destination.
    await context.resume();
    if (closed) return null;
    const samples = new Float32Array(analyser.fftSize);
    const tick = () => {
      const time=performance.now();
      if (closed) return;
      analyser!.getFloatTimeDomainData(samples);
      const rms = Math.sqrt(samples.reduce((sum, value) => sum + value * value, 0) / samples.length);
      sample(rms, time);

    };
    sampleTimer=setInterval(tick,40);
    return { stream, close };
  } catch (error) {
    close();
    throw error;
  }
}
