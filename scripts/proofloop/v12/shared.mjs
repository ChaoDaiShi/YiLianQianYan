import {createHash} from 'node:crypto';
import {readFile} from 'node:fs/promises';
import {execFileSync} from 'node:child_process';
import {dirname, isAbsolute, relative, resolve, sep} from 'node:path';
import {fileURLToPath} from 'node:url';

export const scriptDirectory = dirname(fileURLToPath(import.meta.url));
export const repositoryRoot = resolve(scriptDirectory, '../../..');
export const terminalStatuses = new Set(['PASS', 'FAIL', 'BLOCKED_ENV', 'CANCELLED', 'NOT_TESTED']);

export function canonicalJson(value) {
  if (Array.isArray(value)) return `[${value.map(canonicalJson).join(',')}]`;
  if (value && typeof value === 'object') {
    return `{${Object.keys(value).sort().map(key => `${JSON.stringify(key)}:${canonicalJson(value[key])}`).join(',')}}`;
  }
  return JSON.stringify(value);
}

export function sha256(value) {
  return createHash('sha256').update(value).digest('hex');
}

export function schemaHash(descriptor) {
  return sha256(canonicalJson({
    input_schema: descriptor.input_schema ?? null,
    output_schema: descriptor.output_schema ?? null,
  }));
}

export async function readJson(path) {
  return JSON.parse(await readFile(path, 'utf8'));
}

export async function readJsonl(path) {
  const text = await readFile(path, 'utf8');
  return text.split(/\r?\n/).filter(Boolean).map((line, index) => {
    try { return JSON.parse(line); }
    catch (error) { throw new Error(`invalid_jsonl:${path}:${index + 1}:${error.message}`); }
  });
}

export function resolveFromRoot(path) {
  return isAbsolute(path) ? resolve(path) : resolve(repositoryRoot, path);
}

export function isWithin(parent, child) {
  const rel = relative(resolve(parent), resolve(child));
  return rel === '' || (!rel.startsWith(`..${sep}`) && rel !== '..' && !isAbsolute(rel));
}

export function sourceCommit() {
  return execFileSync('git', ['rev-parse', 'HEAD'], {cwd: repositoryRoot, encoding: 'utf8'}).trim();
}

export function sourceTreeDirty() {
  return execFileSync('git', ['status', '--porcelain'], {cwd: repositoryRoot, encoding: 'utf8'}).trim().length > 0;
}

export function parseArguments(argv) {
  const result = {positionals: [], flags: new Set(), values: {}};
  for (let index = 0; index < argv.length; index += 1) {
    const token = argv[index];
    if (!token.startsWith('--')) { result.positionals.push(token); continue; }
    const equal = token.indexOf('=');
    if (equal >= 0) {
      result.values[token.slice(2, equal)] = token.slice(equal + 1);
      continue;
    }
    const name = token.slice(2);
    if (argv[index + 1] && !argv[index + 1].startsWith('--')) {
      result.values[name] = argv[index + 1];
      index += 1;
    } else result.flags.add(name);
  }
  return result;
}

export function appendJsonLine(path, value) {
  return import('node:fs/promises').then(({appendFile}) => appendFile(path, `${JSON.stringify(value)}\n`, 'utf8'));
}

export function redact(value) {
  if (Array.isArray(value)) return value.map(redact);
  if (value && typeof value === 'object') {
    return Object.fromEntries(Object.entries(value).map(([key, item]) => {
      if (/api[_-]?key|token|secret|password|authorization|control_session/i.test(key)) return [key, '[REDACTED_SECRET]'];
      return [key, redact(item)];
    }));
  }
  if (typeof value === 'string') {
    return value
      .replace(/Bearer\s+[A-Za-z0-9._~+\/-]+=*/gi, 'Bearer [REDACTED_SECRET]')
      .replace(/([?&](?:token|api_key|secret)=)[^&\s]+/gi, '$1[REDACTED_SECRET]');
  }
  return value;
}
