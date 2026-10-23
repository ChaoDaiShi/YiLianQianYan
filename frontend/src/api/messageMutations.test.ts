import { afterEach, describe, expect, it, vi } from "vitest";
import { deleteConversationMessage, rollbackConversationMessage } from "./conversations";
import { requestResult } from "../core/api/http";

vi.mock("../core/api/http", () => ({ requestResult: vi.fn() }));
afterEach(() => vi.resetAllMocks());

describe("single message mutation transport", () => {
  it("targets a single message endpoint rather than deleting the conversation", async () => {
    vi.mocked(requestResult).mockResolvedValueOnce({ ok: true, data: { conversation: { id: "conversation" }, deleted_message_ids: ["reply"], removed_protocol_count: 0 } });
    await deleteConversationMessage("conversation", "reply");
    expect(requestResult).toHaveBeenCalledWith("DELETE", "/api/conversations/conversation/messages/reply", undefined, true);
  });
  it("retains the affected user identity from a rejected rollback without issuing another mutation", async () => {
    vi.mocked(requestResult).mockResolvedValueOnce({ ok: false, status: 409, error: "历史工具记录不完整", details: { code: "chat_history_protocol_invalid", affected_user_message_id: "earlier-user" } });
    await expect(rollbackConversationMessage("conversation", "user")).rejects.toMatchObject({ affectedUserMessageId: "earlier-user", code: "chat_history_protocol_invalid" });
    expect(requestResult).toHaveBeenCalledTimes(1);
  });
});
