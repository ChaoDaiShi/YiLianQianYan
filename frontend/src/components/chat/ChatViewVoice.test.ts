import { describe, expect, it } from "vitest";
import source from "./ChatView.tsx?raw";
import pageSource from "../../pages/ChatPage.tsx?raw";
import messageListSource from "./MessageList.tsx?raw";

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

  it("wires the host bridge through ChatPage and forwards manual TTS to messages", () => {
    expect(pageSource).toContain("chatVoiceControls");
    expect(pageSource).toContain("requestGlobalVoiceSession");
    expect(pageSource).toContain("speakAssistantMessage");
    expect(source).toContain("onManualSpeech");
    expect(messageListSource).toContain("onSpeak");
    expect(messageListSource).toContain("<MessageBubble");
  });
});
