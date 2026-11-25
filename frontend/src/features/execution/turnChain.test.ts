import { describe, expect, it } from "vitest";
import type { Message } from "../../types";
import type { ExecutionRecord } from "./model";
import { projectTurnChain } from "./turnChain";

const turn = { id: "current", user_message_id: "user-current", status: "completed", started_at: 10, finished_at: 20 };
const message = (id: string, role: string, content: string, created_at: number): Message => ({ id, role, content, created_at });
function record(id: string, startedAt?: number): ExecutionRecord {
  return { toolCallId: id, conversationId: "conversation", name: "write_file", args: {}, riskLevel: "medium", approvalStatus: "approved", executionStatus: "succeeded", verificationStatus: "passed", sequence: 1, startedAt };
}

describe("latest chat chain", () => {
  it("matches the accepted user turn and excludes known actions from adjacent turns", () => {
    const old = { ...message("old-tool", "tool", "old", 10), tool_call_id: "old-action" };
    const user = message("user-current", "user", "weekly report", 10);
    const tool = { ...message("tool", "tool", "created", 12), tool_call_id: "current-action" };
    const reply = message("reply", "assistant", "done", 15);
    const next = message("next", "user", "next", 20);
    const nextTool = { ...message("next-tool", "tool", "next", 20), tool_call_id: "next-action" };
    const chain = projectTurnChain(turn, [user, reply, next], [record("old-action", 10), record("current-action"), record("next-action", 20)], [old, user, tool, reply, next, nextTool]);
    expect(chain?.actions.map(action => action.toolCallId)).toEqual(["current-action"]);
    expect(chain).toMatchObject({ request: user, response: reply, savedReply: reply, approved: 1, verified: 1 });
  });

  it("keeps a pending action visible before its execution timestamp exists", () => {
    const pending = { ...record("pending"), approvalStatus: "pending", executionStatus: "awaiting_approval", verificationStatus: "not_requested" } as const;
    const chain = projectTurnChain({ ...turn, finished_at: null }, [message("user-current", "user", "write", 10)], [pending], [], [{
      approval_id: "approval", conversation_id: "conversation", tool_call_id: "pending", tool_name: "write_file", arguments: {}, risk_level: "medium", reason: "confirm", status: "pending", created_at: new Date(11).toISOString(), expires_at: "",
    }]);
    expect(chain?.pending).toBe(1);
    expect(chain?.response).toBeUndefined();
  });

  it("uses the saved reply from the completed turn and excludes a tool preamble or streamed draft", () => {
    const user = message("user-current", "user", "write", 10);
    const preamble = { ...message("preamble", "assistant", "writing", 11), tool_calls: [{ toolCallId: "write", name: "write_file", args: {}, status: "running" }] } as Message;
    const result = message("result", "tool", "saved", 13);
    const draft = message("stream", "assistant", "streamed draft", 18);
    const saved = message("saved", "assistant", "finished answer", 17);
    const next = message("next", "user", "another question", 20);
    expect(projectTurnChain(turn, [user, draft], [], [user, preamble, result, saved, next])?.savedReply).toEqual(saved);
    expect(projectTurnChain(turn, [user, draft], [], [user, preamble, result])?.savedReply).toBeUndefined();
    expect(projectTurnChain({ ...turn, status: "running", finished_at: null }, [user, draft], [], [user, saved])?.savedReply).toBeUndefined();
  });
});
