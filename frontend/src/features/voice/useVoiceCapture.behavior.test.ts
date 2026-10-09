import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { GlobalVoiceSession, VoiceInputLease } from "../../api/voice";

const runtime = vi.hoisted(() => ({
  refs: [] as Array<{ current: unknown }>,
  states: [] as unknown[],
  effects: [] as Array<{ deps: unknown[]; cleanup?: () => void }>,
  cursor: 0,
  acquire: vi.fn(),
  transcribe: vi.fn(),
  recorder: null as FakeRecorder | null,
}));

vi.mock("react", () => ({
  useRef: <T,>(value: T) => {
    const index = runtime.cursor++;
    return runtime.refs[index] ??= { current: value } as { current: T };
  },
  useState: <T,>(initial: T) => {
    const index = runtime.cursor++;
    if (!(index in runtime.states)) runtime.states[index] = initial;
    return [runtime.states[index] as T, (value: T | ((current: T) => T)) => {
      const current = runtime.states[index] as T;
      runtime.states[index] = typeof value === "function"
        ? (value as (current: T) => T)(current)
        : value;
    }] as const;
  },
  useCallback: <T,>(callback: T) => callback,
  useEffect: (effect: () => void | (() => void), deps: unknown[]) => {
    const index = runtime.cursor++;
    const previous = runtime.effects[index];
    const changed = !previous || deps.some((value, depIndex) => value !== previous.deps[depIndex]);
    if (changed) {
      previous?.cleanup?.();
      const cleanup = effect();
      runtime.effects[index] = { deps, cleanup: typeof cleanup === "function" ? cleanup : undefined };
    }
  },
}));

vi.mock("../../api/voice", async () => {
  const actual = await vi.importActual<typeof import("../../api/voice")>("../../api/voice");
  return {
    ...actual,
    acquireVoiceInputLease: runtime.acquire,
    transcribeAudio: runtime.transcribe,
  };
});

import { useVoiceCapture } from "./useVoiceCapture";

class FakeRecorder {
  static isTypeSupported = vi.fn(() => true);
  state: RecordingState = "inactive";
  mimeType = "audio/ogg";
  ondataavailable: ((event: BlobEvent) => void) | null = null;
  onstop: (() => void) | null = null;
  onerror: (() => void) | null = null;

  constructor(_stream: MediaStream, _options: MediaRecorderOptions) {
    runtime.recorder = this;
  }

  start() { this.state = "recording"; }
  stop() {
    if (this.state === "inactive") return;
    this.state = "inactive";
    this.ondataavailable?.({ data: new Blob([new Uint8Array([1])]) } as BlobEvent);
    this.onstop?.();
  }
}

class FakeAudioContext {
  readonly close = vi.fn(async () => undefined);
  readonly analyser = {
    fftSize: 0,
    getFloatTimeDomainData: (samples: Float32Array) => samples.fill(0.1),
    disconnect: vi.fn(),
  };
  readonly source = { connect: vi.fn(), disconnect: vi.fn() };
  createMediaStreamSource = vi.fn(() => this.source);
  createAnalyser = vi.fn(() => this.analyser);
}

const session: GlobalVoiceSession = {
  voice_session_id: "voice-1",
  generation: 3,
  state: "listening",
  focused_surface: "conversation",
  attention_mode: "balanced",
  started_at: 1,
  updated_at: 1,
  schema_version: 1,
};

const lease: VoiceInputLease = {
  lease_id: "lease-1",
  voice_session_id: "voice-1",
  generation: 3,
  owner: "push_to_talk",
  acquired_at: 1,
};

function media() {
  const track = { stop: vi.fn() } as unknown as MediaStreamTrack;
  const stream = {
    getTracks: () => [track],
    clone: () => stream,
  } as unknown as MediaStream;
  return { stream, track };
}

function render(onFinal = vi.fn(), onError = vi.fn()) {
  runtime.cursor = 0;
  const capture = useVoiceCapture({ session, onFinal, onError });
  return { capture, onFinal, onError };
}

