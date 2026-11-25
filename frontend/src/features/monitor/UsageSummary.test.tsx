import { renderToStaticMarkup } from "react-dom/server";
import { expect, it } from "vitest";
import type { LlmUsageReport } from "../../types";
import { UsageSummary } from "./TokenUsagePanel";

const report: LlmUsageReport = {
  prompt_tokens: 0, completion_tokens: 0, total_tokens: 0,
  days: [], models: [], timezone_offset_minutes: 480,
  known_usage_count: 0, unknown_usage_count: 0, failed_attempt_count: 0, pending_attempt_count: 0,
};

it("distinguishes an unknown total from a confirmed empty date range", () => {
  const unknown = renderToStaticMarkup(<UsageSummary report={{ ...report, unknown_usage_count: 2 }} />);
  const empty = renderToStaticMarkup(<UsageSummary report={report} />);
  expect(unknown).toContain(">—</strong>");
  expect(unknown).toContain("未知用量 2 次");
  expect(empty).toContain(">0</strong>");
});

it("shows reported usage alongside pending and failed attempts in the compact summary", () => {
  const html = renderToStaticMarkup(<UsageSummary report={{ ...report, total_tokens: 15, known_usage_count: 1, pending_attempt_count: 1, failed_attempt_count: 2 }} />);
  expect(html).toContain(">15</strong>");
  expect(html).toContain("进行中 1 次");
  expect(html).toContain("失败 2 次");
});
