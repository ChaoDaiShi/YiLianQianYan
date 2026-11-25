import { copyFile, mkdir } from "node:fs/promises";
import { join, resolve } from "node:path";

// Reuse the full existing isolated product/model/MCP fixture harness.
const root = resolve(import.meta.dirname, "../..");
const evidence = `C:/Users/25113/.codex/visualizations/2026/10/29/01a1881b-f385-7b01-a3a7-270f9de6e401/shared-goal-followup/${process.env.YILIAN_SHARED_GOAL_CONTROLS_ONLY === "1" ? "controls" : "behavior"}`;
await mkdir(evidence, { recursive: true });
const binary = join(evidence, "shared-goal-behavior-server.exe");
await copyFile(join(root, "target/debug/yilian-server.exe"), binary);
process.env.YILIAN_E2E_BACKEND_BINARY = binary;
process.env.YILIAN_SHARED_GOAL_EVIDENCE = evidence;
process.env.YILIAN_SHARED_GOAL_BEHAVIOR = "1";
await import("./shared-goal-state.mjs");
