// S6b helper: move definitions out of `legacy.ts` into their domain module.
//
// Ownership is decided by *name*: each domain entrypoint already declares the
// names it owns in its `export { ... } from "./legacy"` line, and the section
// comments in `legacy.ts` do not match it (the workflow-graph and workflow-run
// calls sit under the `Health` marker).
//
// Item boundaries come from the TypeScript parser, not from text heuristics —
// a signature with a balanced-brace parameter object and a union type whose
// members end in `;` both defeat brace counting, and silently truncate the item.
//
// Usage: node _drain.mjs <domain-module> "<one-line purpose>"
// Deleted once the drain is complete.

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import ts from "typescript";

const [moduleName, purpose] = process.argv.slice(2);
if (!moduleName) throw new Error("usage: node _drain.mjs <module> \"<purpose>\"");

const dir = path.dirname(fileURLToPath(import.meta.url));
const legacyPath = path.join(dir, "legacy.ts");
const targetPath = path.join(dir, `${moduleName}.ts`);

const legacy = fs.readFileSync(legacyPath, "utf8");
const target = fs.readFileSync(targetPath, "utf8");

// ── which names does this entrypoint own? ──
const values = [];
const types = [];
for (const m of target.matchAll(/^export (type )?\{([^}]*)\} from "\.\/legacy";/gm)) {
  const bucket = m[1] ? types : values;
  for (const raw of m[2].split(",")) {
    const name = raw.trim();
    if (name) bucket.push(name);
  }
}
if (values.length === 0 && types.length === 0) {
  throw new Error(`${moduleName}.ts re-exports nothing from ./legacy`);
}

// ── exact spans, with any leading comment, straight from the parser ──
const source = ts.createSourceFile(legacyPath, legacy, ts.ScriptTarget.Latest, true);
const wanted = new Set([...values, ...types]);
const spans = [];
for (const statement of source.statements) {
  const name = statement.name?.getText(source);
  if (!name || !wanted.has(name)) continue;
  const comments = ts.getLeadingCommentRanges(legacy, statement.getFullStart()) ?? [];
  const start = comments.length ? comments[0].pos : statement.getStart(source);
  spans.push({ name, start, end: statement.getEnd() });
}

const missing = [...wanted].filter((n) => !spans.some((s) => s.name === n));
if (missing.length) throw new Error(`not found in legacy.ts: ${missing.join(", ")}`);

// ── build the domain module ──
const body = spans
  .sort((a, b) => a.start - b.start)
  .map((s) => legacy.slice(s.start, s.end).trimEnd())
  .join("\n\n");

const need = (pattern) => pattern.test(body);
const imports = [];
if (/\bAPI_BASE\b/.test(body)) imports.push('import { API_BASE } from "../core/api/http";');
if (/\brequest\b(?!Result)/.test(body)) imports.push('import { request } from "../core/api/http";');
if (/\brequestResult\b/.test(body)) imports.push('import { requestResult } from "../core/api/http";');
if (/\bApiResult\b/.test(body)) imports.push('import type { ApiResult } from "../core/api/http";');
if (/\bcontrolSessionHeaders\b/.test(body)) imports.push('import { controlSessionHeaders } from "./controlSession";');
if (/\b(AppConfig|LlmModel|LlmModelPayload|LlmUsageReport|ConversationSummary)\b/.test(body)) {
  const used = ["AppConfig", "ConversationSummary", "LlmModel", "LlmModelPayload", "LlmUsageReport"]
    .filter((t) => new RegExp(`\\b${t}\\b`).test(body));
  if (used.length) imports.push(`import type { ${used.join(", ")} } from "../types";`);
}
void need;

fs.writeFileSync(
  targetPath,
  `// ${purpose}\n//\n// Transport is \`core/api/http.ts\`; this module owns these calls rather than\n// borrowing them from the legacy bundle.\n`
    + imports.join("\n") + "\n\n" + body + "\n",
  "utf8",
);

// ── rewrite legacy.ts: drop the moved spans, add one re-export block ──
let out = "";
let cursor = 0;
for (const span of [...spans].sort((a, b) => a.start - b.start)) {
  out += legacy.slice(cursor, span.start);
  cursor = span.end;
}
out += legacy.slice(cursor);

const reexport = [];
if (values.length) reexport.push(`export { ${values.join(", ")} } from "./${moduleName}";`);
if (types.length) reexport.push(`export type { ${types.join(", ")} } from "./${moduleName}";`);

// put it where the first moved definition used to be
const anchor = Math.min(...spans.map((s) => s.start));
const withoutFirst = out.slice(0, anchor) + "\n" + out.slice(anchor);
out = (legacy.slice(0, anchor) + reexport.join("\n") + "\n" + withoutFirst.slice(anchor))
  .replace(/\n{3,}/g, "\n\n")
  .replace(/\n+$/, "\n");

fs.writeFileSync(legacyPath, out, "utf8");
console.log(`${moduleName}: moved ${spans.length} definitions (${values.length} values, ${types.length} types)`);
