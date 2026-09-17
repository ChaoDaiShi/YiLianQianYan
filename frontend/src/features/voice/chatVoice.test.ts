import { describe, expect, it } from "vitest";
import {
  appendFinalTranscriptToComposer,
  shouldInsertFinalTranscript,
  voiceMeterPercent,
} from "./chatVoice";

describe("chat voice transcript contract", () => {
  it("inserts a final transcript into the draft without submitting it", () => {
    expect(appendFinalTranscriptToComposer("现有草稿", "识别结果")).toBe("现有草稿\n识别结果");
    expect(shouldInsertFinalTranscript("final", false)).toBe(true);
  });

  it("never inserts partial or cancelled capture results", () => {
    expect(shouldInsertFinalTranscript("partial", false)).toBe(false);
    expect(shouldInsertFinalTranscript("final", true)).toBe(false);
  });

  it("maps measured analyser volume to a bounded visual meter", () => {
    expect(voiceMeterPercent(0)).toBe(0);
    expect(voiceMeterPercent(0.12)).toBeGreaterThan(0);
    expect(voiceMeterPercent(10)).toBe(100);
  });
});
