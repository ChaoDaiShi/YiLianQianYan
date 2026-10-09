import { describe, expect, it } from "vitest";
import chatPageSource from "../../pages/ChatPage.tsx?raw";
import chatViewSource from "./ChatView.tsx?raw";
import voiceHostSource from "../../features/voice/GlobalVoiceHost.tsx?raw";

describe("mounted chat voice refresh contract", () => {
  it("binds a persisted voice completion to the matching open conversation", () => {
    expect(voiceHostSource).toContain("conversationRefresh");
    expect(chatPageSource).toContain("conversationRefresh");
    expect(chatPageSource).toContain("conversationRefreshRevision");
    expect(chatViewSource).toContain("conversationRefreshRevision");
    expect(chatViewSource).toContain("[conversationId, conversationRefreshRevision");
  });
});
