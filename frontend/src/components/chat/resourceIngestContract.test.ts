import { describe, expect, it } from "vitest";
// @ts-expect-error -- Node file access is test-only and not bundled.
import { readFileSync } from "node:fs";

describe("chat resource ingest entry", () => {
  it("uses the browser file permission boundary and Resource API", () => {
    const source = readFileSync(new URL("./ChatInput.tsx", import.meta.url), "utf8");
    expect(source).toContain('type="file"');
    expect(source).toContain("ingestResource(file)");
    expect(source).toContain("resource.id");
  });
});
