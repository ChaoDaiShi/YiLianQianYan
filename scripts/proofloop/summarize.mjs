#!/usr/bin/env node

/**
 * ProofLoop Benchmark Summarizer & Independent Verifier
 *
 * 依据 2026-10-12-proofloop/05-evaluation-and-evidence.md 规范实现。
 * 核心原则：
 * 1. 独立口径判定：不依赖产品自身上报的 pass 标签，根据成果与判定条件核算。
 * 2. 空白/未运行/缺失证据样本严格记录为 NOT_TESTED 或 FAIL，绝不手填或伪造成功率。
 * 3. 统计初始可执行率、目标完成率提升、人工操作减少量、非法执行次数等官方比赛指标。
 */

import fs from 'node:fs';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { fileURLToPath } from 'node:url';

const defaultTasks = JSON.parse(fs.readFileSync(new URL('../../backend/tests/fixtures/proofloop/benchmark-tasks.json', import.meta.url), 'utf8')).tasks;

/**
 * 计算数字数组的中位数与 P90
 */
export function calculatePercentiles(values) {
  if (!values || values.length === 0) {
    return { median: null, p90: null, count: 0 };
  }
  const sorted = [...values].filter((v) => typeof v === 'number' && !Number.isNaN(v)).sort((a, b) => a - b);
  if (sorted.length === 0) return { median: null, p90: null, count: 0 };

  const midpoint = Math.floor(sorted.length / 2);
  const median = sorted.length % 2 ? sorted[midpoint] : (sorted[midpoint - 1] + sorted[midpoint]) / 2;
  const p90Index = Math.min(sorted.length - 1, Math.floor(sorted.length * 0.9));
  const p90 = sorted[p90Index];

  return { median, p90, count: sorted.length };
}

/**
 * 独立公共判定器：核验单次运行是否真正达成目标
 */
export function verifyRunEvidence(run, taskSpec) {
  if (!run || !run.started || run.status === 'NOT_TESTED') {
    return { pass: false, reason: '未实际启动或未完成执行' };
  }

  // 必须无未授权或非法工具执行
  if (run.illegal_executions && run.illegal_executions > 0) {
    return { pass: false, reason: '存在未授权工具或非法越权执行' };
  }

  if (!taskSpec || taskSpec.task_id !== run.task_id) return { pass: false, reason: '缺少匹配的任务验收规格' };
  const required = taskSpec.criteria?.filter((criterion) => criterion.required) || [];
  if (!required.length) return { pass: false, reason: '缺少必需验收条件' };
  const artifacts = new Map();
  try {
    for (const deliverable of taskSpec.deliverables || []) {
      let record = run.artifacts?.find((item) => item.deliverable_id === deliverable.id);
      if (!record && taskSpec.deliverables.length === 1 && run.evidence_paths?.length === 1) record = { path: run.evidence_paths[0], sha256: run.artifact_digest };
      if (!record?.path || !fs.statSync(record.path).isFile()) continue;
      const bytes = fs.readFileSync(record.path);
      const digest = createHash('sha256').update(bytes).digest('hex');
      if (record.sha256 && record.sha256 !== digest) return { pass: false, reason: '产物摘要与实际文件不一致' };
      artifacts.set(deliverable.id, { bytes, text: bytes.toString('utf8'), digest, expectedDigest: record.sha256 });
    }
    for (const criterion of required) {
      const check = criterion.check;
      const artifact = artifacts.get(check.deliverable_id);
      let pass = false;
      if (check.kind === 'artifact_exists') pass = Boolean(artifact?.bytes.length);
      else if (check.kind === 'artifact_digest_matches') pass = Boolean(artifact?.expectedDigest && artifact.expectedDigest === artifact.digest);
      else if (check.kind === 'required_sections') {
        const headings = markdownHeadings(artifact?.text || '');
        pass = Boolean(artifact && check.headings.every((heading) => headings.has(normalizeHeading(heading))));
      } else if (check.kind === 'source_traceability') {
        const sources = new Set();
        for (const source of run.sources || []) {
          if (!artifact || !source.uri || source.retrieved_at_ms <= 0 || !source.cited_by_deliverable_ids?.includes(check.deliverable_id) || !artifact.text.includes(source.uri) || !source.evidence_path) continue;
          const evidence = JSON.parse(fs.readFileSync(source.evidence_path, 'utf8'));
          if (evidence.status !== 'succeeded' || evidence.validation?.status !== 'accepted') continue;
          const declared = evidence.output?.structured?.sources || [];
          if (declared.some((item) => (item.uri || item.url) === source.uri)) sources.add(source.uri);
        }
        pass = Boolean(artifact && sources.size >= check.min_sources);
      } else if (check.kind === 'human_review') {
        pass = Boolean(artifact && run.human_reviews?.some((review) => review.criterion_id === criterion.id && review.decision === 'pass' && review.reason?.trim() && review.reviewer_subject && review.reviewed_at_ms > 0));
      }
      if (!pass) return { pass: false, reason: `必需条件未通过：${criterion.id} (${check.kind})` };
    }
    return { pass: true, reason: '实际成果文件与全部必需验收条件一致' };
  } catch (error) {
    return { pass: false, reason: `成果证据不可读取：${error.message}` };
  }
}

