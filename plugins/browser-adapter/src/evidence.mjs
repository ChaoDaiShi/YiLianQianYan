import { createHash, randomUUID } from 'node:crypto';
import { appendFile, mkdir, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';

export async function createEvidenceStore(requestedDirectory) {
  const directory = requestedDirectory
    ? resolve(requestedDirectory)
    : join(tmpdir(), `yilian-browser-adapter-${process.pid}`);
  await mkdir(directory, { recursive: true });
  return {
    directory,
    async record(sessionId, event) {
      const evidenceId = randomUUID();
      const path = join(directory, `${sessionId}.jsonl`);
      await appendFile(path, `${JSON.stringify({ evidence_id: evidenceId, ...event })}\n`, 'utf8');
      return { evidence_id: evidenceId, evidence_path: path };
    },
    async storeScreenshot(sessionId, pageId, bytes) {
      const filename = `${sessionId}-${pageId}-${Date.now()}-${randomUUID()}.png`;
      const path = join(directory, filename);
      await writeFile(path, bytes);
      return {
        screenshot_path: path,
        sha256: createHash('sha256').update(bytes).digest('hex'),
      };
    },
  };
}

export function evidenceUrl(value) {
  try {
    const url = new URL(value);
    return `${url.origin}${url.pathname}`;
  } catch {
    return value;
  }
}
