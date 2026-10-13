import assert from 'node:assert/strict';
import { mkdtempSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { createHash } from 'node:crypto';
import { verifyRunEvidence } from './summarize.mjs';

const root = mkdtempSync(join(tmpdir(), 'proofloop-verifier-'));
const file = join(root, 'report.md');
const text = '# Report\n\n## Background\nActual isolated output.\n';
writeFileSync(file, text);
const sha256 = createHash('sha256').update(text).digest('hex');
const spec = { task_id: 'review', deliverables: [{ id: 'report' }], criteria: [{ id: 'exists', required: true, check: { kind: 'artifact_exists', deliverable_id: 'report' } }, { id: 'digest', required: true, check: { kind: 'artifact_digest_matches', deliverable_id: 'report' } }, { id: 'sections', required: true, check: { kind: 'required_sections', deliverable_id: 'report', headings: ['Background'] } }] };
const run = { task_id: 'review', started: true, status: 'COMPLETED', goal_pass: true, artifacts: [{ deliverable_id: 'report', path: file, sha256 }], illegal_executions: 0 };
const declaredTwoDeliverables = {
  ...spec,
  deliverables: [...spec.deliverables, { id: 'appendix' }],
  criteria: [{ id: 'human', required: true, check: { kind: 'human_review', deliverable_id: 'report' } }],
};
assert.equal(verifyRunEvidence(run, declaredTwoDeliverables).pass, false, 'every declared deliverable must exist even when only one is referenced by a human criterion');
assert.equal(verifyRunEvidence({ ...run, artifacts: [], evidence_paths: [join(root, 'missing.md')], artifact_digest: 'invalid' }, spec).pass, false, 'self-reported pass and a nonexistent path must not pass');
assert.equal(verifyRunEvidence(run, undefined).pass, false, 'unknown task specifications must not pass');
assert.equal(verifyRunEvidence({ ...run, goal_pass: false }, spec).pass, true, 'independent evidence must determine the verdict');
writeFileSync(file, '# Report\nMissing required content.\n');
assert.equal(verifyRunEvidence(run, spec).pass, false, 'changed content and missing headings must fail');
console.log('Independent evidence regression tests passed.');
