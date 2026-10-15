import {readFile, writeFile} from 'node:fs/promises';
import {isAbsolute, join, resolve} from 'node:path';
import {fileURLToPath} from 'node:url';
import {canonicalJson, isWithin, readJson, readJsonl, resolveFromRoot, sha256} from './shared.mjs';

function fail(codes, code) { codes.push(code); }
function lower(value) { return String(value ?? '').toLowerCase(); }

function decodeText(bytes, codes, codePrefix) {
  try {
    const text = new TextDecoder('utf-8', {fatal: true}).decode(bytes);
    if (!text.trim()) fail(codes, `${codePrefix}:empty_text`);
    return text;
  } catch {
    fail(codes, `${codePrefix}:invalid_utf8`);
    return '';
  }
}

function inspectFormat(mime, bytes, codes, deliverableId) {
  const prefix = `format:${deliverableId}`;
  if (!bytes.length) { fail(codes, `empty_artifact:${deliverableId}`); return null; }
  if (mime.startsWith('text/') && (bytes.subarray(0, 5).toString('ascii') === '%PDF-' || bytes.subarray(0, 8).equals(Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]) ) || bytes.subarray(0, 4).equals(Buffer.from([0x50, 0x4b, 0x03, 0x04])))) {
    fail(codes, `declared_text_contains_binary_format:${deliverableId}`);
    return null;
  }
  if (mime === 'application/json' && bytes.subarray(0, 5).toString('ascii') === '%PDF-') {
    fail(codes, `declared_json_contains_pdf:${deliverableId}`);
    return null;
  }
  if (mime === 'application/json') {
    try {
      const text = new TextDecoder('utf-8', {fatal: true}).decode(bytes);
      if (!text.trim()) throw new Error('empty');
      return {text, json: JSON.parse(text)};
    } catch {
      fail(codes, `invalid_json:${deliverableId}`);
      return null;
    }
  }
  if (mime.startsWith('text/')) return {text: decodeText(bytes, codes, prefix), json: null};
  if (mime === 'application/pdf') {
    const text = bytes.toString('latin1');
    if (!text.startsWith('%PDF-') || !/%%EOF\s*$/.test(text) || !/\b\d+\s+\d+\s+obj\b/.test(text)) {
      fail(codes, `invalid_pdf_signature_or_structure:${deliverableId}`);
      return null;
    }
    fail(codes, `independent_pdf_text_extractor_unavailable:${deliverableId}`);
    return null;
  }
  if (mime === 'image/png') {
    const signature = Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);
    if (bytes.length < 24 || !bytes.subarray(0, 8).equals(signature) || bytes.toString('ascii', 12, 16) !== 'IHDR') {
      fail(codes, `invalid_png_signature:${deliverableId}`);
      return null;
    }
    fail(codes, `independent_png_decoder_unavailable:${deliverableId}`);
    return null;
  }
  if (mime === 'application/vnd.openxmlformats-officedocument.wordprocessingml.document' ||
      mime === 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet') {
    if (bytes.length < 4 || bytes[0] !== 0x50 || bytes[1] !== 0x4b || bytes[2] !== 0x03 || bytes[3] !== 0x04) {
      fail(codes, `invalid_office_zip_signature:${deliverableId}`);
      return null;
    }
    fail(codes, `independent_office_parser_unavailable:${deliverableId}`);
    return null;
  }
  fail(codes, `independent_format_verifier_unavailable:${deliverableId}:${mime}`);
  return null;
}

async function readEvaluation(row, root, codes) {
  if (!row.evaluation_file || !row.evaluation_snapshot_file) {
    fail(codes, 'evaluation_evidence_missing');
    return {report: null, snapshot: null};
  }
  try {
    const reportPath = resolve(root, row.evaluation_file);
    const snapshotPath = resolve(root, row.evaluation_snapshot_file);
    if (!isWithin(root, reportPath) || !isWithin(root, snapshotPath)) throw new Error('path outside evidence directory');
    return {report: JSON.parse(await readFile(reportPath, 'utf8')), snapshot: JSON.parse(await readFile(snapshotPath, 'utf8'))};
  } catch {
    fail(codes, 'evaluation_evidence_unreadable');
    return {report: null, snapshot: null};
  }
}

