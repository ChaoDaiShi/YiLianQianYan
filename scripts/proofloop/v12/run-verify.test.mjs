import assert from 'node:assert/strict';
import {mkdtemp, rm, writeFile} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import test from 'node:test';
import * as runner from './run.mjs';
import {verifyRun} from './verify.mjs';

test('V12 evidence preserves counters, redacts credentials, and gates unproven token budgets', async () => {
  assert.deepEqual(runner.safePayload({
    budget: {max_model_calls: 6, max_total_tokens: 24_000},
    usage: {total_tokens: 1_234, totalTokens: 1_234, prompt_tokens: 1_000, completion_tokens: 234},
    unknown_usage: {total_tokens: null},
    api_key: 'provider-secret',
    accessToken: 'access-secret',
    control_session: 'control-session-secret',
    control_session_token: 'control-secret',
    authorization: 'Bearer bearer-secret',
  }), {
    budget: {max_model_calls: 6, max_total_tokens: 24_000},
    usage: {total_tokens: 1_234, totalTokens: 1_234, prompt_tokens: 1_000, completion_tokens: 234},
    unknown_usage: {total_tokens: null},
    api_key: '[REDACTED_SECRET]',
    accessToken: '[REDACTED_SECRET]',
    control_session: '[REDACTED_SECRET]',
    control_session_token: '[REDACTED_SECRET]',
    authorization: '[REDACTED_SECRET]',
  });

  const root = await mkdtemp(join(tmpdir(), 'v12-usage-judge-'));
  try {
    const task = {budget: {max_model_calls: 6, max_total_tokens: 24_000}, deliverables: [], criteria: []};
    const row = {
      started: true,
      evidence_class: 'REAL_MODEL',
      model_usage_file: 'model-usage.json',
      token_usage_source: 'model_usage_api_cross_checked_against_run_response',
      model_usage_unknown: false,
      total_tokens: 18_000,
      direct_total_tokens: 18_000,
      model_calls: 1,
      artifact_files: [],
      execution_ids: [],
      all_execution_ids: [],
      execution_statuses: [],
    };

    await writeFile(join(root, 'model-usage.json'), JSON.stringify({
      total_tokens: 18_000,
      prompt_tokens: 14_000,
      completion_tokens: 4_000,
      accounting: {
        token_usage_source: 'model_usage_api_cross_checked_against_run_response',
        attributed_total_tokens: 18_000,
        planning_response_usage: {total_tokens: 18_000},
        ai_repair_response_usages: [],
      },
    }));
    const apiMeasured = await verifyRun(row, task, root);
    assert.ok(!apiMeasured.codes.includes('model_usage_evidence_mismatch'));
    assert.ok(!apiMeasured.codes.includes('total_token_budget_exceeded'));

    await writeFile(join(root, 'model-usage.json'), JSON.stringify({
      total_tokens: 0,
      prompt_tokens: 0,
      completion_tokens: 0,
      accounting: {
        token_usage_source: 'planning_response_no_usage_ledger_entry',
        attributed_total_tokens: 1_300,
        planning_response_usage: {total_tokens: 1_000},
        ai_repair_response_usages: [{model_calls: 1, total_tokens: 300}],
      },
    }));
    row.token_usage_source = 'planning_response_no_usage_ledger_entry';
    row.total_tokens = 1_300;
    row.direct_total_tokens = 1_300;
    const responseMeasured = await verifyRun(row, task, root);
    assert.ok(!responseMeasured.codes.includes('model_usage_evidence_mismatch'));
    assert.ok(!responseMeasured.codes.includes('total_token_budget_exceeded'));

    await writeFile(join(root, 'model-usage.json'), JSON.stringify({
      total_tokens: 30_001,
      prompt_tokens: 25_000,
      completion_tokens: 5_001,
      accounting: {
        token_usage_source: 'model_usage_api_cross_checked_against_run_response',
        attributed_total_tokens: 1,
        planning_response_usage: {total_tokens: 1},
        ai_repair_response_usages: [],
      },
    }));
    row.token_usage_source = 'model_usage_api_cross_checked_against_run_response';
    row.total_tokens = 1;
    row.direct_total_tokens = 1;
    const result = await verifyRun(row, task, root);
    assert.ok(result.codes.includes('model_usage_evidence_mismatch'));
    assert.ok(result.codes.includes('total_token_budget_exceeded'));

    row.model_usage_file = undefined;
    row.total_tokens = null;
    row.model_usage_unknown = true;
    const missingEvidence = await verifyRun(row, task, root);
    assert.ok(missingEvidence.codes.includes('token_budget_evidence_unavailable'));
  } finally {
    await rm(root, {recursive: true, force: true});
  }
});