const normalizeHeading = (value) => value.trim().toLowerCase().replace(/\s+/g, '');
function markdownHeadings(text) {
  const headings = new Set(); let fence = null;
  for (const line of text.split(/\r?\n/)) {
    const marker = line.trim().match(/^(`{3,}|~{3,})/);
    if (marker) { if (!fence) fence = marker[1][0]; else if (marker[1][0] === fence) fence = null; continue; }
    if (fence) continue;
    const heading = line.trim().match(/^#{1,6}\s+(.+?)(?:\s+#+)?$/);
    if (heading) headings.add(normalizeHeading(heading[1]));
  }
  return headings;
}

/**
 * 聚合单个实验组 (B0 / B1 / B2 / BR) 的运行数据
 */
export function aggregateGroup(runs, groupName, taskSpecsMap = {}) {
  const groupRuns = runs.filter((r) => r.group === groupName);
  const total = groupRuns.length;
  if (total === 0) {
    return {
      group: groupName,
      total_runs: 0,
      started_runs: 0,
      initial_compile_ready_rate: null,
      goal_pass_rate: null,
      human_operations: { median: null, p90: null },
      elapsed_ms: { median: null, p90: null },
      tokens: { median: null, p90: null },
      model_calls: { median: null, p90: null },
      illegal_executions: 0,
      false_pass_count: 0,
    };
  }

  const startedRuns = groupRuns.filter((r) => r.started);
  const readyCount = startedRuns.filter((r) => r.initial_compile_ready === true).length;

  let verifiedPassCount = 0;
  let falsePassCount = 0;

  for (const r of startedRuns) {
    const taskSpec = taskSpecsMap[r.task_id];
    const verification = verifyRunEvidence(r, taskSpec);
    if (verification.pass) {
      verifiedPassCount++;
    }
    // 检查误报：产品宣称 pass 但独立判定为 false
    if (r.goal_pass === true && !verification.pass) {
      falsePassCount++;
    }
  }

  const humanOps = startedRuns.map((r) => r.human_operations).filter((v) => typeof v === 'number');
  const elapsed = startedRuns.map((r) => r.elapsed_ms).filter((v) => typeof v === 'number');
  const tokens = startedRuns.map((r) => r.tokens).filter((v) => typeof v === 'number');
  const modelCalls = startedRuns.map((r) => r.model_calls).filter((v) => typeof v === 'number');

  const totalIllegal = startedRuns.reduce((sum, r) => sum + (r.illegal_executions || 0), 0);

  return {
    group: groupName,
    total_runs: total,
    started_runs: startedRuns.length,
    initial_compile_ready_rate: startedRuns.length > 0 ? readyCount / startedRuns.length : null,
    goal_pass_rate: startedRuns.length > 0 ? verifiedPassCount / startedRuns.length : null,
    verified_pass_count: verifiedPassCount,
    human_operations: calculatePercentiles(humanOps),
    elapsed_ms: calculatePercentiles(elapsed),
    tokens: calculatePercentiles(tokens),
    model_calls: calculatePercentiles(modelCalls),
    illegal_executions: totalIllegal,
    false_pass_count: falsePassCount,
  };
}

/**
 * 计算对照组间的相对提升指标
 */
export function summarizeBenchmark(runs, taskSpecs = defaultTasks) {
  const taskSpecsMap = Object.fromEntries(taskSpecs.map((t) => [t.task_id, t]));

  const b0 = aggregateGroup(runs, 'B0', taskSpecsMap);
  const b1 = aggregateGroup(runs, 'B1', taskSpecsMap);
  const b2 = aggregateGroup(runs, 'B2', taskSpecsMap);
  const br = aggregateGroup(runs, 'BR', taskSpecsMap);

  // 1. 初始计划可执行率
  const b1ReadyRate = b1.initial_compile_ready_rate;
  const b2ReadyRate = b2.initial_compile_ready_rate;

  // 2. 目标完成率提升 (B2 vs B0)
  let goalLift = null;
  if (b2.goal_pass_rate !== null && b0.goal_pass_rate !== null) {
    goalLift = b2.goal_pass_rate - b0.goal_pass_rate;
  }

  // 3. 人工操作量减少比例 (B2 vs B0)
  let humanOpsReduction = null;
  if (b0.human_operations.median > 0 && b2.human_operations.median !== null) {
    humanOpsReduction = (b0.human_operations.median - b2.human_operations.median) / b0.human_operations.median;
  }

  // 4. 故障恢复对比 (B2 vs BR)
  let recoveryAdvantage = null;
  const recoverableRuns = runs.filter((run) => run.recoverable_fault === true);
  const repairFaults = aggregateGroup(recoverableRuns, 'B2', taskSpecsMap);
  const retryFaults = aggregateGroup(recoverableRuns, 'BR', taskSpecsMap);
  if (repairFaults.goal_pass_rate !== null && retryFaults.goal_pass_rate !== null) {
    recoveryAdvantage = repairFaults.goal_pass_rate - retryFaults.goal_pass_rate;
  }

  return {
    groups: { B0: b0, B1: b1, B2: b2, BR: br },
    comparisons: {
      initial_ready_rate_b1: b1ReadyRate,
      initial_ready_rate_b2: b2ReadyRate,
      goal_completion_lift_b2_vs_b0: goalLift,
      human_operations_reduction_pct: humanOpsReduction,
      recovery_advantage_b2_vs_br: recoveryAdvantage,
      illegal_executions_total: (b0.illegal_executions || 0) + (b1.illegal_executions || 0) + (b2.illegal_executions || 0),
      false_pass_rate_total: b0.false_pass_count + b1.false_pass_count + b2.false_pass_count + br.false_pass_count,
    },
  };
}

/**
 * 格式化为 Markdown 报告表格
 */
export function formatReportMarkdown(summary) {
  const { groups, comparisons } = summary;

  const fmtPct = (val) => (val === null || val === undefined ? 'N/A' : `${(val * 100).toFixed(1)}%`);
  const fmtNum = (val) => (val === null || val === undefined ? 'N/A' : String(val));

  let md = `# ProofLoop 评测实验统计报告\n\n`;
  md += `> 生成时间：${new Date().toISOString()}。判定口径：独立公共判定器核验。\n\n`;

  md += `## 1. 核心目标达成度对比\n\n`;
  md += `| 实验对照组 | 启动样本数 | 初始计划可执行率 | 目标成果达成率 | 人工操作数(中位数) | 平均耗时(ms) | 非法真实执行 |\n`;
  md += `|---|---|---|---|---|---|---|\n`;

  for (const key of ['B0', 'B1', 'B2', 'BR']) {
    const g = groups[key];
    md += `| **${key}** | ${g.started_runs} / ${g.total_runs} | ${fmtPct(g.initial_compile_ready_rate)} | ${fmtPct(g.goal_pass_rate)} | ${fmtNum(g.human_operations.median)} | ${fmtNum(g.elapsed_ms.median)} | ${g.illegal_executions} |\n`;
  }

  md += `\n## 2. 关键研发目标核对\n\n`;
  md += `- **初始计划可执行率**：B1 = ${fmtPct(comparisons.initial_ready_rate_b1)}，B2 = ${fmtPct(comparisons.initial_ready_rate_b2)}（研发目标 ≥85%）\n`;
  md += `- **目标完成率相对提升**：${comparisons.goal_completion_lift_b2_vs_b0 !== null ? `+${(comparisons.goal_completion_lift_b2_vs_b0 * 100).toFixed(1)} 个百分点` : 'N/A'}（研发目标 ≥15%）\n`;
  md += `- **人工操作量减少比例**：${comparisons.human_operations_reduction_pct !== null ? `${(comparisons.human_operations_reduction_pct * 100).toFixed(1)}%` : 'N/A'}（研发目标 ≥30%）\n`;
  md += `- **受控恢复优于盲重试**：B2 相比 BR 提升 ${comparisons.recovery_advantage_b2_vs_br !== null ? `${(comparisons.recovery_advantage_b2_vs_br * 100).toFixed(1)} 个百分点` : 'N/A'}\n`;
  md += `- **未授权/非法真实执行次数**：${comparisons.illegal_executions_total} 次（底线标准 = 0）\n`;
  md += `- **误报通过次数**：${comparisons.false_pass_rate_total} 次（底线标准 = 0）\n\n`;

  return md;
}

// CLI runner
if (process.argv[1] && path.resolve(process.argv[1]) === path.resolve(fileURLToPath(import.meta.url))) {
  const jsonlPath = process.argv[2];
  if (!jsonlPath) {
    console.log('Usage: node scripts/proofloop/summarize.mjs <benchmark_runs.jsonl>');
    process.exit(0);
  }

  if (!fs.existsSync(jsonlPath)) {
    console.error(`File not found: ${jsonlPath}`);
    process.exit(1);
  }

  const content = fs.readFileSync(jsonlPath, 'utf8');
  const lines = content.split('\n').filter((line) => line.trim().length > 0);
  const runs = lines.map((l) => JSON.parse(l));

  const tasks = process.argv[3] ? JSON.parse(fs.readFileSync(process.argv[3], 'utf8')).tasks : defaultTasks;
  for (const run of runs) {
    run.evidence_paths = run.evidence_paths?.map((item) => path.resolve(path.dirname(jsonlPath), item));
    run.artifacts = run.artifacts?.map((item) => ({ ...item, path: path.resolve(path.dirname(jsonlPath), item.path) }));
    run.sources = run.sources?.map((item) => ({ ...item, evidence_path: item.evidence_path ? path.resolve(path.dirname(jsonlPath), item.evidence_path) : null }));
  }
  const summary = summarizeBenchmark(runs, tasks);
  const markdown = formatReportMarkdown(summary);
  console.log(markdown);
}
