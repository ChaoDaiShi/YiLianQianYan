export const IMPORT_RESULT_PAGE_SIZE = 50;
export interface ImportNotice {
  name: string;
  path: string;
  status: "imported" | "existing" | "failed" | "skipped";
  message: string;
}
export interface ImportProgress {
  total: number;
  current: string;
  rows: ImportNotice[];
  stopped: boolean;
}
export function summarizeImport(rows: ImportNotice[]) {
  return rows.reduce((result, item) => ({ ...result, [item.status]: result[item.status] + 1 }), { imported: 0, existing: 0, failed: 0, skipped: 0 });
}
export function importResultPage(rows: ImportNotice[], requestedPage: number, onlyErrors: boolean) {
  const filtered = onlyErrors ? rows.filter((row) => row.status === "failed") : rows;
  const pages = Math.max(1, Math.ceil(filtered.length / IMPORT_RESULT_PAGE_SIZE));
  const page = Math.max(0, Math.min(requestedPage, pages - 1));
  return { page, pages, rows: filtered.slice(page * IMPORT_RESULT_PAGE_SIZE, (page + 1) * IMPORT_RESULT_PAGE_SIZE) };
}
