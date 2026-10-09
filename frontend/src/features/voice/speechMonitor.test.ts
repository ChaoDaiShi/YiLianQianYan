import { afterEach, describe, expect, it, vi } from "vitest";
import { openSpeechMonitor } from "./speechMonitor";

afterEach(() => vi.unstubAllGlobals());

function environment() {
  const track = { stop: vi.fn() };
  const stream = { getTracks: () => [track] } as unknown as MediaStream;
  const getUserMedia = vi.fn().mockResolvedValue(stream);
  const source = { connect: vi.fn(), disconnect: vi.fn() };
  const analyser = { fftSize: 1024, disconnect: vi.fn(), getFloatTimeDomainData: (data: Float32Array) => data.fill(0.1) };
  const close = vi.fn().mockResolvedValue(undefined);
  vi.stubGlobal("navigator", { mediaDevices: { getUserMedia } });
  vi.stubGlobal("AudioContext", class {
    createMediaStreamSource = () => source;
    createAnalyser = () => analyser;
    resume = vi.fn().mockResolvedValue(undefined);
    close = close;
  });
  const frames: FrameRequestCallback[] = [];
  vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => frames.push(callback));
  vi.stubGlobal("cancelAnimationFrame", vi.fn());
  return { stream, track, getUserMedia, close, frames, source, analyser };
}

describe("browser speech monitor resources", () => {
  it("samples without connecting microphone to speakers and releases resources on abort", async () => {
    const env = environment();
    const controller = new AbortController();
    const sample = vi.fn();
    const monitor = await openSpeechMonitor(controller.signal, sample);
    expect(monitor?.stream).toBe(env.stream);
    env.frames.shift()!(100);
    expect(sample).toHaveBeenCalledOnce();
    expect(env.source.connect).toHaveBeenCalledWith(env.analyser);
    controller.abort();
    expect(env.track.stop).toHaveBeenCalledOnce();
    expect(env.close).toHaveBeenCalledOnce();
    env.frames.shift()!(200);
    expect(sample).toHaveBeenCalledOnce();
    monitor!.close();
    expect(env.close).toHaveBeenCalledOnce();
  });

  it("stops a late microphone grant after the user has switched mode off", async () => {
    const env = environment();
    const controller = new AbortController();
    env.getUserMedia.mockImplementationOnce(async () => {
      controller.abort();
      return env.stream;
    });
    expect(await openSpeechMonitor(controller.signal, vi.fn())).toBeNull();
    expect(env.track.stop).toHaveBeenCalledOnce();
    expect(env.frames).toHaveLength(0);
  });
});
