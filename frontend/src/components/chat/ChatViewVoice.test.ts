import { describe, expect, it } from "vitest";
import source from "./ChatView.tsx?raw";

describe("ChatView voice bridge contract", () => {
  it("passes the single global-voice capture bridge into the composer", () => {
    expect(source).toContain("chatVoiceControls");
    expect(source).toContain("onRequestGlobalVoiceSession");
    expect(source).toContain("<ChatInput");
  });

  it("does not manufacture a chat-specific session or agent", () => {
    expect(source).not.toContain("startVoiceSession(");
    expect(source).not.toContain("dispatchVoiceTurn(");
  });
});
