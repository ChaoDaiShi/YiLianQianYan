import { describe, expect, it } from "vitest";
import { isPlaybackEcho, SpeechActivity } from "./speechActivity";

describe("hands-free speech activity and echo guard", () => {
  it("ends a spoken turn over steady noise without cutting a short pause", () => {
    const detector = new SpeechActivity();
    expect(detector.sample(0.14, 0, false, true)).toBeNull();
    expect(detector.sample(0.14, 140, false, true)).toBe("start");
    expect(detector.sample(0.032, 200, false, true)).toBeNull();
    expect(detector.sample(0.032, 600, false, true)).toBeNull();
    expect(detector.sample(0.11, 700, false, true)).toBeNull();
    expect(detector.sample(0.032, 900, false, true)).toBeNull();
    expect(detector.sample(0.032, 1700, false, true)).toBe("end");
    detector.sample(0.032, 1750, false, false);
    detector.sample(0.032, 1850, false, true);
    detector.sample(0.032, 2350, false, true);
    expect(detector.sample(0.11, 2450, false, true)).toBeNull();
    expect(detector.sample(0.11, 2600, false, true)).toBe("start");
  });

  it("recognizes sustained speech immediately after listening begins", () => {
    const detector = new SpeechActivity();
    expect(detector.sample(0.1, 0, false, true)).toBeNull();
    expect(detector.sample(0.1, 140, false, true)).toBe("start");
  });

  it("interrupts sustained speech during TTS, endpoints once, and waits for quiet before rearming", () => {
    const detector = new SpeechActivity();
    expect(detector.sample(0, 0, true, true)).toBeNull();
    expect(detector.sample(0, 500, true, true)).toBeNull();
    expect(detector.sample(0.15, 600, true, true)).toBeNull();
    expect(detector.sample(0.15, 740, true, true)).toBe("start");
    expect(detector.sample(0.1, 900, false, true)).toBeNull();
    expect(detector.sample(0.1, 980, false, true)).toBeNull();
    expect(detector.sample(0, 1000, false, true)).toBeNull();
    expect(detector.sample(0, 1800, false, true)).toBe("end");
    expect(detector.sample(0, 1900, false, false)).toBeNull();
    expect(detector.sample(0.2, 2400, false, true)).toBeNull();
    expect(detector.sample(0.2, 2600, false, true)).toBeNull();
  });

  it("discards a TTS-triggered burst when speech does not continue after playback stops", () => {
    const detector = new SpeechActivity();
    detector.sample(0, 0, true, true);
    detector.sample(0, 500, true, true);
    detector.sample(0.15, 600, true, true);
    expect(detector.sample(0.15, 740, true, true)).toBe("start");
    detector.sample(0.015, 800, false, true);
    expect(detector.sample(0.015, 1600, false, true)).toBe("discard");
  });

  it("ignores brief noise and low-level TTS leakage", () => {
    const detector = new SpeechActivity();
    detector.sample(0, 0, true, true);
    detector.sample(0, 500, true, true);
    expect(detector.sample(0.15, 600, true, true)).toBeNull();
    expect(detector.sample(0.005, 620, true, true)).toBeNull();
    expect(detector.sample(0.02, 900, true, true)).toBeNull();
    expect(detector.sample(0.02, 1300, true, true)).toBeNull();
  });

  it("suppresses own TTS transcript including punctuation and short approval echoes", () => {
    expect(isPlaybackEcho("继续执行！", "继续执行")).toBe(true);
    expect(isPlaybackEcho("同意", "请说同意或拒绝")).toBe(true);
    expect(isPlaybackEcho("打开任务列表", "刚才的任务已经暂停")).toBe(false);
  });
});
