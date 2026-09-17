import { beforeEach, describe, expect, it, vi } from "vitest";
import { useHandsFree } from "./useHandsFree";
import { openSpeechMonitor } from "./speechMonitor";
import { bindCaptureOperation, type CaptureOperationHandle } from "./captureOperation";

const hooks = vi.hoisted(() => ({ refs: [] as Array<{ current: unknown }>, cursor: 0, deps: null as unknown[] | null, cleanup: undefined as (() => void) | undefined }));
vi.mock("react", () => ({
  useRef: (value: unknown) => { const index = hooks.cursor++; return hooks.refs[index] ??= { current: value }; },
  useEffect: (effect: () => (() => void) | undefined, deps: unknown[]) => {
    if (!hooks.deps || deps.some((value, index) => value !== hooks.deps![index])) {
      hooks.cleanup?.(); hooks.deps = deps; hooks.cleanup = effect();
    }
  },
}));
vi.mock("./speechMonitor", () => ({ openSpeechMonitor: vi.fn() }));
beforeEach(() => { hooks.cleanup?.(); hooks.refs = []; hooks.cursor = 0; hooks.deps = null; hooks.cleanup = undefined; vi.resetAllMocks(); });

describe("hands-free effect coordination", () => {
  it.each([false, true])("binds deferred endpoint to its own operation across ordinary barge-in (manual replacement: %s)", async (replaced) => {
    let sample!: (rms: number, time: number) => void;
    vi.mocked(openSpeechMonitor).mockImplementation(async (_signal, callback) => {
      sample = callback; return { stream: {} as MediaStream, close: vi.fn() };
    });
    let operation = 1;
    let generation = 3;
    let finish!: (handle: CaptureOperationHandle) => void;
    const controls = { stop: vi.fn(), cancel: vi.fn() };
    const capture = {
      state: { status: "idle" as const, lease: null, error: null },
      start: vi.fn(() => new Promise<CaptureOperationHandle>((resolve) => { finish = resolve; })),
      stop: vi.fn(), cancel: vi.fn(),
    };
    const options = { enabled: true, sessionId: "voice", identityEpoch: 1, playing: false, capture, onError: vi.fn() };
    hooks.cursor = 0; useHandsFree(options); await Promise.resolve();
    sample(0, 0); sample(0, 500); sample(0.1, 600); sample(0.1, 740);
    generation = 4;
    const handle = bindCaptureOperation(() => operation === 1 && generation === 4, () => controls);
    if (replaced) operation = 2;
    const currentCapture = { ...capture, state: { ...capture.state, status: "listening" as const }, stop: vi.fn(), cancel: vi.fn() };
    hooks.cursor = 0; useHandsFree({ ...options, capture: currentCapture });
    sample(0, 800); sample(0, 1600);
    finish(handle); await Promise.resolve();
    expect(controls.stop).toHaveBeenCalledTimes(replaced ? 0 : 1);
    expect(currentCapture.stop).not.toHaveBeenCalled();
    expect(currentCapture.cancel).not.toHaveBeenCalled();
    expect(openSpeechMonitor).toHaveBeenCalledOnce();
  });

  it("an A endpoint settling after an explicit identity switch cannot stop B capture", async () => {
    const samples: Array<(rms: number, time: number) => void> = [];
    vi.mocked(openSpeechMonitor).mockImplementation(async (_signal, sample) => { samples.push(sample); return { stream: {} as MediaStream, close: vi.fn() }; });
    let finish!: () => void;
    const start = vi.fn(() => new Promise<void>((resolve) => { finish = resolve; }));
    const captureA = { state: { status: "idle" as const, lease: null, error: null }, start, stop: vi.fn(), cancel: vi.fn() };
    const options = { enabled: true, sessionId: "voice", identityEpoch: 1, playing: false, capture: captureA, onError: vi.fn() };
    hooks.cursor = 0; useHandsFree(options); await Promise.resolve();
    samples[0](0, 0); samples[0](0, 500); samples[0](0.1, 600); samples[0](0.1, 740);
    const captureB = { ...captureA, state: { ...captureA.state, status: "listening" as const }, stop: vi.fn(), cancel: vi.fn() };
    hooks.cursor = 0; useHandsFree({ ...options, identityEpoch: 2, capture: captureB }); await Promise.resolve();
    samples[0](0, 800); samples[0](0, 1600);
    finish(); await Promise.resolve(); await Promise.resolve();
    expect(captureB.stop).not.toHaveBeenCalled();
    expect(captureB.cancel).not.toHaveBeenCalled();
    expect(openSpeechMonitor).toHaveBeenCalledTimes(2);
  });
});
