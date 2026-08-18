import { describe, expect, it } from "vitest";
import badgeSource from "./CapabilityStatusBadge.tsx?raw";
import detailSource from "./CapabilityDetailSection.tsx?raw";
import metaSource from "./CapabilityMetaRow.tsx?raw";
import indexSource from "./index.ts?raw";

describe("capability display components", () => {
  it("uses shared UI primitives and stays presentational", () => {
    expect(badgeSource).toContain('from "../ui"');
    expect(metaSource).not.toContain("api/client");
    expect(detailSource).not.toContain("api/client");
    expect(indexSource).toContain("CapabilityStatusBadge");
  });

  it("keeps details accessible and hides empty metadata rows", () => {
    expect(detailSource).toContain("aria-labelledby");
    expect(detailSource).toContain("<details");
    expect(metaSource).toContain("!value");
  });
});
