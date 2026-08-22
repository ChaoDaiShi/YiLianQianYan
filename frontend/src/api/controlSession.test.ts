import { describe, expect, it, vi } from "vitest";

import { ControlSessionClient } from "./controlSession";

describe("ControlSessionClient", () => {
  it("uses the Tauri token provider when the desktop runtime is available", async () => {
    const invokeToken = vi.fn(async () => "a".repeat(64));
    const session = new ControlSessionClient();

    await session.initialize({
      tauriAvailable: true,
      environmentToken: "b".repeat(64),
      invokeToken,
    });

    expect(invokeToken).toHaveBeenCalledTimes(1);
    expect(session.headers()).toEqual({
      "X-Yilian-Control-Session": "a".repeat(64),
    });
  });

  it("uses an explicit environment token in browser development", async () => {
    const session = new ControlSessionClient();

    await session.initialize({
      tauriAvailable: false,
      environmentToken: "c".repeat(64),
    });

    expect(session.headers()).toEqual({
      "X-Yilian-Control-Session": "c".repeat(64),
    });
  });

  it("fails closed when no browser-development token is configured", async () => {
    const session = new ControlSessionClient();

    await expect(
      session.initialize({ tauriAvailable: false, environmentToken: "" }),
    ).rejects.toThrow("control session token is required");
    expect(() => session.headers()).toThrow("control session is not initialized");
  });

  it("rejects short tokens from either provider", async () => {
    const session = new ControlSessionClient();

    await expect(
      session.initialize({
        tauriAvailable: true,
        invokeToken: async () => "short",
      }),
    ).rejects.toThrow("at least 32 characters");
  });

  it("clears a previous token before a failed reinitialization", async () => {
    const session = new ControlSessionClient();
    await session.initialize({
      tauriAvailable: false,
      environmentToken: "d".repeat(64),
    });

    await expect(
      session.initialize({ tauriAvailable: false, environmentToken: "" }),
    ).rejects.toThrow("control session token is required");
    expect(() => session.headers()).toThrow("control session is not initialized");
  });
});
