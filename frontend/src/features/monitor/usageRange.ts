export function localDateInput(date: Date): string { return `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, "0")}-${String(date.getDate()).padStart(2, "0")}`; }
export function usageQuery(from: string, through: string, modelId = ""): string {
  const start = new Date(`${from}T00:00:00`); const end = new Date(`${through}T00:00:00`); end.setDate(end.getDate() + 1);
  if (!Number.isFinite(start.getTime()) || !Number.isFinite(end.getTime()) || end <= start) throw new Error("请选择有效的起止日期。");
  if (end.getTime() - start.getTime() > 366 * 86400000) throw new Error("统计范围最多 366 天。");
  const query = new URLSearchParams({ from: String(start.getTime()), to: String(end.getTime()), timezone_offset_minutes: String(-start.getTimezoneOffset()) });
  if (modelId) query.set("model_id", modelId); return `?${query}`;
}
