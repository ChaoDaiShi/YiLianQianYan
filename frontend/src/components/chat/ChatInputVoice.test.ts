import { describe, expect, it } from "vitest";
import source from "./ChatInput.tsx?raw";

describe("ChatInput voice contract", () => {
  it("subscribes only to final chat capture and inserts it into the editable draft", () => {
    expect(source).toContain("subscribeFinal");
    expect(source).toContain("appendFinalTranscriptToComposer");
    expect(source).toContain("ChatVoiceInput");
  });

  it("does not auto-send a recognized transcript", () => {
    const finalSubscription = source.slice(source.indexOf("subscribeFinal"), source.indexOf("subscribeFinal") + 700);
    expect(finalSubscription).not.toContain("onSend(");
    expect(finalSubscription).not.toContain("void send(");
  });
});
