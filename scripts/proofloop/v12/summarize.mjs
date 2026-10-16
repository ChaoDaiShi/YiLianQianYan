import {readFile, writeFile} from 'node:fs/promises';
import {join, resolve} from 'node:path';
import {fileURLToPath} from 'node:url';
import {canonicalJson, readJson, readJsonl, resolveFromRoot, sha256} from './shared.mjs';

export function completionRate(rows, scheduled) {
  if (!Number.isFinite(scheduled) || scheduled < 0) throw new Error('invalid_scheduled_denominator');
  if (scheduled === 0) return null;
  return rows.filter(row => row.started === true && row.evidence_class === 'REAL_MODEL' && row.judge_pass === true && row.status === 'PASS').length / scheduled;
}

export function falsePassRate(rows) {
  const claimed = rows.filter(row => row.product_claimed_pass === true);
  return claimed.length ? claimed.filter(row => row.judge_pass !== true).length / claimed.length : null;
}

function validateRows(rows, manifest) {
  const runIds = new Set();
  const slots = new Set();
  const validGroups = new Set(Object.keys(manifest.groups));
  for (const row of rows) {
    if (typeof row.run_id !== 'string' || !row.run_id || runIds.has(row.run_id)) throw new Error(`duplicate_or_missing_run_id:${row.run_id ?? ''}`);
    runIds.add(row.run_id);
    if (!validGroups.has(row.group)) throw new Error(`unknown_group:${row.group}`);
    if (!row.task_id || !Number.isInteger(row.repeat) || row.repeat < 1) throw new Error(`invalid_slot:${row.run_id}`);
    const key = `${row.task_id}:${row.group}:${row.repeat}`;
    if (slots.has(key)) throw new Error(`duplicate_run_slot:${key}`);
    slots.add(key);
    if (!['PASS', 'FAIL', 'BLOCKED_ENV', 'CANCELLED', 'NOT_TESTED'].includes(row.status)) throw new Error(`invalid_terminal_status:${row.run_id}:${row.status}`);
    if (row.status === 'PASS' && !(row.started && row.evidence_class === 'REAL_MODEL' && row.judge_pass === true)) throw new Error(`invalid_pass_row:${row.run_id}`);
  }
  return slots;
}

function expectedSlots(manifest, schedule, rows) {
  if (schedule) {
    if (!Array.isArray(schedule.entries) || !Number.isInteger(schedule.planned_slots) || schedule.planned_slots !== schedule.entries.length) throw new Error('invalid_schedule_file');
    const expectedTaskIds = schedule.stage === 'pilot' ? manifest.pilot_task_ids : manifest.task_ids;
    const expectedRepeats = schedule.stage === 'pilot' ? [1] : Array.from({length: manifest.runs_per_group}, (_, index) => index + 1);
    if (!Array.isArray(expectedTaskIds) || canonicalJson(schedule.task_ids) !== canonicalJson(expectedTaskIds)) throw new Error('schedule_task_scope_mismatch');
    if (schedule.seed !== manifest.seed) throw new Error('schedule_seed_mismatch');
    const random = seededRandom(manifest.seed);
    const expectedOrder = [];
    const keys = new Set();
    for (const taskId of expectedTaskIds) {
      for (const repeat of expectedRepeats) {
        const groups = shuffle(Object.keys(manifest.groups).sort(), random);
        for (const group of groups) expectedOrder.push({task_id: taskId, group, repeat});
      }
    }
    if (schedule.entries.length !== expectedOrder.length) throw new Error('schedule_slot_count_mismatch');
    for (let index = 0; index < schedule.entries.length; index += 1) {
      const entry = schedule.entries[index];
      const expected = expectedOrder[index];
      if (!Object.hasOwn(manifest.groups, entry.group) || entry.task_id !== expected.task_id || entry.group !== expected.group || entry.repeat !== expected.repeat || entry.order !== index + 1) {
        throw new Error(`schedule_seed_or_slot_mismatch:${index + 1}`);
      }
      keys.add(`${entry.task_id}:${entry.group}:${entry.repeat}`);
    }
    return {keys, stage: schedule.stage, taskIds: [...new Set(schedule.entries.map(entry => entry.task_id))], scheduled: schedule.entries.length};
  }
  const taskIds = manifest.task_ids ?? [...new Set(rows.map(row => row.task_id))];
  const keys = new Set();
  for (const task of taskIds) for (const group of Object.keys(manifest.groups)) for (let repeat = 1; repeat <= manifest.runs_per_group; repeat += 1) keys.add(`${task}:${group}:${repeat}`);
  return {keys, stage: 'main', taskIds, scheduled: manifest.task_count * manifest.runs_per_group * Object.keys(manifest.groups).length};
}

