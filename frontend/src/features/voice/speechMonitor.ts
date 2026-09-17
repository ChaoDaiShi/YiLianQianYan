export interface SpeechMonitor {
  stream: MediaStream;
  close: () => void;
}

export async function openSpeechMonitor(signal: AbortSignal, sample: (rms: number, time: number) => void): Promise<SpeechMonitor | null> {
  if (signal.aborted) return null;
  const stream = await navigator.mediaDevices.getUserMedia({ audio: {
    echoCancellation: true, noiseSuppression: true, autoGainControl: false,
  } });
  if (signal.aborted) {
    stream.getTracks().forEach((track) => track.stop());
    return null;
  }
  let context: AudioContext | null = null;
  let source: MediaStreamAudioSourceNode | null = null;
  let analyser: AnalyserNode | null = null;
  let frame = 0;
  let closed = false;
  const close = () => {
    if (closed) return;
    closed = true;
    signal.removeEventListener("abort", close);
    cancelAnimationFrame(frame);
    source?.disconnect();
    analyser?.disconnect();
    stream.getTracks().forEach((track) => track.stop());
    if (context) void context.close().catch(() => undefined);
  };
  signal.addEventListener("abort", close, { once: true });
  try {
    context = new AudioContext();
    source = context.createMediaStreamSource(stream);
    analyser = context.createAnalyser();
    analyser.fftSize = 1024;
    source.connect(analyser);
    // Deliberately never connect the microphone to context.destination.
    await context.resume();
    if (closed) return null;
    const samples = new Float32Array(analyser.fftSize);
    const tick = (time: number) => {
      if (closed) return;
      analyser!.getFloatTimeDomainData(samples);
      const rms = Math.sqrt(samples.reduce((sum, value) => sum + value * value, 0) / samples.length);
      sample(rms, time);
      if (!closed) frame = requestAnimationFrame(tick);
    };
    frame = requestAnimationFrame(tick);
    return { stream, close };
  } catch (error) {
    close();
    throw error;
  }
}
