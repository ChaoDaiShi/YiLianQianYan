// The frontend tsconfig intentionally has no Node typings; Vitest supplies this module at test time.
// @ts-expect-error -- Node file access is test-only and not bundled.
import { readFileSync } from "node:fs";

/**
 * The whole stylesheet as text, in cascade order.
 *
 * The stylesheet lives in several files under `src/styles/`, so the CSS-facing
 * tests read it through here rather than from one file. Reading is done with
 * Node rather than Vite's `?raw`: Vitest has CSS processing disabled, and a
 * `?raw` CSS import resolves to an empty string there — silently, which is how
 * this was found.
 *
 * This mirrors `index.css`. **If you add a style file, add it here too** —
 * nothing enforces that, because the architecture boundary check only reads
 * `.ts`/`.tsx`.
 *
 * Test-only in practice; the app imports `index.css`, not this module.
 */
function styleFile(name: string): string {
  return readFileSync(new URL(`./${name}`, import.meta.url), "utf8");
}

export const stylesheetText = [
  "tailwind.css",
  "base.css",
  "workspace.css",
  "capability.css",
  "system.css",
  "utilities.css",
  "shell.css",
  "responsive.css",
  "animations.css",
  "markdown.css",
  "workbench.css",
].map(styleFile).join("\n");
