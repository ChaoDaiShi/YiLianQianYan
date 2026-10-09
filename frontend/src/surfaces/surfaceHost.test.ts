import { describe, expect, it } from "vitest";
import { resolveSurfaceHostMode, surfaceContainsWorkspace } from "./surfaceHost";

describe("surface host contract", () => {
  it("defaults to the standalone host", () => {
    expect(resolveSurfaceHostMode()).toBe("standalone");
    expect(resolveSurfaceHostMode("unknown")).toBe("standalone");
  });

  it("allows the desktop skeleton without changing the workspace surface", () => {
    expect(resolveSurfaceHostMode("desktop-skeleton")).toBe("desktop-skeleton");
    expect(surfaceContainsWorkspace("standalone")).toBe(true);
    expect(surfaceContainsWorkspace("desktop-skeleton")).toBe(true);
  });
});
