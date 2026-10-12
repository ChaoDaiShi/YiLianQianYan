import assert from 'node:assert';
import {
  calculatePercentiles,
  verifyRunEvidence,
  aggregateGroup,
  summarizeBenchmark,
  formatReportMarkdown,
} from './summarize.mjs';

console.log('Testing summarize.mjs...');

// 1. 测试分位数计算
const stats = calculatePercentiles([10, 20, 30, 40, 50]);
assert.strictEqual(stats.median, 30);
assert.strictEqual(stats.p90, 50);

// 2. 测试独立验证器
const passRun = {
  started: true,
  status: 'COMPLETED',
  goal_pass: true,
  evidence_paths: ['artifacts/out.md'],
  illegal_executions: 0,
};
const passResult = verifyRunEvidence(passRun, { deliverables: [{ id: 'd1' }] });
assert.strictEqual(passResult.pass, true);

// 缺证据时不可判定通过
const missingEvidenceRun = {
  started: true,
  status: 'COMPLETED',
  goal_pass: true,
  evidence_paths: [],
  illegal_executions: 0,
};
const missingResult = verifyRunEvidence(missingEvidenceRun, { deliverables: [{ id: 'd1' }] });
assert.strictEqual(missingResult.pass, false);

// 3. 测试对照组聚合
const mockRuns = [
  // B0 基线：完成率较低，操作较多
  { task_id: 'A01', group: 'B0', started: true, initial_compile_ready: false, goal_pass: false, human_operations: 10, elapsed_ms: 1000 },
  { task_id: 'A02', group: 'B0', started: true, initial_compile_ready: true, goal_pass: true, evidence_paths: ['a.md'], human_operations: 8, elapsed_ms: 900 },
  // B2 证行：完成率高，操作少
  { task_id: 'A01', group: 'B2', started: true, initial_compile_ready: true, goal_pass: true, evidence_paths: ['a.md'], human_operations: 4, elapsed_ms: 600 },
  { task_id: 'A02', group: 'B2', started: true, initial_compile_ready: true, goal_pass: true, evidence_paths: ['b.md'], human_operations: 5, elapsed_ms: 650 },
  // BR 盲重试：恢复较差
  { task_id: 'A01', group: 'BR', started: true, initial_compile_ready: true, goal_pass: false, human_operations: 6, elapsed_ms: 800 },
  { task_id: 'A02', group: 'BR', started: true, initial_compile_ready: true, goal_pass: true, evidence_paths: ['b.md'], human_operations: 6, elapsed_ms: 850 },
];

const summary = summarizeBenchmark(mockRuns);
assert.strictEqual(summary.groups.B2.goal_pass_rate, 1.0);
assert.strictEqual(summary.groups.B0.goal_pass_rate, 0.5);
assert.strictEqual(summary.comparisons.goal_completion_lift_b2_vs_b0, 0.5); // +50% lift
assert.strictEqual(summary.comparisons.human_operations_reduction_pct, 0.5); // (10-5)/10 = 50% reduction

const md = formatReportMarkdown(summary);
assert.ok(md.includes('ProofLoop 评测实验统计报告'));
assert.ok(md.includes('| **B2** |'));

console.log('All summarize.mjs tests passed successfully!');
