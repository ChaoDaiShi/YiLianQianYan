import { describe, expect, it } from "vitest";
import { resolveAgentStatus } from "./agentStatus";

describe("resolveAgentStatus", () => {
  it("prioritizes the observable approval state", () => {
    expect(
      resolveAgentStatus({
        connection: "connected",
        hasApproval: true,
        isLoading: true,
        finished: false,
      }),
    ).toBe("approval");
  });

  it("shows finished only for the short done lifecycle", () => {
    expect(
      resolveAgentStatus({
        connection: "connected",
        hasApproval: false,
        isLoading: false,
        finished: true,
      }),
    ).toBe("finished");
  });

  it("falls back to idle when no run is active", () => {
    expect(
      resolveAgentStatus({
        connection: "idle",
        hasApproval: false,
        isLoading: false,
        finished: false,
      }),
    ).toBe("idle");
  });
});
