import { describe, expect, it } from "vitest";
import {
  nextConversationRefresh,
  type ConversationRefreshSignal,
} from "./GlobalVoiceHost";

describe("voice conversation refresh bridge", () => {
  it("advances only the completed continuation conversation", () => {
    const first = nextConversationRefresh(null, "conversation-a");
    expect(first).toEqual({ conversation_id: "conversation-a", revision: 1 });

    const second = nextConversationRefresh(first, "conversation-a");
    expect(second).toEqual({ conversation_id: "conversation-a", revision: 2 });

    const switched = nextConversationRefresh(second, "conversation-b");
    expect(switched).toEqual({ conversation_id: "conversation-b", revision: 1 });
  });

  it("rejects an empty conversation identity", () => {
    expect(() => nextConversationRefresh(null, "  ")).toThrow("conversation identity");
  });

  it("keeps the signal shape free of message content", () => {
    const signal: ConversationRefreshSignal = nextConversationRefresh(null, "conversation-a");
    expect(Object.keys(signal).sort()).toEqual(["conversation_id", "revision"]);
  });
});
