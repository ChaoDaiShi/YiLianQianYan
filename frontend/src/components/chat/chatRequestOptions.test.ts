import { describe, expect, it } from "vitest";
import { approvalModes, supportsThinking, buildChatRequestOptions } from "./chatRequestOptions";
describe("composer request options", () => {
  it("uses the three approval modes with persisted protocol values", () => {
    expect(approvalModes.map(mode => mode.label)).toEqual(["完全审批", "部分审批", "完全权限"]);
    expect(approvalModes.map(mode => mode.value)).toEqual(["full_approval", "partial_approval", "full_access"]);
  });
  it("sends an explicit model profile and thinking choice", () => {
    expect(buildChatRequestOptions("profile-42", "thinking")).toEqual({ model_profile_id: "profile-42", thinking_mode: "thinking" });
    expect(buildChatRequestOptions("", "standard")).toEqual({ thinking_mode: "standard" });
  });
  it("sends the selected workspace mode as an explicit request option", () => {
    expect(buildChatRequestOptions("", "standard", "create")).toEqual({ thinking_mode: "standard", workspace_mode: "create" });
  });
  it("only exposes thinking for supported provider protocols", () => {
    expect(supportsThinking("deepseek", "deepseek-v4-flash")).toBe(true);
    expect(supportsThinking("openai", "gpt-5.4")).toBe(true);
    expect(supportsThinking("openai", "gpt-4o")).toBe(false);
    expect(supportsThinking("custom", "unknown")).toBe(false);
  });
});