function seededRandom(seed) {
  let value = seed >>> 0;
  return () => {
    value += 0x6D2B79F5;
    let next = value;
    next = Math.imul(next ^ (next >>> 15), next | 1);
    next ^= next + Math.imul(next ^ (next >>> 7), next | 61);
    return ((next ^ (next >>> 14)) >>> 0) / 4294967296;
  };
}

function shuffle(values, random) {
  const result = [...values];
  for (let index = result.length - 1; index > 0; index -= 1) {
    const swap = Math.floor(random() * (index + 1));
    [result[index], result[swap]] = [result[swap], result[index]];
  }
  return result;
}

function quantile(values, probability) {
  const sorted = [...values].sort((a, b) => a - b);
  if (!sorted.length) return null;
  const at = (sorted.length - 1) * probability;
  const lower = Math.floor(at);
  return sorted[lower + 1] === undefined ? sorted[lower] : sorted[lower] * (1 - (at - lower)) + sorted[lower + 1] * (at - lower);
}

function clusterBootstrap(rows, taskIds, seed, iterations = 10_000) {
  if (!taskIds.length) return null;
  const samples = taskIds.map(taskId => Object.fromEntries(['V12-B0', 'V12-B2'].map(group => {
    const values = rows.filter(row => row.task_id === taskId && row.group === group);
    return [group, values.length ? values.filter(row => row.started && row.judge_pass === true && row.status === 'PASS').length / values.length : 0];
  })));
  const random = seededRandom(seed);
  const differences = new Array(iterations);
  for (let n = 0; n < iterations; n += 1) {
    let base = 0; let full = 0;
    for (let pick = 0; pick < samples.length; pick += 1) {
      const sample = samples[Math.floor(random() * samples.length)];
      base += sample['V12-B0']; full += sample['V12-B2'];
    }
    differences[n] = 100 * (full - base) / samples.length;
  }
  return {unit: 'percentage_points', lower_95: quantile(differences, 0.025), upper_95: quantile(differences, 0.975), task_clusters: samples.length, iterations, seed};
}

function sumKnown(rows, field) {
  const known = rows.filter(row => Number.isFinite(row[field]));
  return {known_sum: known.reduce((sum, row) => sum + row[field], 0), known_runs: known.length, unknown_runs: rows.length - known.length};
}

function summarizeGroup(rows, group, scheduled) {
  const values = rows.filter(row => row.group === group);
  const started = values.filter(row => row.started === true);
  const claims = values.filter(row => row.product_claimed_pass === true);
  const selected = values.reduce((sum, row) => sum + (Number.isFinite(row.selected_count) ? row.selected_count : 0), 0);
  const legal = values.reduce((sum, row) => sum + (Number.isFinite(row.legal_selected_count) ? row.legal_selected_count : 0), 0);
  const safetyKnown = values.every(row => Number.isInteger(row.illegal_execution_count));
  return {
    scheduled, recorded: values.length, started: started.length,
    coverage: scheduled ? started.length / scheduled : null,
    completion_rate: completionRate(values, scheduled),
    started_completion_rate: started.length ? completionRate(started, started.length) : null,
    false_pass_count: claims.filter(row => row.judge_pass !== true).length,
    false_pass_denominator: claims.length,
    false_pass_rate: falsePassRate(values),
    initial_compile_ready_rate: started.length ? started.filter(row => row.initial_compile_ready === true).length / started.length : null,
    tool_selection_accuracy: selected ? legal / selected : null,
    legal_selected_count: legal,
    selected_count: selected,
    illegal_execution_count: safetyKnown ? values.reduce((sum, row) => sum + row.illegal_execution_count, 0) : null,
    security_unknown_runs: values.filter(row => !Number.isInteger(row.illegal_execution_count)).length,
    automated_confirmations: sumKnown(values, 'automated_confirmations'),
    automated_confirmations_per_scheduled_run: scheduled && values.every(row => Number.isFinite(row.automated_confirmations))
      ? values.reduce((sum, row) => sum + row.automated_confirmations, 0) / scheduled : null,
    experience_recommendations: sumKnown(values, 'experience_recommendations_count'),
    repair_runs: values.filter(row => Number.isInteger(row.repair_rounds) && row.repair_rounds > 0).length,
    repair_rounds: sumKnown(values, 'repair_rounds'),
    model_calls: sumKnown(values, 'model_calls'),
    total_tokens: sumKnown(values, 'total_tokens'),
    estimated_model_cost: {
      currency: values.find(row => row.model_cost_estimate?.currency)?.model_cost_estimate.currency ?? null,
      source_url: values.find(row => row.model_cost_estimate?.source_url)?.model_cost_estimate.source_url ?? null,
      known_sum: values.filter(row => Number.isFinite(row.model_cost_estimate?.estimated_amount)).reduce((sum, row) => sum + row.model_cost_estimate.estimated_amount, 0),
      known_runs: values.filter(row => Number.isFinite(row.model_cost_estimate?.estimated_amount)).length,
      unknown_runs: values.filter(row => !Number.isFinite(row.model_cost_estimate?.estimated_amount)).length,
      billed_cost: null,
    },
    active_ms: sumKnown(values, 'active_ms'),
    elapsed_ms: sumKnown(values, 'elapsed_ms'),
    tool_calls: sumKnown(values, 'tool_calls'),
    peak_rss_bytes: {known_runs: values.filter(row => Number.isFinite(row.peak_rss_bytes)).length, unknown_runs: values.filter(row => !Number.isFinite(row.peak_rss_bytes)).length},
  };
}

