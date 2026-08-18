import { describe, expect, it } from "vitest";
import barrelSource from "./index.ts?raw";
import statusSource from "./SystemStatusBadge.tsx?raw";
import metricSource from "./SystemMetricCard.tsx?raw";
import sectionSource from "./SystemSection.tsx?raw";
import settingSource from "./SettingRow.tsx?raw";

describe("system component contract", () => {
  it("exports the shared system presentation components", () => {
    expect(barrelSource).toContain("SystemStatusBadge");
    expect(barrelSource).toContain("SystemMetricCard");
    expect(barrelSource).toContain("SystemSection");
    expect(barrelSource).toContain("SettingRow");
  });

  it("keeps the implementation token-based", () => {
    for (const source of [statusSource, metricSource, sectionSource, settingSource]) {
      expect(source).toContain("var(--");
    }
  });
});
