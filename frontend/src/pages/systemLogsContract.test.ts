import { describe, expect, it } from "vitest";
import source from "./LogsPage.tsx?raw";

describe("Logs contract", () => {
  it("keeps real fields and existing controls", () => {
    for (const field of ["timestamp", "level", "source", "message"]) {
      expect(source).toContain(field);
    }
    expect(source).toContain("getLogs");
    expect(source).toContain("drain: true");
  });

  it("adds local search and detail without invented raw fields", () => {
    expect(source).toContain("searchQuery");
    expect(source).toContain("Drawer");
    expect(source).toContain("暂时没有日志记录");
    expect(source).not.toContain("stack_trace");
    expect(source).not.toContain("raw_detail");
  });
});
