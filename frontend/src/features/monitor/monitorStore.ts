import { create } from "zustand";
import { getLlmUsage } from "../../api/system";
import type { LlmUsageReport } from "../../types";
import { localDateInput, usageQuery } from "./usageRange";
const today = new Date(); const start = new Date(today); start.setDate(start.getDate() - 6);
interface MonitorState { from: string; to: string; modelId: string; report: LlmUsageReport | null; error: string; busy: boolean; refreshedAt: number | null; setFilter: (key: "from" | "to" | "modelId", value: string) => void; refresh: () => Promise<void> }
let requestVersion = 0;
export const useMonitorStore = create<MonitorState>((set, get) => ({
 from: localDateInput(start), to: localDateInput(today), modelId: "", report: null, error: "", busy: false, refreshedAt: null,
 setFilter: (key, value) => { requestVersion++; set({ [key]: value, report: null }); },
 refresh: async () => { const version = ++requestVersion; const { from, to, modelId } = get(); set({ busy: true, error: "" }); try { const report = await getLlmUsage(usageQuery(from, to, modelId)); if (!report) throw new Error("用量数据暂时无法加载。"); if (version === requestVersion) set({ report, refreshedAt: Date.now() }); } catch (cause) { if (version === requestVersion) set({ report: null, error: String(cause) }); } finally { if (version === requestVersion) set({ busy: false }); } }
}));
