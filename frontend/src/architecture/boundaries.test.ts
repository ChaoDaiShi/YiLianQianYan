import { describe, expect, it } from "vitest";

/**
 * Lightweight architecture boundary checks (mandate R7).
 *
 * These read source text, so they enforce direction, not types. They exist to
 * catch what a reviewer would otherwise have to remember: a primitive growing
 * a feature dependency, two features reaching into each other, business logic
 * binding itself to Tauri, or a new file quietly crossing the line budget.
 *
 * See `docs/architecture/dependency-rules.md`.
 */

const sources = import.meta.glob("../**/*.{ts,tsx}", {
  query: "?raw",
  import: "default",
  eager: true,
}) as Record<string, string>;

/** Normalised to `src/...` so the assertions read like the tree. */
const files = Object.entries(sources).map(([path, source]) => ({
  path: path.replace(/^\.\.\//, "src/"),
  source,
}));

const production = files.filter((file) => !file.path.includes(".test."));

/**
 * Files already above the 600-line budget when these checks were introduced.
 * They are recorded debt, not approval — a file may leave this list by being
 * split, and nothing may join it.
 *
 * Left the list in R2: `src/api/legacy.ts` (1,335 → 62, now a re-export surface),
 * `src/api/taskWorld.ts` (601 → split into `taskWorld/`), and
 * `src/features/voice/GlobalVoiceHost.tsx` (616 → 576, runtime extracted).
 */
const LINE_BUDGET = 600;
const KNOWN_LARGE_FILES = [
  "src/components/chat/ChatView.tsx",
  "src/pages/PluginsPage.tsx",
];

describe("frontend architecture boundaries", () => {
  it("keeps ui primitives independent of features, pages and stores", () => {
    const offenders = production
      .filter((file) => file.path.startsWith("src/components/ui/"))
      .filter((file) => /from\s+"[^"]*\/(features|pages|stores)\//.test(file.source))
      .map((file) => file.path);
    expect(offenders).toEqual([]);
  });

  it("keeps features independent of each other", () => {
    const featureOf = (path: string) => path.match(/^src\/features\/([^/]+)\//)?.[1];
    const offenders: string[] = [];
    for (const file of production) {
      const own = featureOf(file.path);
      if (!own) continue;
      for (const match of file.source.matchAll(/from\s+"((?:\.\.\/)+features\/[^"]+)"/g)) {
        const target = match[1].match(/features\/([^/]+)/)?.[1];
        if (target && target !== own) offenders.push(`${file.path} -> ${target}`);
      }
    }
    expect(offenders).toEqual([]);
  });

  it("reaches Tauri only through a dynamic import, and never from a feature or api layer", () => {
    const offenders = production
      .filter((file) => !file.path.startsWith("src/surfaces/desktop/"))
      // A static import binds the module to the desktop shell at load time;
      // the dynamic form keeps the surface portable to web and cloud.
      .filter((file) => /^\s*import\s[^;]*from\s+"@tauri-apps\//m.test(file.source))
      .map((file) => file.path);
    expect(offenders).toEqual([]);
  });

  it("does not add new files above the line budget", () => {
    const lines = (source: string) => source.replace(/\n$/, "").split("\n").length;
    const added = production
      .filter((file) => lines(file.source) > LINE_BUDGET)
      .map((file) => file.path)
      .filter((path) => !KNOWN_LARGE_FILES.includes(path));
    expect(added).toEqual([]);
  });

  it("keeps the api compatibility surface free of implementations", () => {
    // `src/api/legacy.ts` is a re-export surface: `api/client.ts` re-exports it
    // so pre-existing imports keep resolving to the same function objects.
    // Declaring anything here again would create a second implementation of a
    // call the domain module already owns, and the two would drift.
    const legacy = production.find((file) => file.path === "src/api/legacy.ts");
    expect(legacy, "src/api/legacy.ts must exist").toBeDefined();
    const declarations = legacy!.source
      .split("\n")
      .filter((line) => /^export (?:async function|function|const|let|class|interface)\b/.test(line)
        || /^export type \w+ =/.test(line))
      .map((line) => line.trim());
    expect(declarations).toEqual([]);
  });
});