async function flush() {
  await Promise.resolve();
  await Promise.resolve();
}

describe("useVoiceCapture resource and cancellation behavior", () => {
  let contexts: FakeAudioContext[];
  let cancelFrame: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    runtime.refs = [];
    runtime.states = [];
    runtime.effects = [];
    runtime.cursor = 0;
    runtime.recorder = null;
    runtime.acquire.mockReset().mockResolvedValue(lease);
    runtime.transcribe.mockReset();
    contexts = [];
    cancelFrame = vi.fn();
    vi.stubGlobal("MediaRecorder", FakeRecorder);
    vi.stubGlobal("AudioContext", class extends FakeAudioContext {
      constructor() { super(); contexts.push(this); }
    });
    vi.stubGlobal("requestAnimationFrame", vi.fn(() => 41));
    vi.stubGlobal("cancelAnimationFrame", cancelFrame);
    vi.stubGlobal("window", { setTimeout: vi.fn(() => 42), clearTimeout: vi.fn() });
    vi.stubGlobal("navigator", { mediaDevices: { getUserMedia: vi.fn() } });
  });

  afterEach(() => {
    runtime.effects.forEach((effect) => effect.cleanup?.());
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  it("aborts acquisition before a lease can create capture or notify final", async () => {
    let resolveLease!: (value: VoiceInputLease | null) => void;
    let signal: AbortSignal | undefined;
    runtime.acquire.mockImplementation((_sessionId, _generation, _owner, nextSignal) => {
      signal = nextSignal;
      return new Promise((resolve) => { resolveLease = resolve; });
    });
    const { capture, onFinal } = render();
    const start = capture.start("user-click");
    await flush();

    capture.cancel();
    expect(signal?.aborted).toBe(true);
    expect(runtime.recorder).toBeNull();

    resolveLease(lease);
    await start;
    expect(onFinal).not.toHaveBeenCalled();
  });

  it("aborts an in-flight final transcription, releases media, and never notifies final", async () => {
    let resolveTranscription!: (value: unknown) => void;
    let signal: AbortSignal | undefined;
    runtime.transcribe.mockImplementation((_audio, _sessionId, _generation, _leaseId, nextSignal) => {
      signal = nextSignal;
      return new Promise((resolve) => { resolveTranscription = resolve; });
    });
    const { stream, track } = media();
    const { capture, onFinal } = render();

    await capture.start("user-click", stream);
    expect(runtime.recorder?.state).toBe("recording");
    runtime.recorder?.ondataavailable?.({ data: new Blob([new Uint8Array([1])]) } as BlobEvent);
    runtime.recorder?.onstop?.();
    await flush();
    expect(runtime.transcribe).toHaveBeenCalledOnce();

    capture.cancel();
    expect(signal?.aborted).toBe(true);
    expect(track.stop).toHaveBeenCalledOnce();
    expect(cancelFrame).toHaveBeenCalledWith(41);
    expect(contexts[0].close).toHaveBeenCalledOnce();

    resolveTranscription({ text: "不得插入" });
    await flush();
    expect(onFinal).not.toHaveBeenCalled();
  });

  it("releases analyser and tracks when recorder reports an error", async () => {
    const { stream, track } = media();
    const { capture, onError } = render();
    await capture.start("user-click", stream);

    runtime.recorder?.onerror?.();

    expect(onError).toHaveBeenCalledOnce();
    expect(track.stop).toHaveBeenCalledOnce();
    expect(cancelFrame).toHaveBeenCalledWith(41);
    expect(contexts[0].close).toHaveBeenCalledOnce();
  });

  it("releases analyser and tracks during hook unmount", async () => {
    const { stream, track } = media();
    const { capture } = render();
    await capture.start("user-click", stream);

    runtime.effects.forEach((effect) => effect.cleanup?.());

    expect(track.stop).toHaveBeenCalledOnce();
    expect(cancelFrame).toHaveBeenCalledWith(41);
    expect(contexts[0].close).toHaveBeenCalledOnce();
  });
});
