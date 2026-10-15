#!/usr/bin/env node
import { createInterface } from 'node:readline';
import { chromium } from 'playwright-core';
import { browserCapture, browserClick, browserFill, browserRead, browserSubmit, BrowserAdapterError } from './actions.mjs';
import { createEvidenceStore } from './evidence.mjs';
import { BrowserSessions } from './session.mjs';

const VERSION = '0.1.0';
const PROTOCOL_VERSIONS = ['2026-07-28', '2025-11-25'];

function parseArgs(args) {
  const options = { allowedOrigins: [] };
  for (let index = 0; index < args.length; index += 1) {
    const key = args[index];
    if (key === '--allow-origin') options.allowedOrigins.push(args[++index]);
    else if (key === '--browser-executable') options.executablePath = args[++index];
    else if (key === '--evidence-dir') options.evidenceDirectory = args[++index];
    else throw new Error(`unknown argument: ${key}`);
  }
  options.executablePath ??= process.env.BROWSER_EXECUTABLE_PATH;
  if (options.allowedOrigins.length === 0) {
    options.allowedOrigins = (process.env.BROWSER_ALLOWED_ORIGINS ?? '').split(',').map((value) => value.trim()).filter(Boolean);
  }
  if (!options.executablePath) throw new Error('set --browser-executable or BROWSER_EXECUTABLE_PATH to an installed Chromium-compatible browser');
  if (options.allowedOrigins.length === 0) throw new Error('set at least one --allow-origin or BROWSER_ALLOWED_ORIGINS entry');
  options.evidenceDirectory ??= process.env.BROWSER_EVIDENCE_DIR;
  return options;
}

function schema(properties, required = Object.keys(properties)) {
  return { type: 'object', properties, required, additionalProperties: false };
}

const string = { type: 'string' };
const toolDefinitions = [
  ['browser_open', 'Open one allowlisted URL in an adapter-owned browser session.',
    schema({ url: string }), schema({ session_id: string, page_id: string, url: string, title: string, evidence_id: string, evidence_path: string })],
  ['browser_read', 'Read visible page text and structural controls for the current page.',
    schema({ session_id: string, page_id: string }), schema({ observation_id: string, url: string, title: string, text: string, elements: { type: 'array', items: schema({ element_id: string, role: string, name: string, tag: string, enabled: { type: 'boolean' } }) }, evidence_id: string, evidence_path: string })],
  ['browser_fill', 'Fill one uniquely identified textbox from a current observation and verify its value.',
    schema({ session_id: string, page_id: string, observation_id: string, element_id: string, value: string }), schema({ changed: { type: 'boolean' }, value_matches: { type: 'boolean' }, next_observation_id: string, operation_id: string, evidence_id: string, evidence_path: string })],
  ['browser_click', 'Click one uniquely identified structural control from a current observation.',
    schema({ session_id: string, page_id: string, observation_id: string, element_id: string }), schema({ clicked: { type: 'boolean' }, next_observation_id: string, navigation_url: { type: ['string', 'null'] }, operation_id: string, evidence_id: string, evidence_path: string })],
  ['browser_submit', 'Submit one observed form control. The Gateway must classify this tool as requiring approval.',
    schema({ session_id: string, page_id: string, observation_id: string, element_id: string }), schema({ submitted: { type: ['boolean', 'null'] }, result_state: { enum: ['clicked', 'unknown'] }, operation_id: string, final_url: string, next_observation_id: string, error_code: string, started_evidence_id: string, evidence_id: string, evidence_path: string }, ['submitted', 'result_state', 'operation_id', 'final_url', 'evidence_id', 'evidence_path'])],
  ['browser_capture', 'Capture a real PNG screenshot and return its path and SHA-256 digest.',
    schema({ session_id: string, page_id: string }), schema({ screenshot_path: string, sha256: string, observation_id: string, url: string, evidence_id: string, evidence_path: string })],
  ['browser_close', 'Close an adapter-owned browser session and its page context.',
    schema({ session_id: string }), schema({ closed: { type: 'boolean' }, evidence_id: string, evidence_path: string })],
].map(([name, description, inputSchema, outputSchema]) => ({ name, description, inputSchema, outputSchema }));

function textResult(value, isError = false) {
  return {
    content: [{ type: 'text', text: JSON.stringify(value) }],
    structuredContent: value,
    ...(isError ? { isError: true } : {}),
  };
}

async function main() {
  const options = parseArgs(process.argv.slice(2));
  const evidence = await createEvidenceStore(options.evidenceDirectory);
  const sessions = new BrowserSessions({
    chromium,
    executablePath: options.executablePath,
    allowedOrigins: options.allowedOrigins,
    evidence,
  });
  const input = createInterface({ input: process.stdin, crlfDelay: Infinity });

  const respond = (id, body) => process.stdout.write(`${JSON.stringify({ jsonrpc: '2.0', id, ...body })}\n`);
  const callTool = async (name, args) => {
    switch (name) {
      case 'browser_open': return sessions.open(args.url);
      case 'browser_read': return browserRead(sessions, args);
      case 'browser_fill': return browserFill(sessions, args);
      case 'browser_click': return browserClick(sessions, args);
      case 'browser_submit': return browserSubmit(sessions, args);
      case 'browser_capture': return browserCapture(sessions, evidence, args);
      case 'browser_close': return sessions.close(args.session_id);
      default: throw new BrowserAdapterError('browser_tool_not_found', `unknown browser tool: ${name}`);
    }
  };

  input.on('line', (line) => {
    void (async () => {
      let message;
      try {
        message = JSON.parse(line);
      } catch {
        respond(null, { error: { code: -32700, message: 'Parse error' } });
        return;
      }
      if (!message || message.jsonrpc !== '2.0') return;
      if (!Object.hasOwn(message, 'id')) {
        if (message.method === 'exit') void sessions.shutdown();
        return;
      }
      try {
        if (message.method === 'initialize') {
          const requested = message.params?.protocolVersion;
          const protocolVersion = PROTOCOL_VERSIONS.includes(requested) ? requested : PROTOCOL_VERSIONS[0];
          respond(message.id, { result: {
            protocolVersion,
            capabilities: { tools: { listChanged: false } },
            serverInfo: { name: 'yilian-playwright-browser-adapter', version: VERSION },
            instructions: 'Only invocation-allowlisted origins can be opened or reached. Browser sessions and pages are created by this adapter. browser_submit must pass the YiLian Gateway approval policy.',
          } });
          return;
        }
        if (message.method === 'ping') {
          respond(message.id, { result: {} });
          return;
        }
        if (message.method === 'tools/list') {
          respond(message.id, { result: { tools: toolDefinitions } });
          return;
        }
        if (message.method === 'tools/call') {
          try {
            const value = await callTool(message.params?.name, message.params?.arguments ?? {});
            respond(message.id, { result: textResult(value) });
          } catch (error) {
            const value = {
              error_code: error.code ?? 'browser_operation_failed',
              message: error instanceof BrowserAdapterError ? error.message : 'browser operation failed',
              ...(error.evidence ?? {}),
            };
            respond(message.id, { result: textResult(value, true) });
          }
          return;
        }
        respond(message.id, { error: { code: -32601, message: `Method not found: ${message.method}` } });
      } catch (error) {
        respond(message.id, { error: { code: -32603, message: error.message } });
      }
    })();
  });

  input.on('close', () => { void sessions.shutdown(); });
}

try {
  await main();
} catch (error) {
  process.stderr.write(`${error.message}\n`);
  process.exitCode = 2;
}