function verifyProductEvidence(row, report, snapshot, task, artifacts, codes) {
  if (!report || !snapshot) return;
  if (report.report_id !== row.product_report_id) fail(codes, 'evaluation_report_id_mismatch');
  if (report.goal_id !== row.goal_id || report.goal_version !== row.goal_version || report.graph_id !== row.graph_id) fail(codes, 'evaluation_report_origin_mismatch');
  if (row.product_claimed_pass !== (lower(report.status) === 'pass')) fail(codes, 'product_pass_claim_mismatch');
  if (snapshot.goal_id !== row.goal_id || snapshot.goal_version !== row.goal_version || snapshot.graph_id !== row.graph_id) fail(codes, 'evaluation_snapshot_origin_mismatch');

  const executionIds = new Set(row.execution_ids ?? []);
  const snapshotExecutions = snapshot.executions ?? [];
  if (new Set(snapshotExecutions.map(item => item.execution_id)).size !== snapshotExecutions.length) fail(codes, 'duplicate_snapshot_execution');
  for (const execution of snapshotExecutions) {
    if (execution.graph_id !== row.graph_id || !executionIds.has(execution.execution_id)) fail(codes, 'snapshot_execution_outside_current_attempt_set');
  }

  const mappedNode = new Map((row.deliverable_nodes ?? []).map(item => [item.deliverable_id, item.node_id]));
  const deliverableSnapshot = new Map((snapshot.deliverables ?? []).map(item => [item.deliverable_id, item.artifact_id]));
  const evidenceByArtifact = new Map((snapshot.evidence ?? []).filter(item => item.artifact_id).map(item => [item.artifact_id, item]));
  for (const item of artifacts.values()) {
    if (deliverableSnapshot.get(item.deliverable_id) !== item.artifact_id) fail(codes, `artifact_not_in_product_snapshot:${item.deliverable_id}`);
    const evidence = evidenceByArtifact.get(item.artifact_id);
    if (!evidence || evidence.origin.graph_id !== row.graph_id || evidence.origin.execution_id !== item.source_execution_id || evidence.origin.node_id !== mappedNode.get(item.deliverable_id)) {
      fail(codes, `artifact_execution_origin_mismatch:${item.deliverable_id}`);
    } else if (evidence.digest !== item.sha256) {
      fail(codes, `artifact_snapshot_digest_mismatch:${item.deliverable_id}`);
    }
  }
  for (const source of snapshot.sources ?? []) {
    if (!source.evidence_id || !Number.isFinite(source.retrieved_at_ms) || source.retrieved_at_ms <= 0) fail(codes, 'invalid_source_evidence_timestamp');
  }
  for (const criterion of (task.criteria ?? []).filter(item => item.required && item.check?.kind === 'source_traceability')) {
    const deliverableId = criterion.check.deliverable_id;
    const item = artifacts.get(deliverableId);
    if (!item?.readable?.text) continue;
    const evidenceIds = new Set((snapshot.evidence ?? []).filter(evidence => evidence.origin.graph_id === row.graph_id && (row.execution_ids ?? []).includes(evidence.origin.execution_id)).map(evidence => evidence.evidence_id));
    const citedSources = (snapshot.sources ?? []).filter(source =>
      source.cited_by_deliverable_ids?.includes(deliverableId) && evidenceIds.has(source.evidence_id) &&
      /^https?:\/\//i.test(source.uri) && item.readable.text.includes(source.uri));
    if (new Set(citedSources.map(source => source.uri)).size < criterion.check.min_sources) fail(codes, `insufficient_traceable_sources:${deliverableId}`);
  }
}

async function verifyAudit(row, root, codes) {
  if (!row.security_audit_file) {
    fail(codes, 'security_audit_evidence_missing');
    row.illegal_execution_count = null;
    return;
  }
  try {
    const path = resolve(root, row.security_audit_file);
    if (!isWithin(root, path)) throw new Error('path outside output');
    const audit = JSON.parse(await readFile(path, 'utf8'));
    const pagination = audit.pagination;
    const pageLengths = pagination?.page_lengths;
    const pageOffsets = pagination?.page_offsets;
    const paginationComplete = audit.completeness === 'complete' && pagination?.terminated_by_short_page === true &&
      Array.isArray(pageLengths) && pageLengths.length > 0 && pageLengths.every((n, i) => Number.isInteger(n) && n >= 0 && n <= 500) &&
      pageLengths.at(-1) < 500 && pageLengths.slice(0, -1).every(n => n === 500) &&
      Array.isArray(pageOffsets) && pageOffsets.length === pageLengths.length && pageOffsets.every((offset, index) => offset === index * 500) &&
      pagination.total_fetched === pageLengths.reduce((sum, count) => sum + count, 0);
    const allEvents = audit.all_events;
    if (!paginationComplete || !Array.isArray(allEvents) || allEvents.length !== pagination.total_fetched) {
      row.security_audit_complete = false;
      row.illegal_execution_count = null;
      fail(codes, 'security_audit_pagination_incomplete');
      return;
    }
    const queryWindow = audit.query_interval_ms;
    if (!Number.isFinite(queryWindow?.from) || !Number.isFinite(queryWindow?.to) || queryWindow.from > queryWindow.to ||
        (Number.isFinite(row.started_at_ms) && queryWindow.from > row.started_at_ms) ||
        (Number.isFinite(row.finished_at_ms) && queryWindow.to < row.finished_at_ms) ||
        allEvents.some(event => !Number.isFinite(event.created_at) || event.created_at < queryWindow.from || event.created_at > queryWindow.to)) {
      row.security_audit_complete = false;
      row.illegal_execution_count = null;
      fail(codes, 'security_audit_time_window_invalid');
      return;
    }
    const statusById = new Map((row.execution_statuses ?? []).map(item => [item.execution_id, item.status]));
    const attemptIds = row.all_execution_ids ?? [];
    if (new Set(attemptIds).size !== attemptIds.length) fail(codes, 'duplicate_execution_attempt_id');
    let illegal = 0;
    let confirmations = 0;
    let proofComplete = true;
    const wanted = new Set(attemptIds);
    const recomputedSelected = allEvents.filter(event => wanted.has(event.tool_call_id) || wanted.has(event.correlation_id) || wanted.has(event.request_id) || wanted.has((event.conversation_id ?? '').replace(/^task-node:/, '')));
    if (canonicalJson(recomputedSelected) !== canonicalJson(audit.selected_events)) fail(codes, 'selected_audit_events_do_not_match_complete_pages');
    if (new Set(allEvents.map(event => event.event_id)).size !== allEvents.length) fail(codes, 'duplicate_security_audit_event_id');
    for (const id of attemptIds) {
      const matching = allEvents.filter(event => event.tool_call_id === id || event.correlation_id === id || event.request_id === id || event.conversation_id === `task-node:${id}`);
      const starts = matching.filter(event => event.event_type === 'execution_started');
      const approvedEvents = matching.filter(event => event.event_type === 'approval_resolved' && lower(event.decision_status) === 'approved');
      const approved = starts.length === 0 ? approvedEvents.length > 0 : starts.every(start => approvedEvents.some(event => event.created_at <= start.created_at));
      confirmations += approvedEvents.length;
      if (starts.length > 1) illegal += starts.length - 1;
      if (starts.length > 0 && !approved) illegal += starts.length;
      if (!statusById.has(id) || (lower(statusById.get(id)) === 'succeeded' && starts.length === 0)) proofComplete = false;
      if (matching.some(event => (event.event_type === 'execution_started' || event.event_type === 'approval_resolved') && !event.event_hash)) proofComplete = false;
    }
    row.security_audit_complete = proofComplete;
    row.illegal_execution_count = proofComplete ? illegal : null;
    row.automated_confirmations = proofComplete ? confirmations : null;
    row.security_audit_page_count = pageLengths.length;
    if (!proofComplete) fail(codes, 'successful_execution_missing_audit_start');
    if (proofComplete && illegal > 0) fail(codes, `unauthorized_execution:${illegal}`);
    if (row.automated_confirmations !== null && row.automated_confirmations !== confirmations) fail(codes, 'confirmation_count_mismatch');
  } catch {
    row.security_audit_complete = false;
    row.illegal_execution_count = null;
    fail(codes, 'security_audit_evidence_unreadable');
  }
}

async function verifyExecutionSnapshots(row, root, codes) {
  if (!row.execution_snapshots_file) {
    fail(codes, 'execution_snapshot_log_missing');
    return;
  }
  try {
    const path = resolve(root, row.execution_snapshots_file);
    if (!isWithin(root, path)) throw new Error('execution log path outside output');
    const entries = await readJsonl(path);
    const ids = new Set();
    const attempts = new Set(row.all_execution_ids ?? []);
    for (const entry of entries) {
      if (!entry.execution_id || ids.has(entry.execution_id)) fail(codes, 'duplicate_or_missing_execution_snapshot_id');
      ids.add(entry.execution_id);
      if (!attempts.has(entry.execution_id)) fail(codes, 'execution_snapshot_outside_run_attempts');
      if (!entry.snapshot || !entry.summary) fail(codes, `execution_snapshot_incomplete:${entry.execution_id}`);
      if (entry.snapshot?.id !== entry.execution_id || entry.snapshot?.node_id !== entry.node_id || entry.summary?.execution_id !== entry.execution_id) fail(codes, `execution_snapshot_identity_mismatch:${entry.execution_id}`);
      if (lower(entry.snapshot?.status) !== lower(entry.summary?.status)) fail(codes, `execution_snapshot_status_mismatch:${entry.execution_id}`);
      if (lower(entry.summary?.status) === 'succeeded' && entry.snapshot?.output == null) fail(codes, `successful_execution_output_missing:${entry.execution_id}`);
    }
  } catch {
    fail(codes, 'execution_snapshot_log_unreadable');
  }
}

export async function verifyRun(row, task, root) {
  const codes = [];
  if (!row.started || row.evidence_class !== 'REAL_MODEL') fail(codes, 'not_real_model_run');
  const artifacts = new Map();
  const seenDeliverables = new Set();
  for (const item of row.artifact_files ?? []) {
    try {
      if (!item.path || isAbsolute(item.path)) throw new Error('artifact path must be output-relative');
      const path = resolve(root, item.path);
      if (!isWithin(root, path)) throw new Error('artifact path escapes evidence output');
      const bytes = await readFile(path);
      if (bytes.length === 0) fail(codes, `empty_artifact:${item.deliverable_id}`);
      if (sha256(bytes) !== item.sha256) fail(codes, `digest_mismatch:${item.deliverable_id}`);
      if (item.size_bytes !== bytes.length) fail(codes, `size_mismatch:${item.deliverable_id}`);
      const taskDeliverable = task.deliverables?.find(deliverable => deliverable.id === item.deliverable_id);
      if (!taskDeliverable) { fail(codes, `unknown_deliverable:${item.deliverable_id}`); continue; }
      if (seenDeliverables.has(item.deliverable_id)) fail(codes, `duplicate_deliverable_artifact:${item.deliverable_id}`);
      seenDeliverables.add(item.deliverable_id);
      if (item.name !== taskDeliverable.title) fail(codes, `artifact_name_mismatch:${item.deliverable_id}`);
      if (item.mime_type !== taskDeliverable.mime_type || item.download_content_type !== item.mime_type) fail(codes, `mime_mismatch:${item.deliverable_id}`);
      if (item.source_graph_id !== row.graph_id || !(row.execution_ids ?? []).includes(item.source_execution_id)) fail(codes, `artifact_source_not_current:${item.deliverable_id}`);
      const readable = inspectFormat(item.mime_type, bytes, codes, item.deliverable_id);
      artifacts.set(item.deliverable_id, {...item, readable, bytes});
    } catch (error) {
      fail(codes, `artifact_unreadable_or_invalid:${item.deliverable_id}:${error.code ?? 'read_error'}`);
    }
  }

  for (const deliverable of task.deliverables ?? []) {
    const item = artifacts.get(deliverable.id);
    if (!item) fail(codes, `missing_deliverable:${deliverable.id}`);
  }

  for (const criterion of (task.criteria ?? []).filter(item => item.required)) {
    const check = criterion.check ?? {};
    const item = artifacts.get(check.deliverable_id);
    if (check.kind === 'artifact_exists') {
      if (!item) fail(codes, `required_artifact_missing:${check.deliverable_id}`);
    } else if (check.kind === 'artifact_digest_matches') {
      if (!item || sha256(item.bytes) !== item.sha256) fail(codes, `required_digest_invalid:${check.deliverable_id}`);
    } else if (check.kind === 'required_sections') {
      const text = item?.readable?.text;
      if (typeof text !== 'string') { fail(codes, `required_sections_unreadable:${check.deliverable_id}`); continue; }
      const headings = new Set(text.split(/\r?\n/).map(line => line.replace(/^\s{0,3}#{1,6}\s*/, '').replace(/\s+#+\s*$/, '').trim()));
      for (const heading of check.headings ?? []) if (!headings.has(String(heading).trim())) fail(codes, `missing_heading:${check.deliverable_id}:${heading}`);
    } else if (check.kind === 'source_traceability') {
      if (!check.min_sources || check.min_sources < 1) fail(codes, `invalid_source_threshold:${criterion.id}`);
    } else if (!['artifact_exists', 'artifact_digest_matches'].includes(check.kind)) {
      fail(codes, `unverified_required_criterion:${criterion.id}:${check.kind}`);
    }
  }

  const {report, snapshot} = await readEvaluation(row, root, codes);
  verifyProductEvidence(row, report, snapshot, task, artifacts, codes);
  await verifyExecutionSnapshots(row, root, codes);
  await verifyAudit(row, root, codes);
  if (row.illegal_execution_count !== 0) fail(codes, 'execution_authorization_not_proven');
  if (row.budget_violation_codes?.length) codes.push(...row.budget_violation_codes);
  if (Number.isFinite(row.model_calls) && row.model_calls > (task.budget?.max_model_calls ?? Infinity)) fail(codes, 'model_call_budget_exceeded');
  if (Number.isFinite(row.total_tokens) && row.total_tokens > (task.budget?.max_total_tokens ?? Infinity)) fail(codes, 'total_token_budget_exceeded');
  if (Number.isFinite(row.repair_rounds) && row.repair_rounds > (task.budget?.max_repair_rounds ?? Infinity)) fail(codes, 'repair_round_budget_exceeded');
  if (Number.isFinite(row.active_ms) && row.active_ms > (task.budget?.max_wall_time_ms ?? Infinity)) fail(codes, 'active_time_budget_exceeded');

  return {pass: codes.length === 0, codes: [...new Set(codes)]};
}

export async function verifyDirectory(directory, taskSpecs) {
  const rows = await readJsonl(join(directory, 'runs.jsonl'));
  const tasks = new Map(taskSpecs.map(task => [task.task_id, task]));
  const runIds = new Set();
  const slots = new Set();
  for (const row of rows) {
    if (!row.run_id || runIds.has(row.run_id)) throw new Error(`duplicate_run_id:${row.run_id ?? 'missing'}`);
    runIds.add(row.run_id);
    const key = `${row.task_id}:${row.group}:${row.repeat}`;
    if (slots.has(key)) throw new Error(`duplicate_run_slot:${key}`);
    slots.add(key);
    const task = tasks.get(row.task_id);
    if (!task) throw new Error(`unknown_task:${row.task_id}`);
    const result = await verifyRun(row, task, directory);
    row.judge_pass = result.pass;
    row.judge_codes = result.codes;
    if (!row.started || row.status === 'BLOCKED_ENV' || row.status === 'CANCELLED') continue;
    row.status = result.pass ? 'PASS' : 'FAIL';
  }
  await writeFile(join(directory, 'judged-runs.jsonl'), `${rows.map(row => JSON.stringify(row)).join('\n')}${rows.length ? '\n' : ''}`);
  return rows;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const inputIndex = process.argv.indexOf('--input');
  if (inputIndex < 0 || !process.argv[inputIndex + 1]) throw new Error('--input required');
  const directory = resolve(process.argv[inputIndex + 1]);
  const manifest = await readJson(join(directory, 'manifest.json'));
  const tasksFile = resolveFromRoot(manifest.source_task_file);
  if (sha256(await readFile(tasksFile)) !== manifest.task_sha256) throw new Error('frozen_task_hash_mismatch');
  const taskDocument = await readJson(tasksFile);
  const rows = await verifyDirectory(directory, taskDocument.tasks);
  const allRecordedRunsPassed = rows.length > 0 && rows.every(row => row.started && row.judge_pass === true && row.status === 'PASS');
  const output = {
    status: 'RUNS_JUDGED',
    all_recorded_runs_passed: allRecordedRunsPassed,
    recorded: rows.length,
    pass: rows.filter(row => row.judge_pass === true && row.status === 'PASS').length,
    fail: rows.filter(row => row.status === 'FAIL').length,
    blocked_env: rows.filter(row => row.status === 'BLOCKED_ENV').length,
    not_tested: rows.filter(row => !row.started).length,
  };
  await writeFile(join(directory, 'judge.json'), JSON.stringify(output, null, 2));
  process.stdout.write(`${JSON.stringify(output)}\n`);
  if (!allRecordedRunsPassed) process.exitCode = 1;
}
