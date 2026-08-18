import { describe, expect, it } from "vitest";
import capabilitiesPageSource from "./CapabilitiesPage.tsx?raw";

describe("Capabilities capability surface", () => {
  it("uses the real registry search, filters, refresh, and detail API", () => {
    expect(capabilitiesPageSource).toContain("listCapabilities");
    expect(capabilitiesPageSource).toContain("getCapability");
    expect(capabilitiesPageSource).toContain("refreshCapabilities");
    expect(capabilitiesPageSource).toContain("q:");
    expect(capabilitiesPageSource).toContain("provider");
    expect(capabilitiesPageSource).toContain("status");
    expect(capabilitiesPageSource).toContain("aria-selected");
  });

  it("keeps capability management read-only and truthful", () => {
    expect(capabilitiesPageSource).toContain("ErrorState");
    expect(capabilitiesPageSource).toContain("Skeleton");
    expect(capabilitiesPageSource).toContain("permissions");
    expect(capabilitiesPageSource).not.toContain("执行能力");
    expect(capabilitiesPageSource).not.toContain("切换启用");
  });
});