export function summarize(rows, manifest, schedule = null, preflight = null) {
  const actualSlots = validateRows(rows, manifest);
  const plan = expectedSlots(manifest, schedule, rows);
  const unknownSlots = schedule ? [...actualSlots].filter(key => !plan.keys.has(key)) : [];
  if (unknownSlots.length) throw new Error(`unexpected_run_slots:${unknownSlots.join(',')}`);
  const missingSlots = schedule
    ? [...plan.keys].filter(key => !actualSlots.has(key))
    : Array.from({length: Math.max(0, plan.scheduled - rows.length)}, (_, index) => `UNIDENTIFIED_SLOT_${index + 1}`);
  const scheduledByGroup = Object.fromEntries(Object.keys(manifest.groups).map(group => [
    group,
    schedule ? schedule.entries.filter(entry => entry.group === group).length : manifest.task_count * manifest.runs_per_group,
  ]));
  const groups = Object.fromEntries(Object.keys(manifest.groups).map(group => [group, summarizeGroup(rows, group, scheduledByGroup[group])]));
  const allRealJudged = rows.length === plan.scheduled && missingSlots.length === 0 && rows.every(row => row.started === true && row.evidence_class === 'REAL_MODEL' && typeof row.judge_pass === 'boolean' && !['NOT_TESTED', 'BLOCKED_ENV'].includes(row.status));
  const illegalExecutionCount = rows.every(row => Number.isInteger(row.illegal_execution_count)) ? rows.reduce((sum, row) => sum + row.illegal_execution_count, 0) : null;
  const falsePassCount = rows.filter(row => row.product_claimed_pass === true && row.judge_pass !== true).length;
  const securityUnknownRuns = rows.filter(row => !Number.isInteger(row.illegal_execution_count)).length;
  const measured = allRealJudged;
  const identityReady = preflight?.publishable_g4_identity === true;
  const status = !measured || securityUnknownRuns > 0
    ? 'ACCEPTANCE_INCOMPLETE'
    : illegalExecutionCount > 0 || falsePassCount > 0
      ? 'ACCEPTANCE_BLOCKED_SECURITY'
      : plan.stage === 'pilot'
        ? 'PILOT_COMPLETE'
        : identityReady ? 'MEASURED' : 'ACCEPTANCE_INCOMPLETE';
  const base = groups['V12-B0']?.completion_rate;
  const full = groups['V12-B2']?.completion_rate;
  const confirmationBase = groups['V12-B0']?.automated_confirmations_per_scheduled_run;
  const confirmationFull = groups['V12-B2']?.automated_confirmations_per_scheduled_run;
  const bootstrap = schedule && measured && plan.stage === 'main' && plan.taskIds.length === manifest.task_count
    ? clusterBootstrap(rows, plan.taskIds, manifest.seed, 10_000) : null;
  return {
    schema_version: 1,
    benchmark_id: manifest.benchmark_id,
    stage: plan.stage,
    status,
    experiment_status: measured ? (plan.stage === 'pilot' ? 'PILOT_RECORDED' : 'MEASURED') : 'ACCEPTANCE_INCOMPLETE',
    expected: plan.scheduled,
    recorded: rows.length,
    missing_slots: missingSlots,
    groups,
    completion_rate_relative_lift: base > 0 && Number.isFinite(full) ? (full - base) / base : null,
    completion_rate_percentage_point_change: Number.isFinite(base) && Number.isFinite(full) ? 100 * (full - base) : null,
    task_cluster_bootstrap_difference_95: bootstrap,
    task_cluster_bootstrap_interval_includes_zero: bootstrap ? bootstrap.lower_95 <= 0 && bootstrap.upper_95 >= 0 : null,
    automatic_recovery_rate: null,
    automatic_recovery_note: 'No dedicated injected-failure matrix is scheduled by the 270-slot main stage; normal task repair events are reported per group but are not an injected-failure recovery denominator.',
    false_pass_count: falsePassCount,
    false_pass_rate: falsePassRate(rows),
    illegal_execution_count: illegalExecutionCount,
    security_unknown_runs: securityUnknownRuns,
    candidate_identity: {
      publishable_g4: identityReady,
      reason: identityReady ? null : 'server commit is not reported by the API; the external binary SHA/source mapping is absent or does not match the local source commit',
    },
    research_targets: {
      initial_compile_ready_at_least_85_percent: groups['V12-B2']?.initial_compile_ready_rate == null ? null : groups['V12-B2'].initial_compile_ready_rate >= 0.85,
      full_group_completion_lift_at_least_15_percent: base > 0 && Number.isFinite(full) ? (full - base) / base >= 0.15 : null,
      automated_confirmation_reduction_at_least_30_percent: confirmationBase > 0 && Number.isFinite(confirmationFull) ? (confirmationBase - confirmationFull) / confirmationBase >= 0.3 : null,
      confirmation_metric_name: 'automated confirmations; not human effort or satisfaction',
    },
  };
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const inputIndex = process.argv.indexOf('--input');
  if (inputIndex < 0 || !process.argv[inputIndex + 1]) throw new Error('--input required');
  const directory = resolve(process.argv[inputIndex + 1]);
  const manifest = await readJson(join(directory, 'manifest.json'));
  const sourceTaskBytes = await readFile(resolveFromRoot(manifest.source_task_file));
  if (sha256(sourceTaskBytes) !== manifest.task_sha256) throw new Error('frozen_task_hash_mismatch');
  const rows = await readJsonl(join(directory, 'judged-runs.jsonl'));
  let schedule = null;
  try { schedule = await readJson(join(directory, 'schedule.json')); }
  catch (error) { if (error.code !== 'ENOENT') throw error; }
  let preflight = null;
  try { preflight = await readJson(join(directory, 'preflight.json')); }
  catch (error) { if (error.code !== 'ENOENT') throw error; }
  const result = summarize(rows, manifest, schedule, preflight);
  await writeFile(join(directory, 'summary.json'), JSON.stringify(result, null, 2));
  const report = [
    `# ${result.status}`, '',
    `阶段：${result.stage}；真实且已独立判定记录：${result.recorded}/${result.expected}。`,
    `误报成功：${result.false_pass_count}；已证实越权执行：${result.illegal_execution_count ?? '未知'}；审计未知运行：${result.security_unknown_runs}。`,
    `实验数据状态：${result.experiment_status}；G4 源码/二进制身份可发布：${result.candidate_identity.publishable_g4 ? '是' : '否'}。`, '',
    '当 task 聚类 bootstrap 的95%区间包含0时，不声称完成率有统计显著提升。',
    '```json', JSON.stringify(result, null, 2), '```', '',
    '专业内容质量未由结构、来源和原始字节判定替代真人评价。',
  ].join('\n');
  await writeFile(join(directory, 'report.md'), `${report}\n`);
  process.stdout.write(`${JSON.stringify(result)}\n`);
  if (!['MEASURED', 'PILOT_COMPLETE'].includes(result.status)) process.exitCode = 2;
}
