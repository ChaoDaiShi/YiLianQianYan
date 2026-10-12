import assert from 'node:assert';
import { mkdtempSync, writeFileSync, unlinkSync, rmdirSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { createHash } from 'node:crypto';
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
  task_id: 'review',
  started: true,
  status: 'COMPLETED',
  goal_pass: true,
  illegal_executions: 0,
};
const root = mkdtempSync(join(tmpdir(), 'proofloop-summary-'));
const filename = join(root, 'report.md');
writeFileSync(filename, '# Report\n\nActual test output.\n');
const sha256 = createHash('sha256').update('# Report\n\nActual test output.\n').digest('hex');
const specification = (task_id) => ({ task_id, deliverables: [{ id: 'd1' }], criteria: [{ id: 'exists', required: true, check: { kind: 'artifact_exists', deliverable_id: 'd1' } }] });
passRun.artifacts = [{ deliverable_id: 'd1', path: filename, sha256 }];
const passResult = verifyRunEvidence(passRun, specification('review'));
assert.strictEqual(passResult.pass, true);

// 缺证据时不可判定通过
const missingEvidenceRun = {
  task_id: 'review',
  started: true,
  status: 'COMPLETED',
  goal_pass: true,
  evidence_paths: [],
  illegal_executions: 0,
};
const missingResult = verifyRunEvidence(missingEvidenceRun, specification('review'));
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

for (const run of mockRuns) {
  if (run.goal_pass) run.artifacts = [{ deliverable_id: 'd1', path: filename, sha256 }];
}
const summary = summarizeBenchmark(mockRuns, [specification('A01'), specification('A02')]);
assert.strictEqual(summary.groups.B2.goal_pass_rate, 1.0);
assert.strictEqual(summary.groups.B0.goal_pass_rate, 0.5);
assert.strictEqual(summary.comparisons.goal_completion_lift_b2_vs_b0, 0.5); // +50% lift
assert.strictEqual(summary.comparisons.human_operations_reduction_pct, 0.5); // (10-5)/10 = 50% reduction

const md = formatReportMarkdown(summary);
assert.ok(md.includes('ProofLoop 评测实验统计报告'));
assert.ok(md.includes('| **B2** |'));

console.log('All summarize.mjs tests passed successfully!');
unlinkSync(filename);
rmdirSync(root);
