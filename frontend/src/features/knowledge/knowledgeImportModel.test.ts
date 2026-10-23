import { describe, expect, it } from "vitest";
import { importResultPage, summarizeImport, type ImportNotice } from "./knowledgeImportModel";

describe("large folder import result presentation", () => {
  const rows: ImportNotice[] = Array.from({ length: 1990 }, (_, index) => ({ name: `${index}.txt`, path: `资料/章节/${index}.txt`, status: index % 10 === 0 ? "failed" : "imported", message: "stored result" }));
  it("shows at most 50 results while retaining all 1990 outcomes", () => {
    expect(importResultPage(rows, 0, false).rows).toHaveLength(50);
    const last = importResultPage(rows, 39, false);
    expect(last.pages).toBe(40);
    expect(last.rows).toHaveLength(40);
    expect(last.rows[last.rows.length - 1]?.path).toBe("资料/章节/1989.txt");
    expect(summarizeImport(rows)).toEqual({ imported: 1791, existing: 0, failed: 199, skipped: 0 });
  });
  it("filters failures before pagination and retains the original relative paths", () => {
    const result = importResultPage(rows, 100, true);
    expect(result.page).toBe(3);
    expect(result.rows).toHaveLength(49);
    expect(result.rows.every((row) => row.status === "failed")).toBe(true);
    expect(result.rows[result.rows.length - 1]?.path).toBe("资料/章节/1980.txt");
    expect(importResultPage([], 5, true)).toEqual({ page: 0, pages: 1, rows: [] });
  });
});
