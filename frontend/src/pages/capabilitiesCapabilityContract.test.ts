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

  it("manages capabilities through their real source instead of inventing registry CRUD", () => {
    expect(capabilitiesPageSource).toContain("ErrorState");
    expect(capabilitiesPageSource).toContain("Skeleton");
    expect(capabilitiesPageSource).toContain("permissions");
    expect(capabilitiesPageSource).toContain("getCapabilityManagementTarget");
    expect(capabilitiesPageSource).toContain("新增能力来源");
    expect(capabilitiesPageSource).toContain("管理来源");
    expect(capabilitiesPageSource).not.toContain("执行能力");
    expect(capabilitiesPageSource).not.toContain("切换启用");
  });
});
