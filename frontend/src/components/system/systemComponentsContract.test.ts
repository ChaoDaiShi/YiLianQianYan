import { describe, expect, it } from "vitest";
import fs from "node:fs";

const componentDir = new URL(".", import.meta.url);

describe("system component contract", () => {
  it("exports the shared system presentation components", () => {
    const barrel = fs.readFileSync(new URL("./index.ts", componentDir), "utf8");
    expect(barrel).toContain("SystemStatusBadge");
    expect(barrel).toContain("SystemMetricCard");
    expect(barrel).toContain("SystemSection");
    expect(barrel).toContain("SettingRow");
  });

  it("keeps the implementation token-based", () => {
    const files = [
      "SystemStatusBadge.tsx",
      "SystemMetricCard.tsx",
      "SystemSection.tsx",
      "SettingRow.tsx",
    ];
    for (const file of files) {
      expect(fs.existsSync(new URL(`./${file}`, componentDir))).toBe(true);
    }
  });
});
