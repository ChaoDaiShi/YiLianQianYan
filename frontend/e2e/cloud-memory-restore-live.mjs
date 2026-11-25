import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { once } from "node:events";
import { randomUUID } from "node:crypto";
import { mkdtemp, readFile, mkdir, writeFile, stat } from "node:fs/promises";
import { createServer } from "node:net";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

const root = resolve(import.meta.dirname, "../..");
assert.ok(process.env.YILIAN_TEST_CLOUD_URL, "Set YILIAN_TEST_CLOUD_URL to the isolated cloud API root");
const cloud = new URL(process.env.YILIAN_TEST_CLOUD_URL);
assert.ok(["127.0.0.1", "localhost"].includes(cloud.hostname), "Use an isolated local private cloud instance");
const exe = process.env.YILIAN_E2E_SERVER || join(root, "target/debug/yilian-server.exe");
const temp = await mkdtemp(join(tmpdir(), "yilian-cloud-memory-qa-"));
const evidence = resolve(process.env.YILIAN_CLOUD_MEMORY_EVIDENCE || join(root, "target/cloud-memory-desktop-e2e"));
await mkdir(evidence, { recursive: true });
const processes = [];
const label = randomUUID();
const email = `memory-restore-${label}@example.test`;
const password = `QA-${randomUUID()}-7x`;
const token = `${randomUUID()}${randomUUID()}`;
const targetText = `Synthetic memory to purge ${label}`;
const retainText = `Synthetic neighbor to retain ${label}`;
let stage = "startup";

async function freePort() {
  const socket = createServer();
  socket.listen(0, "127.0.0.1");
  await once(socket, "listening");
  const port = socket.address().port;
  await new Promise(done => socket.close(done));
  return port;
}

async function start(data, workspace) {
  await mkdir(workspace, { recursive: true });
  const url = `http://127.0.0.1:${await freePort()}`;
  const child = spawn(exe, [], { cwd: root, windowsHide: true, stdio: "ignore", env: { ...process.env,
    YILIAN_DATA_DIR: data, YILIAN_WORKSPACE: workspace, YILIAN_HOST: new URL(url).host, YILIAN_CONTROL_SESSION_TOKEN: token,
  } });
  processes.push(child);
  const end = Date.now() + 30000;
  while (Date.now() < end) {
    if (child.exitCode !== null) throw new Error("isolated client exited at startup");
    try { if ((await fetch(url + "/api/health")).ok) return { url, child }; } catch { /* startup */ }
    await new Promise(done => setTimeout(done, 120));
  }
  throw new Error("isolated client was not healthy");
}

async function stop(client) {
  if (client.child.exitCode !== null || client.child.signalCode !== null) return;
  const exited = once(client.child, "exit");
  client.child.kill();
  await exited;
}

async function api(client, method, path, body) {
  const res = await fetch(client.url + path, { method, headers: { "Content-Type": "application/json", "X-Yilian-Control-Session": token }, body: body === undefined ? undefined : JSON.stringify(body) });
  const data = await res.json().catch(() => null);
  assert.ok(res.ok, `${method} ${path}: HTTP ${res.status} ${data?.code || ""}`);
  return data;
}

async function account(name, register) {
  const data = join(temp, name, "data"), workspace = join(temp, name, "workspace");
  let client = await start(data, workspace);
  const auth = await api(client, "POST", `/api/cloud/${register ? "register" : "login"}`, { cloud_base_url: cloud.href, email, password, display_name: "Isolated QA" });
  assert.equal(auth.status, "awaiting_restart");
  await stop(client);
  client = await start(data, workspace);
  const registry = JSON.parse(await readFile(join(data, "profile-registry.json"), "utf8"));
  const db = join(data, "profiles", registry.profiles[registry.active_profile_id].directory_name, "app.db");
  await api(client, "PUT", "/api/cloud/sync/choice", { choice: "sync" });
  return { ...client, data, workspace, db };
}

async function loginAgain(client) {
  const auth = await api(client, "POST", "/api/cloud/login", { cloud_base_url: cloud.href, email, password });
  assert.equal(auth.status, "awaiting_restart");
  await stop(client);
  const restarted = { ...client, ...await start(client.data, client.workspace) };
  await api(restarted, "PUT", "/api/cloud/sync/choice", { choice: "sync" });
  return restarted;
}

async function sqliteCopy(src, dst) {
  const code = "import sqlite3,sys; a=sqlite3.connect(sys.argv[1]); b=sqlite3.connect(sys.argv[2]); a.backup(b); b.close(); a.close()";
  const child = spawn("python", ["-c", code, src, dst], { windowsHide: true, stdio: "ignore" });
  const [exit] = await once(child, "close");
  assert.equal(exit, 0, "SQLite Online Backup API");
}

try {
  stage = "register and login isolated clients";
  let writer = await account("writer", true);
  stage = "synchronize synthetic memories before backup";
  const make = content => api(writer, "POST", "/api/memories", { content, category: "note", source: "manual" });
  const doomed = await make(targetText);
  const retain = await make(retainText);
  const sync = async client => (await api(client, "POST", "/api/cloud/sync/now")).report;
  assert.ok((await sync(writer)).pushed >= 2);
  // The production OS keyring is account-scoped on this Windows login.
  // Each client signs in again before its next remote action on this host.
  let reader = await account("reader", false);
  await sync(reader);
  const list = client => api(client, "GET", "/api/memories");
  assert.ok((await list(reader)).some(item => item.id === doomed.id && item.content === targetText));
  assert.ok((await list(reader)).some(item => item.id === retain.id));
  await stop(reader);
  const snapshot = join(temp, "before-delete.db");
  await sqliteCopy(reader.db, snapshot);
  reader = { ...reader, ...await start(reader.data, reader.workspace) };

  stage = "propagate remote deletion into reader";
  writer = await loginAgain(writer);
  await api(writer, "DELETE", `/api/memories/${doomed.id}`);
  assert.ok((await sync(writer)).pushed >= 1);
  reader = await loginAgain(reader);
  assert.ok((await sync(reader)).pulled >= 1);
  assert.ok(!(await list(reader)).some(item => item.id === doomed.id));
  assert.ok((await stat(`${reader.db}.memory-purge.sqlite`)).size > 0);

  stage = "restore pre-delete reader SQLite snapshot with current deletion journal";
  await stop(reader);
  await sqliteCopy(snapshot, reader.db);
  reader = { ...reader, ...await start(reader.data, reader.workspace) };
  const restored = await list(reader);
  assert.ok(!restored.some(item => item.id === doomed.id || item.content === targetText));
  assert.ok(restored.some(item => item.id === retain.id && item.content === retainText));
  const recall = await api(reader, "GET", `/api/memories/retrieve?${new URLSearchParams({ q: targetText })}`);
  assert.ok(!JSON.stringify(recall).includes(doomed.id));
  reader = await loginAgain(reader);
  await sync(reader);
  assert.ok(!(await list(reader)).some(item => item.id === doomed.id));
  const result = { realPrivateCloud: true, localCloud: cloud.origin, sqliteBackup: true, remoteDeletePulled: true, restoredBeforeSync: true, restoredAfterSync: true, retainedNeighbor: true };
  await writeFile(join(evidence, "result.json"), JSON.stringify(result, null, 2));
  console.log(JSON.stringify(result));
} catch (error) {
  console.error(`[${stage}] ${error.message}`);
  process.exitCode = 1;
} finally {
  for (const child of processes.reverse()) if (child.exitCode === null && child.signalCode === null) child.kill();
}
