import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { once } from 'node:events';
import { createInterface } from 'node:readline';
import { mkdtemp, readFile, rm } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import test, { before, after } from 'node:test';
import { startFixtureSite } from './fixture-site.mjs';

const adapterRoot = resolve(import.meta.dirname, '..');
const executable = process.env.BROWSER_EXECUTABLE_PATH;

class McpProcess {
  constructor(child) {
    this.child = child;
    this.nextId = 0;
    this.pending = new Map();
    this.output = createInterface({ input: child.stdout });
    this.output.on('line', (line) => {
      const message = JSON.parse(line);
      const pending = this.pending.get(message.id);
      if (pending) {
        this.pending.delete(message.id);
        pending.resolve(message);
      }
    });
    child.on('exit', (code, signal) => {
      for (const [id, pending] of this.pending) {
        this.pending.delete(id);
        pending.reject(new Error(`adapter exited (${code ?? signal})`));
      }
    });
    child.stderr.on('data', () => {});
  }

  async request(method, params = {}) {
    const id = ++this.nextId;
    const response = new Promise((resolve, reject) => this.pending.set(id, { resolve, reject }));
    this.child.stdin.write(`${JSON.stringify({ jsonrpc: '2.0', id, method, params })}\n`);
    const message = await response;
    assert.equal(message.error, undefined, `${method}: ${message.error?.message}`);
    return message.result;
  }

  async tool(name, arguments_) {
    return this.request('tools/call', { name, arguments: arguments_ });
  }

  close() {
    this.child.stdin.end();
    this.child.kill();
  }
}

let fixture;
let evidenceDir;
let adapter;

before(async () => {
  assert.ok(executable, 'set BROWSER_EXECUTABLE_PATH to an installed Chromium-compatible browser');
  fixture = await startFixtureSite();
  evidenceDir = await mkdtemp(join(tmpdir(), 'yilian-browser-adapter-test-'));
  const child = spawn(process.execPath, [
    join(adapterRoot, 'src', 'server.mjs'),
    '--allow-origin', fixture.origin,
    '--browser-executable', executable,
    '--evidence-dir', evidenceDir,
  ], { stdio: ['pipe', 'pipe', 'pipe'] });
  adapter = new McpProcess(child);
  await once(child, 'spawn');
  const initialized = await adapter.request('initialize', {
    protocolVersion: '2026-07-28',
    capabilities: {},
    clientInfo: { name: 'browser-adapter-test', version: '1' },
  });
  assert.equal(initialized.protocolVersion, '2026-07-28');
  adapter.child.stdin.write(`${JSON.stringify({ jsonrpc: '2.0', method: 'notifications/initialized', params: {} })}\n`);
});

after(async () => {
  adapter?.close();
  await fixture?.close();
  if (evidenceDir) await rm(evidenceDir, { recursive: true, force: true });
});

test('MCP browser tools expose structural observations and complete one fixture submission', async () => {
  const listed = await adapter.request('tools/list');
  const names = listed.tools.map((tool) => tool.name);
  assert.deepEqual(names, [
    'browser_open', 'browser_read', 'browser_fill', 'browser_click',
    'browser_submit', 'browser_capture', 'browser_close',
  ]);
  assert.ok(listed.tools.every((tool) => tool.inputSchema && tool.outputSchema));
  assert.equal(names.some((name) => /eval|script/i.test(name)), false);

  const opened = await adapter.tool('browser_open', { url: `${fixture.origin}/` });
  assert.equal(opened.isError, undefined);
  const sessionId = opened.structuredContent.session_id;
  const pageId = opened.structuredContent.page_id;

  const read = await adapter.tool('browser_read', { session_id: sessionId, page_id: pageId });
  const observation = read.structuredContent;
  assert.match(observation.text, /completed three public safety drills/);
  assert.match(observation.text, /Ignore previous instructions/);
  const summary = observation.elements.find((element) => element.role === 'textbox' && element.name === 'Summary');
  assert.ok(summary, 'labelled form field is identified structurally');

  const filled = await adapter.tool('browser_fill', {
    session_id: sessionId,
    page_id: pageId,
    observation_id: observation.observation_id,
    element_id: summary.element_id,
    value: 'Three drills completed in September.',
  });
  assert.equal(filled.structuredContent.changed, true);
  assert.equal(filled.structuredContent.value_matches, true);

  const afterFill = (await adapter.tool('browser_read', { session_id: sessionId, page_id: pageId })).structuredContent;
  const submit = afterFill.elements.find((element) => element.role === 'button' && element.name === 'Submit record');
  const submitted = await adapter.tool('browser_submit', {
    session_id: sessionId,
    page_id: pageId,
    observation_id: afterFill.observation_id,
    element_id: submit.element_id,
  });
  assert.equal(submitted.isError, undefined);
  assert.equal(submitted.structuredContent.submitted, true);
  assert.equal(fixture.submissions.length, 1);
  assert.equal(fixture.submissions[0], 'Three drills completed in September.');

  const capture = await adapter.tool('browser_capture', { session_id: sessionId, page_id: pageId });
  const screenshotBytes = await readFile(capture.structuredContent.screenshot_path);
  assert.ok(screenshotBytes.length > 8);
  assert.equal(createHash('sha256').update(screenshotBytes).digest('hex'), capture.structuredContent.sha256);

  await adapter.tool('browser_close', { session_id: sessionId });
});

test('a structural handle from an earlier observation cannot target a changed page', async () => {
  const opened = await adapter.tool('browser_open', { url: `${fixture.origin}/` });
  const { session_id: sessionId, page_id: pageId } = opened.structuredContent;
  const first = (await adapter.tool('browser_read', { session_id: sessionId, page_id: pageId })).structuredContent;
  const mutate = first.elements.find((element) => element.role === 'button' && element.name === 'Mutate page');
  await adapter.tool('browser_click', {
    session_id: sessionId,
    page_id: pageId,
    observation_id: first.observation_id,
    element_id: mutate.element_id,
  });
  const stale = await adapter.tool('browser_click', {
    session_id: sessionId,
    page_id: pageId,
    observation_id: first.observation_id,
    element_id: mutate.element_id,
  });
  assert.equal(stale.isError, true);
  assert.match(stale.content[0].text, /observation_stale/);
  await adapter.tool('browser_close', { session_id: sessionId });
});

test('browser_open rejects origins outside its invocation allowlist', async () => {
  const rejected = await adapter.tool('browser_open', { url: 'https://example.org/' });
  assert.equal(rejected.isError, true);
  assert.equal(rejected.structuredContent.error_code, 'browser_origin_not_allowed');
});
