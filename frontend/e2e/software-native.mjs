// Actual isolated Tauri/WebView2 + owned Win32 target windows. No real WeChat,
// external account, existing browser profile, or paid model is used.
import assert from "node:assert/strict";
import {spawn, execFileSync} from "node:child_process";
import {createServer} from "node:http";
import {mkdtemp, mkdir, appendFile, writeFile, readFile, stat} from "node:fs/promises";
import {createHash, randomUUID} from "node:crypto";
import {join, resolve, basename} from "node:path";
import {tmpdir} from "node:os";
import {chromium} from "playwright-core";
import {connect} from "node:net";

assert.equal(process.platform, "win32", "This acceptance suite requires Windows native windows");
assert.equal(process.env.YILIAN_SOFTWARE_BUILD_CONFIRMED, "1", "Build the current fixture binaries first; never substitute an old/user app");
const application = resolve(process.env.YILIAN_NATIVE_BROWSER_BINARY || "");
const targetBinary = resolve(process.env.YILIAN_SOFTWARE_FIXTURE_BINARY || "");
assert.equal(basename(application), "yilian-native-browser-fixture.exe");
assert.equal(basename(targetBinary), "yilian-software-fixture.exe");
const root = resolve(import.meta.dirname, "../..");
const isolated = await mkdtemp(join(tmpdir(), "yilian-software-native-"));
const evidence = join(isolated, "evidence"), workspace = join(isolated, "workspace");
await Promise.all([mkdir(evidence), mkdir(workspace), mkdir(join(isolated, "data"))]);
console.log(`[software-native] evidence ${evidence}`);
const backendPort = await freePort(), frontendPort = await freePort(), debugPort = await freePort();
const base = `http://127.0.0.1:${backendPort}`, frontend = `http://127.0.0.1:${frontendPort}`;
const children = [], logs = [], checks = [], geometry = [], diagnostics = [], requests = [], modelCalls = [], modelErrors = [], toolEvidence = [], approvalEvidence = [];
const started = Date.now();
const acceptanceStage = process.env.YILIAN_SOFTWARE_VERIFY_STAGE || "intermediate-debug";
const startupDiagnostic = process.env.YILIAN_SOFTWARE_STARTUP_DIAGNOSTIC === "1";
const targetAfterApp = startupDiagnostic || process.env.YILIAN_SOFTWARE_TARGET_AFTER_APP === "1";
const diagnosticComplete = Symbol("startup-diagnostic-complete");
const debugProbes = [];
let browser, main, app, target, token, registration, project, sessionId, targetA, targetB, model;
let phase = "startup", failure;
const delay = ms => new Promise(r => setTimeout(r, ms));
function check(name, details = {}) { checks.push({name, at: Date.now(), ...details}); console.log(`[software-native] PASS ${name}`); }
async function freePort() { const server = createServer(); await new Promise(r => server.listen(0, "127.0.0.1", r)); const port = server.address().port; await new Promise(r => server.close(r)); return port; }
async function until(read, accept, label, timeout = 30000) {
 const deadline = Date.now() + timeout; let value;
 while (Date.now() < deadline) {
  try { value = await read(); if (accept(value)) return value; } catch (error) { value = String(error); }
  await delay(150);
 }
 throw new Error(`${label}: ${JSON.stringify(value)}`);
}
function launch(command, args, env, label) {
 const child = spawn(command, args, {cwd: label === "vite" ? join(root, "frontend") : root, env: {...process.env, ...env}, windowsHide: true, stdio: ["ignore", "pipe", "pipe"]});
 children.push(child);
 for (const stream of [child.stdout, child.stderr]) stream.on("data", bytes => { const line = `[${label}] ${bytes}`; logs.push(line); void appendFile(join(evidence, "services-live.log"), line); });
 child.on("error", error => logs.push(`[${label}] ${error}\n`));
 child.on("exit", (code, signal) => logs.push(`[${label}] exit ${code} ${signal}\n`));
 void appendFile(join(evidence, "owned-processes.jsonl"), JSON.stringify({label, pid: child.pid}) + "\n");
 return child;
}
async function startTarget() {
 target = launch(targetBinary, ["--target"], {}, "owned-target");
 let ready = ""; target.stdout.on("data", bytes => {ready += bytes;});
 await until(() => ready, text => /READY:\d+:\d+/.test(text), "owned Win32 target ready");
 const match = ready.match(/READY:(\d+):(\d+)/); targetA = match[1]; targetB = match[2];
}
async function ownedMainWindow(captureName) {
 if (!app?.pid || app.exitCode !== null) return {unavailable: "Owned application already exited"};
 const script = `
$ErrorActionPreference='Stop'
[Console]::OutputEncoding=[Text.UTF8Encoding]::new($false)
$ownedId=[int]$env:YILIAN_OWNED_APP_PID
$ownedProcess=Get-Process -Id $ownedId -ErrorAction Stop
if(-not [string]::Equals($ownedProcess.Path,$env:YILIAN_OWNED_APP_BINARY,[StringComparison]::OrdinalIgnoreCase)){throw 'Owned executable mismatch; no window queried'}
Add-Type -AssemblyName System.Drawing
Add-Type -ReferencedAssemblies System.Drawing -TypeDefinition @'
using System; using System.Text; using System.Collections.Generic; using System.Runtime.InteropServices;
public static class OwnedMainProbe {
 public delegate bool Callback(IntPtr h,IntPtr p);
 [StructLayout(LayoutKind.Sequential)] public struct R { public int Left,Top,Right,Bottom; }
 [StructLayout(LayoutKind.Sequential)] public struct P { public int X,Y; }
 [DllImport("user32.dll")] static extern bool EnumWindows(Callback c,IntPtr p);
 [DllImport("user32.dll")] static extern bool EnumChildWindows(IntPtr h,Callback c,IntPtr p);
 [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr h,out uint p);
 [DllImport("user32.dll",CharSet=CharSet.Unicode)] static extern int GetClassName(IntPtr h,StringBuilder t,int n);
 [DllImport("user32.dll",CharSet=CharSet.Unicode)] static extern int GetWindowText(IntPtr h,StringBuilder t,int n);
 [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr h);
 [DllImport("user32.dll")] static extern bool IsHungAppWindow(IntPtr h);
 [DllImport("user32.dll")] static extern bool GetWindowRect(IntPtr h,out R r);
 [DllImport("user32.dll")] static extern uint GetDpiForWindow(IntPtr h);
 [DllImport("user32.dll")] static extern IntPtr GetAncestor(IntPtr h,uint flags);
 [DllImport("user32.dll")] static extern bool PrintWindow(IntPtr h,IntPtr dc,uint flags);
 [DllImport("user32.dll")] static extern IntPtr WindowFromPoint(P p);
 [DllImport("user32.dll")] static extern bool IsChild(IntPtr parent,IntPtr child);
 [DllImport("user32.dll",SetLastError=true)] static extern IntPtr SendMessageTimeout(IntPtr h,uint m,IntPtr w,IntPtr l,uint f,uint t,out UIntPtr r);
 public class Info { public string hwnd,cls,title,parent,nativeParent; public uint pid,dpi; public bool visible,hung,responds,ownsCenter; public int left,top,right,bottom,error; }
 static Info Read(IntPtr h,string parent,bool response) {uint pid;GetWindowThreadProcessId(h,out pid);var cls=new StringBuilder(256);GetClassName(h,cls,cls.Capacity);var title=new StringBuilder(512);GetWindowText(h,title,title.Capacity);R r;GetWindowRect(h,out r);var hit=WindowFromPoint(new P{X=(r.Left+r.Right)/2,Y=(r.Top+r.Bottom)/2});UIntPtr answer=UIntPtr.Zero;var ok=!response||SendMessageTimeout(h,0,IntPtr.Zero,IntPtr.Zero,3,2000,out answer)!=IntPtr.Zero;var error=ok?0:Marshal.GetLastWin32Error();return new Info{hwnd=h.ToInt64().ToString(),cls=cls.ToString(),title=title.ToString(),parent=parent,nativeParent=GetAncestor(h,1).ToInt64().ToString(),pid=pid,dpi=GetDpiForWindow(h),visible=IsWindowVisible(h),hung=IsHungAppWindow(h),responds=ok,ownsCenter=hit==h||IsChild(h,hit),left=r.Left,top=r.Top,right=r.Right,bottom=r.Bottom,error=error};}
 public static Info[] Probe(uint owned) {var list=new List<Info>();EnumWindows((h,p)=>{uint pid;GetWindowThreadProcessId(h,out pid);if(pid!=owned)return true;var top=Read(h,null,true);list.Add(top);EnumChildWindows(h,(child,ignored)=>{if(list.Count<64)list.Add(Read(child,top.hwnd,false));return list.Count<64;},IntPtr.Zero);return list.Count<64;},IntPtr.Zero);return list.ToArray();}
 public static void Capture(Info[] windows,string path) {foreach(var info in windows){if(info.parent!=null||info.cls!="Tauri Window"||!info.visible)continue;var h=new IntPtr(long.Parse(info.hwnd));using(var bitmap=new System.Drawing.Bitmap(info.right-info.left,info.bottom-info.top)){using(var graphics=System.Drawing.Graphics.FromImage(bitmap)){var dc=graphics.GetHdc();try{if(!PrintWindow(h,dc,2))throw new Exception("Owned PrintWindow failed");}finally{graphics.ReleaseHdc(dc);}}bitmap.Save(path,System.Drawing.Imaging.ImageFormat.Png);}return;}throw new Exception("Owned main window missing");}
}
'@
$windows=@([OwnedMainProbe]::Probe([uint32]$ownedId))
if($env:YILIAN_OWNED_CAPTURE_PATH){[OwnedMainProbe]::Capture($windows,$env:YILIAN_OWNED_CAPTURE_PATH)}
@{pid=$ownedId;windows=$windows}|ConvertTo-Json -Depth 5 -Compress
`;
 const child = launch("powershell.exe", ["-NoProfile", "-NonInteractive", "-WindowStyle", "Hidden", "-EncodedCommand", Buffer.from(script, "utf16le").toString("base64")], {YILIAN_OWNED_APP_PID: String(app.pid), YILIAN_OWNED_APP_BINARY: application, YILIAN_OWNED_CAPTURE_PATH: captureName ? join(evidence, captureName) : ""}, "owned-window-probe");
 let stdout = "", stderr = ""; child.stdout.on("data", chunk => stdout += chunk); child.stderr.on("data", chunk => stderr += chunk);
 const code = await Promise.race([new Promise((resolve, reject) => {child.once("exit", resolve);child.once("error", reject);}), delay(15000).then(() => {child.kill();throw new Error("Owned window probe timed out");})]);
 if (code !== 0) throw new Error(`Owned window probe failed (${code}): ${stderr.slice(0, 1500)}`);
 return JSON.parse(stdout.trim().replace(/^\uFEFF/, ""));
}
async function readDebugTargets() {
 const at = Date.now();
 try {
  const response = await fetch(`http://127.0.0.1:${debugPort}/json/list`, {signal: AbortSignal.timeout(3000)});
  const body = await response.json(); debugProbes.push({at, elapsed: Date.now()-at, status: response.status, targets: Array.isArray(body) ? body.map(item => ({type:item.type,url:item.url})) : null});return body;
 } catch(error) {debugProbes.push({at, elapsed: Date.now()-at, error:String(error), cause:error.cause?.code, name:error.name});throw error;}
}
async function api(method, path, body, expected = 200) {
 const response = await fetch(base + path, {method, signal: AbortSignal.timeout(15000), headers: {"Content-Type": "application/json", "X-Yilian-Control-Session": token}, body: body === undefined ? undefined : JSON.stringify(body)});
 const text = await response.text();
 assert.equal(response.status, expected, `${method} ${path}: ${text}`);
 return text ? JSON.parse(text) : null;
}
async function snapshot() { return api("GET", "/api/software"); }
async function current(id = sessionId) { return (await snapshot()).sessions.find(item => item.id === id); }
async function mode(expected) { return until(() => current(), item => item?.window.mode === expected, `native window mode ${expected}`); }
async function state(expected) { return until(() => current(), item => item?.state === expected, `session state ${expected}`); }
async function click(name) { const button = main.getByRole("button", {name, exact: true}); await until(() => button.isEnabled(), Boolean, `${name} ready`); await button.click(); }
async function showIndependent() {
 const response = main.waitForResponse(response => response.url() === `${base}/api/software/sessions/${sessionId}/present`
  && response.request().method() === "POST" && response.request().postDataJSON()?.mode === "focus");
 await click("独立显示");
 const completed = await response, body = await completed.json();
 assert.equal(completed.status(), 200, JSON.stringify(body));
 assert.equal(body.state, "active", "Showing the independent window must not create an unknown-result barrier");
 assert.equal(body.window.visible, true, "The independent window must finish showing before success");
 assert.equal(body.window.mode, "independent");
 return body;
}
async function refresh() { await click("刷新软件状态"); await delay(150); }
async function shot(name) { await main.screenshot({path: join(evidence, name), animations: "disabled", timeout: 10000}); }
async function attachViaUi(hwnd) {
 await click(/^(重新)?连接软件$/);
 const dialog = main.getByRole("dialog", {name: "选择已运行窗口"}); await dialog.waitFor();
 const rows = dialog.locator(".software-candidate");
 const ownedRows = rows.filter({hasText: `PID ${target.pid} · YiLianSoftwareOwnedFixture`});
 assert.equal(await ownedRows.count(), 2, "Owned target has exactly two distinct controlled fixture windows");
 assert.equal(await dialog.getByRole("button", {name: "连接所选窗口"}).isDisabled(), true, "No instance may be implicitly selected");
 const chosen = rows.filter({hasText: new RegExp(`HWND ${hwnd}(?:\\D|$)`)}); assert.equal(await chosen.count(), 1);
 await chosen.getByRole("radio").check(); await dialog.getByRole("button", {name: "连接所选窗口"}).click();
 await dialog.waitFor({state: "hidden"});
 const attached = await until(async () => (await snapshot()).sessions.find(item => item.registration_id === registration.id && item.window.identity.hwnd === hwnd && item.state === "active"), Boolean, "chosen HWND attached");
 sessionId = attached.id; return attached;
}
async function releaseViaUi() {
 await click("断开会话…"); const dialog = main.getByRole("dialog", {name: "断开软件会话"});
 await dialog.getByRole("button", {name: "确认断开"}).click(); await state("released");
 await dialog.waitFor({state: "hidden"});
}
async function cssGeometry(label, width) {
 await main.setViewportSize({width, height: 844}); await delay(180);
 const value = await main.evaluate(() => ({client: document.documentElement.clientWidth, scroll: document.documentElement.scrollWidth,
  dialogs: [...document.querySelectorAll('[role="dialog"]')].map(node => ({client: node.clientWidth, scroll: node.scrollWidth})),
  buttons: [...document.querySelectorAll(".software-page button")].filter(node => {const r = node.getBoundingClientRect();return r.width && r.height;}).map(node => {const r = node.getBoundingClientRect();return {name: node.textContent, left: r.left, right: r.right};})}));
 assert.ok(value.scroll <= value.client + 2, `${label} ${width}: horizontal page overflow`);
 for (const dialog of value.dialogs) assert.ok(dialog.scroll <= dialog.client + 2, `${label} ${width}: dialog overflow`);
 for (const button of value.buttons) assert.ok(button.right <= value.client + 2 && button.left >= -2, `${label} ${width}: unreachable ${button.name}`);
 geometry.push({label, requestedCssWidth: width, ...value, boundary: "CSS stress within actual WebView2; physical desktop minimum unchanged"});
 await shot(`${label}-${width}.png`);
}
async function closeOwned(child) {
 if (child.exitCode !== null || child.signalCode !== null) return;
 child.kill();
 await Promise.race([new Promise(r => child.once("exit", r)), delay(5000)]);
 if (child.exitCode === null && child.signalCode === null) {child.kill("SIGKILL"); await Promise.race([new Promise(r => child.once("exit", r)), delay(3000)]);}
 if (child.exitCode === null && child.signalCode === null) throw new Error(`Owned child ${child.pid} did not exit`);
}
async function sourceManifest() {
 const files = ["backend/src/software/model.rs", "backend/src/software/mod.rs", "backend/src/software/store.rs", "backend/src/software/operations.rs", "backend/src/software/actions.rs", "backend/src/software/lifecycle.rs", "backend/src/software/management.rs", "backend/src/api/software.rs", "backend/src/tools/software.rs", "backend/src/safety/descriptor.rs", "backend/src/agent/verifier.rs", "src-tauri/src/software/identity.rs", "src-tauri/src/software/mount.rs", "src-tauri/src/software/automation.rs", "src-tauri/src/software_bridge.rs", "src-tauri/src/bin/software_fixture.rs", "src-tauri/src/main.rs", "frontend/src/api/software.ts", "frontend/src/features/software/SoftwarePage.tsx", "frontend/src/features/software/softwareForm.ts", "frontend/src/features/software/SoftwareRegistrationDialog.tsx", "frontend/e2e/software-native.mjs"];
 const sources = await Promise.all(files.map(async path => ({path, sha256: createHash("sha256").update(await readFile(join(root, path))).digest("hex")})));
 const git = args => execFileSync("git", args, {cwd: root, encoding: "utf8", stdio: ["ignore", "pipe", "ignore"]}).trim();
 return {at: new Date().toISOString(), branch: git(["branch", "--show-current"]), head: git(["rev-parse", "HEAD"]), dirty: Boolean(git(["status", "--porcelain=v1"])), sources};
}
async function portOpen(port) {
 return new Promise(resolve => {const socket = connect({host: "127.0.0.1", port});let done = false;const finish = value => {if (done) return;done = true;socket.destroy();resolve(value);};socket.once("connect", () => finish(true));socket.once("error", () => finish(false));socket.setTimeout(750, () => finish(false));});
}
function parseTool(message) {
 const text = typeof message.content === "string" ? message.content : JSON.stringify(message.content);
 for (let index = 0; index < text.length; ++index) if (text[index] === "{") {
  try {return JSON.parse(text.slice(index));} catch {}
 }
 throw new Error("Software tool content must expose bounded machine-readable evidence to the model");
}
async function createModel() {
 model = createServer(async (request, response) => {
  try {
   let text = ""; for await (const chunk of request) text += chunk;
   const input = JSON.parse(text);
   if (!input.stream) {response.writeHead(200, {"Content-Type": "application/json"});response.end(JSON.stringify({choices: [{message: {role: "assistant", content: "Controlled software verification"}, finish_reason: "stop"}], usage: {prompt_tokens: 5, completion_tokens: 5, total_tokens: 10}}));return;}
   const previous = input.messages.filter(item => item.role === "tool");
   const users = input.messages.filter(item => item.role === "user").map(item => typeof item.content === "string" ? item.content : JSON.stringify(item.content)).join("\n");
   const scenario = users.includes("SOFTWARE_NATIVE_UNKNOWN") ? "unknown" : users.includes("SOFTWARE_NATIVE_CANCEL") ? "cancel" : "focus";
   let tool;
   if (previous.length === 0) tool = ["software_sessions", {}];
   else if (previous.length === 1) {
    const listing = parseTool(previous[0]); const session = listing.sessions.find(item => item.registration_id === registration.id && item.window.identity.hwnd === targetA);
    assert.ok(session, "Model derives the real authorized software session from listing");
    tool = ["software_observe", {target: {session_id: session.id, generation: session.generation}}];
   } else if (previous.length === 2) {
    const observation = parseTool(previous[1]).observation;
    assert.ok(observation?.id && observation.target);
    assert.ok(observation.controls.every(item => !item.is_password));
    assert.ok(!observation.controls.some(item => item.automation_id === "303"));
    const control = observation.controls.find(item => item.role === "button" && ["Apply", "Applied"].includes(item.name));
    assert.ok(control, "Real UIA observation contains the controlled native button");
    toolEvidence.push({scenario, observation_id: observation.id, target: observation.target, controls: observation.controls});
    tool = ["software_control", {target: observation.target, observation_id: observation.id, element_id: control.id, action: scenario === "focus" ? "focus" : "invoke", operation_id: randomUUID()}];
   } else if (previous.length === 3 && scenario === "focus") {
    const completed = parseTool(previous[2]);
    assert.equal(completed.session?.window.identity.hwnd, targetA, "Focus completion comes from the original real HWND");
    assert.equal(completed.session?.window.identity.pid, target.pid);
   }
   if (tool) {
    const advertised = (input.tools || []).map(item => item.function.name);
    assert.ok(advertised.includes(tool[0]), `Expected ${tool[0]} among the selected turn's tools: ${advertised.join(", ")}`);
   }
   modelCalls.push({scenario, tool: tool?.[0] || "done", prior: previous.length, arguments: tool?.[1]});
   const delta = tool ? {role: "assistant", tool_calls: [{index: 0, id: "controlled-software-" + modelCalls.length, type: "function", function: {name: tool[0], arguments: JSON.stringify(tool[1])}}]} : {content: "The controlled native interaction completed; business outcomes are not inferred."};
   response.writeHead(200, {"Content-Type": "text/event-stream"});
   response.end(`data: ${JSON.stringify({id: "controlled-software-model", choices: [{index: 0, delta, finish_reason: null}]})}\n\ndata: ${JSON.stringify({id: "controlled-software-model", choices: [{index: 0, delta: {}, finish_reason: tool ? "tool_calls" : "stop"}], usage: {prompt_tokens: 20, completion_tokens: 20, total_tokens: 40}})}\n\ndata: [DONE]\n\n`);
  } catch (error) {modelErrors.push(String(error.stack || error));if (!response.headersSent) response.writeHead(500);response.end("Controlled software fixture failed; inspect local evidence");}
 });
 await new Promise(resolve => model.listen(0, "127.0.0.1", resolve)); return model.address().port;
}
async function sendChat(conversation, message) {
 const response = await fetch(base + "/api/chat", {method: "POST", signal: AbortSignal.timeout(45000), headers: {"Content-Type": "application/json", "X-Yilian-Control-Session": token}, body: JSON.stringify({conversation_id: conversation.id, workspace_mode: "work", agent_id: "interaction-general", message})});
 assert.equal(response.status, 200, await response.clone().text()); await response.text();
}
async function pendingFor(conversation) {
 return until(async () => (await api("GET", "/api/approvals/pending")).approvals.find(item => item.conversation_id === conversation.id), Boolean, "persisted software Gateway approval");
}
async function approve(conversation, expected) {
 const pending = await pendingFor(conversation); assert.equal(pending.tool_name, expected);
 const response = await fetch(`${base}/api/approvals/${pending.approval_id}/approve`, {method: "POST", signal: AbortSignal.timeout(45000), headers: {"Content-Type": "application/json", "X-Yilian-Control-Session": token}, body: JSON.stringify({conversation_id: conversation.id})});
 assert.equal(response.status, 200, await response.clone().text()); await response.text();
 approvalEvidence.push({scenario: phase, tool: expected, approval_id: pending.approval_id, status: "approved"}); return pending;
}
async function aiCoverage() {
 const settings = await api("GET", "/api/settings"), port = await createModel();
 await api("PUT", "/api/settings", {...settings, model: {...settings.model, provider: "openai", name: "controlled-native-software", base_url: `http://127.0.0.1:${port}`, api_key_env: "YILIAN_NATIVE_FIXTURE_KEY", api_key_ref: null, temperature: 0, max_tokens: 2048, invoke_timeout_ms: 10000}, permissions: {...settings.permissions, mode: "full_approval"}});
 const allowed = await api("PUT", `/api/software/registrations/${registration.id}`, {revision: registration.revision, config: {name: registration.name, adapter: registration.adapter, executable: registration.executable, workspace_id: project.id, allow_observe: true, allow_actions: true}});
 registration = allowed;
 await refresh(); await attachViaUi(targetA); await showIndependent(); await mode("independent");
 phase = "ai-focus";
 const focus = await api("POST", "/api/conversations", {title: "Controlled native software focus", workspace_id: project.id});
 await api("PUT", `/api/conversations/${focus.id}/workspace`, {workspace_id: project.id});
 await sendChat(focus, "SOFTWARE_NATIVE_FOCUS: list authorized software, observe its controlled button, focus the real button.");
 const controlsBefore = (await snapshot()).history.filter(item => item.action === "control").length;
 for (const name of ["software_sessions", "software_observe"]) await approve(focus, name);
 assert.equal((await snapshot()).history.filter(item => item.action === "control").length, controlsBefore, "No native control before its approval");
 const observedEvidence = (await snapshot()).capability_evidence.find(item => item.session_id === sessionId);
 assert.equal(observedEvidence?.entries.find(item => item.dimension === "perceive")?.status, "verified");
 assert.equal(observedEvidence?.entries.find(item => item.dimension === "operate")?.status, "unverified");
 await approve(focus, "software_control");
 assert.equal((await current()).state, "active");
 const focusedOperation = modelCalls.find(item => item.scenario === "focus" && item.tool === "software_control").arguments.operation_id;
 assert.equal((await snapshot()).history.find(item => item.operation_id === focusedOperation)?.state, "verified", "Native HasKeyboardFocus evidence must verify the controlled focus");
 const controlEvidence = (await snapshot()).capability_evidence.find(item => item.session_id === sessionId);
 assert.equal(controlEvidence?.entries.find(item => item.dimension === "operate")?.evidence_id, focusedOperation);
 assert.equal(controlEvidence?.entries.find(item => item.dimension === "background_interaction")?.status, "unverified");
 assert.deepEqual(modelCalls.filter(item => item.scenario === "focus").map(item => item.tool), ["software_sessions", "software_observe", "software_control", "done"]);
 check("local model derives real native session and UIA button from tool replies; three Gateway approvals focus it without reading password values");

 phase = "ai-unknown";
 const unknown = await api("POST", "/api/conversations", {title: "Controlled native Invoke unknown outcome", workspace_id: project.id});
 await api("PUT", `/api/conversations/${unknown.id}/workspace`, {workspace_id: project.id});
 await sendChat(unknown, "SOFTWARE_NATIVE_UNKNOWN: invoke only the controlled Apply button once, preserve unknown business result.");
 for (const name of ["software_sessions", "software_observe", "software_control"]) await approve(unknown, name);
 await state("result_unknown");
 const invocation = modelCalls.find(item => item.scenario === "unknown" && item.tool === "software_control");
 assert.ok(invocation);
 const receipt = (await snapshot()).history.find(item => item.operation_id === invocation.arguments.operation_id);
 assert.equal(receipt.state, "result_unknown");
 assert.equal(modelCalls.filter(item => item.scenario === "unknown" && item.tool === "software_control").length, 1);
 await refresh(); await click("核对未知结果…");
 let dialog = main.getByRole("dialog", {name: "核对未知结果"});
 await dialog.getByRole("checkbox").check(); await dialog.getByRole("button", {name: "已核对，保持停止"}).click();
 await state("stopped"); await dialog.waitFor({state: "hidden"});
 await click("恢复 AI 会话"); await state("active");
 check("real UIA Invoke remains unknown, blocks further actions, and explicit review never replays the operation");

 phase = "ai-cancel";
 const cancel = await api("POST", "/api/conversations", {title: "Controlled native approval cancellation", workspace_id: project.id});
 await api("PUT", `/api/conversations/${cancel.id}/workspace`, {workspace_id: project.id});
 await sendChat(cancel, "SOFTWARE_NATIVE_CANCEL: prepare a controlled Invoke but cancel before execution.");
 for (const name of ["software_sessions", "software_observe"]) await approve(cancel, name);
 const pending = await pendingFor(cancel); assert.equal(pending.tool_name, "software_control");
 const controls = (await snapshot()).history.filter(item => item.action === "control").length;
 await api("POST", "/api/chat/stop", {conversation_id: cancel.id});
 const late = await fetch(`${base}/api/approvals/${pending.approval_id}/approve`, {method: "POST", signal: AbortSignal.timeout(15000), headers: {"Content-Type": "application/json", "X-Yilian-Control-Session": token}, body: JSON.stringify({conversation_id: cancel.id})});
 assert.equal(late.status, 409, await late.text());
 assert.equal((await snapshot()).history.filter(item => item.action === "control").length, controls);
 approvalEvidence.push({scenario: phase, tool: "software_control", approval_id: pending.approval_id, status: "cancelled", late_approval_status: 409});
 assert.deepEqual(modelErrors, []);
 check("stopping the pending native control cancels its approval; late approval conflicts and no control dispatch receipt is created");
 await refresh(); await click("登记与许可");
 dialog = main.getByRole("dialog", {name: "软件登记与 AI 许可"});
 await dialog.getByRole("checkbox").nth(0).uncheck();
 assert.equal(await dialog.getByRole("checkbox").nth(1).isChecked(), false);
 await dialog.getByRole("button", {name: "保存登记"}).click(); await dialog.waitFor({state: "hidden"});
 registration = (await snapshot()).registrations.find(item => item.id === registration.id);
 assert.equal(registration.allow_observe, false); assert.equal(registration.allow_actions, false);
 await state("stopped");
 check("actual UI edits the canonical registered EXE and revokes observation/actions without losing the safe release path");
 await releaseViaUi();
}

try {
 await writeFile(join(evidence, "source-start.json"), JSON.stringify(await sourceManifest(), null, 2));
 await writeFile(join(evidence, "binaries.json"), JSON.stringify(await Promise.all([application, targetBinary].map(async path => ({path, bytes: (await stat(path)).size, sha256: createHash("sha256").update(await readFile(path)).digest("hex")}))), null, 2));
 await writeFile(join(evidence, "ports.json"), JSON.stringify({backendPort, frontendPort, debugPort}));
 launch(process.execPath, [join(root, "frontend/node_modules/vite/bin/vite.js"), "--host", "127.0.0.1", "--port", String(frontendPort), "--strictPort"], {VITE_API_BASE: base}, "vite");
 await until(async () => (await fetch(frontend, {signal: AbortSignal.timeout(2000)})).ok, Boolean, "isolated Vite ready");
 if (!targetAfterApp) await startTarget();
 app = launch(application, [], {OPENAI_API_KEY: "", YILIAN_NATIVE_FIXTURE_ROOT: isolated, YILIAN_NATIVE_FIXTURE_KEY: "local-controlled-placeholder", YILIAN_DATA_DIR: join(isolated, "data"), YILIAN_WORKSPACE: workspace,
  YILIAN_HOST: `127.0.0.1:${backendPort}`, YILIAN_ALLOWED_ORIGINS: frontend, YILIAN_NATIVE_BROWSER_FRONTEND_URL: frontend,
  YILIAN_SOFTWARE_FIXTURE_TARGET: "1", WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${debugPort}` + (startupDiagnostic ? ` --enable-logging --v=1 --log-file="${join(evidence,"webview2.log")}"` : "")}, "tauri");
 await until(readDebugTargets, list => Array.isArray(list) && list.some(item => item.type === "page" && item.url.startsWith(frontend)), "owned native app WebView2", startupDiagnostic ? 45000 : 90000);
 browser = await chromium.connectOverCDP(`http://127.0.0.1:${debugPort}`, {timeout: 45000});
 main = await until(() => browser.contexts().flatMap(item => item.pages()).find(page => page.url().startsWith(frontend)), Boolean, "owned application page");
 await main.bringToFront();
 if (startupDiagnostic) {
  await writeFile(join(evidence,"renderer-state.json"), JSON.stringify(await main.evaluate(()=>({url:location.href,ready:document.readyState,title:document.title,width:innerWidth,height:innerHeight,bodyLength:document.body?.textContent?.length??0,ipc:typeof window.__TAURI_INTERNALS__?.invoke})),null,2));
  check("startup diagnostic reached the actual owned WebView2 renderer"); throw diagnosticComplete;
 }
 main.on("pageerror", error => diagnostics.push({type: "pageerror", phase, message: error.message}));
 main.on("console", message => {if (message.type() === "error") diagnostics.push({type: "console", phase, text: message.text(), url: message.location().url});});
 main.on("requestfailed", request => requests.push({phase, url: request.url(), error: request.failure()?.errorText}));
 await main.evaluate(() => sessionStorage.setItem("yilian.setup.deferred", "true"));
 await main.goto(frontend + "/software", {waitUntil: "domcontentloaded"});
 token = await until(() => main.evaluate(() => window.__TAURI_INTERNALS__?.invoke("get_control_session_token")), value => typeof value === "string" && value.length >= 32, "trusted control session");
 if (targetAfterApp) await startTarget();
 await until(() => snapshot(), value => value.available === true, "actual native software host installed");
 const denied = await fetch(base + "/api/software"); assert.equal(denied.status, 401);
 await main.getByRole("heading", {name: "软件挂载", exact: true}).waitFor();
 check("actual isolated Tauri/WebView2, native host and control-session boundary");

 phase = "companion-display";
 const petPage = () => browser.contexts().flatMap(context => context.pages()).find(page => page.url().includes("/floating/pet"));
 assert.equal(petPage(), undefined, "A desktop pet with no saved open state stays closed");
 const avatarUpload = await fetch(`${base}/api/resources/ingest?name=fixture-avatar.png`, {
   method: "POST", headers: {"Content-Type":"image/png", "X-Yilian-Control-Session":token},
   body: await readFile(join(root, "frontend/public/favicon.png")),
 });
 assert.equal(avatarUpload.status, 201, await avatarUpload.clone().text());
 const avatar = await avatarUpload.json();
 await api("PUT", "/api/agents/default/profile", {persona_enabled:true, persona:{display_name:"受控伙伴", avatar_resource_id:avatar.id}});
 await main.evaluate(() => window.__TAURI_INTERNALS__.invoke("open_floating_surface", {kind:"pet",expanded:false}));
 const pet = await until(petPage, Boolean, "owned desktop pet page");
 await pet.getByRole("main", {name:"受控伙伴桌宠"}).waitFor();
 const avatarRender = await until(() => pet.locator(".desktop-pet-character img").evaluate(image => ({loaded:image.complete && image.naturalWidth > 0,source:image.getAttribute("src")})), item => item?.loaded && item.source?.startsWith("data:image/png;base64,"), "actual pet renders saved profile avatar");
 assert.ok(avatarRender.loaded);
 await pet.screenshot({path: join(evidence, "desktop-pet-profile.png")});
 assert.equal(await pet.evaluate(async () => {
   try { await window.__TAURI_INTERNALS__.invoke("get_control_session_token"); return true; } catch { return false; }
 }), false, "The pet cannot read a control-session credential");
 await api("PUT", "/api/agents/default/profile", {persona_enabled:true, persona:{display_name:"更新后的伙伴"}});
 await pet.getByRole("main", {name:"更新后的伙伴桌宠"}).waitFor({timeout: 6000});
 const builtInPet = pet.locator('.desktop-pet-character img[src^="/desktop-pet/xilian/"]');
 await builtInPet.waitFor({timeout: 6000});
 await builtInPet.evaluate(image => image.decode());
 await api("DELETE", "/api/agents/default/profile");
 await pet.getByRole("main", {name:"小涟桌宠"}).waitFor({timeout: 6000});
 await pet.getByRole("button", {name:"关闭桌宠"}).click();
 await until(petPage, value => !value, "pet closes without closing the workspace");
 const persistedPet = JSON.parse(await readFile(join(isolated, "data", "desktop-pet-position.json"), "utf8"));
 assert.equal(persistedPet.open, false);
 check("desktop pet reuses the real default profile name/avatar and closes independently without exposing a control-session token");

 phase = "workspace";
 project = await api("POST", "/api/workspaces", {name: "Owned software fixture project", description: "Isolated acceptance", root_path: workspace});
 await main.reload({waitUntil: "domcontentloaded"});
 await main.getByRole("heading", {name: "软件挂载", exact: true}).waitFor();

 phase = "registration";
 await click("登记软件"); let dialog = main.getByRole("dialog", {name: "登记软件", exact: true});
 await dialog.getByLabel("软件名称", {exact: true}).fill("CONTROLLED_NATIVE_SOFTWARE");
 await dialog.getByLabel("适配类型").selectOption("generic_fixture");
 await until(() => dialog.getByLabel("所属项目").locator("option").count(), count => count >= 2, "real project option");
 await dialog.getByLabel("所属项目").selectOption(project.id);
 await dialog.getByLabel("程序 EXE 绝对路径").fill("C:\\missing-controlled-fixture.exe");
 assert.equal(await dialog.getByRole("checkbox").nth(0).isChecked(), false);
 assert.equal(await dialog.getByRole("checkbox").nth(1).isChecked(), false);
 await dialog.getByRole("button", {name: "保存登记"}).click(); await dialog.getByRole("alert").waitFor();
 assert.equal(await dialog.getByLabel("软件名称", {exact: true}).inputValue(), "CONTROLLED_NATIVE_SOFTWARE");
 await dialog.getByLabel("程序 EXE 绝对路径").fill(targetBinary);
 await dialog.getByRole("button", {name: "保存登记"}).click(); await dialog.waitFor({state: "hidden"});
 registration = await until(async () => (await snapshot()).registrations.find(item => item.name === "CONTROLLED_NATIVE_SOFTWARE"), Boolean, "registration persisted");
 assert.equal(registration.allow_observe, false); assert.equal(registration.allow_actions, false);
 assert.equal(registration.workspace_id, project.id);
 await click("软件列表");
 await main.getByLabel("项目筛选").selectOption(project.id);
 assert.equal(await main.locator(".software-list-item").count(), 1);
 await main.getByLabel("项目筛选").selectOption("");
 await click("软件列表");
 check("real registration validation preserves draft; AI permissions default off");

 phase = "attachment";
 let attached = await attachViaUi(targetA);
 assert.equal(attached.window.identity.pid, target.pid);
 assert.equal(typeof attached.window.identity.process_started_at, "string");
 assert.ok(BigInt(attached.window.identity.process_started_at) > BigInt(Number.MAX_SAFE_INTEGER));
 assert.equal(attached.launched_by_host, false);
 const receipt = (await snapshot()).history.find(item => item.action === "attach" && item.session_id === attached.id);
 const repeated = await api("POST", `/api/software/registrations/${registration.id}/attach`, {operation_id: receipt.operation_id, identity: attached.window.identity});
 assert.equal(repeated.id, attached.id);
 assert.equal((await snapshot()).sessions.filter(item => item.window.identity.hwnd === targetA && item.state === "active").length, 1);
 const attachmentSnapshot = await snapshot();
 const mountedWindow = attachmentSnapshot.sessions.find(item => item.id === attached.id).window;
 const capability = attachmentSnapshot.capability_evidence.find(item => item.session_id === attached.id);
 assert.equal(capability?.generation, attached.generation);
 assert.equal(capability?.entries.length, 6);
 assert.equal(capability.entries.find(item => item.dimension === "discover_launch")?.evidence_id, receipt.operation_id);
 assert.equal(capability.entries.find(item => item.dimension === "display")?.status, mountedWindow.visible && !mountedWindow.minimized ? "verified" : "unavailable");
 assert.equal(capability.entries.find(item => item.dimension === "perceive")?.status, "authorization_required");
 await click("会话详情"); dialog = main.getByRole("dialog", {name: "软件会话详情"});
 await dialog.getByRole("region", {name: "能力验证"}).waitFor();
 await dialog.getByText("后台感知与操作", {exact:true}).waitFor();
 await shot("software-capability-evidence.png");
 await dialog.getByRole("button", {name: "关闭", exact: true}).click();
 check("multi-instance explicit selection, exact FILETIME string roundtrip and duplicate attach idempotence", {sessionId, pid: target.pid, hwnd: targetA});

 phase = "presentation";
 await until(() => main.getByRole("button", {name: "嵌回忆涟千言", exact: true}).isEnabled(), Boolean, "compatible native host parent installed");
 await click("嵌回忆涟千言"); attached = await mode("embedded");
 assert.equal(attached.window.identity.hwnd, targetA); assert.equal(attached.window.identity.pid, target.pid);
 await shot("actual-native-embedded.png");
 const native = await ownedMainWindow("owned-native-window-embedded.png");
 const hostWindow = native.windows.find(item => item.cls === "Tauri Window" && item.parent === null);
 const embeddedWindow = native.windows.find(item => item.hwnd === targetA && item.pid === target.pid);
 assert.ok(hostWindow && embeddedWindow, "The real target must appear in the owned Tauri native window hierarchy");
 assert.equal(embeddedWindow.nativeParent, hostWindow.hwnd, "Verify physical Win32 parent, independently of API mode");
 assert.equal(embeddedWindow.visible, true);
 const embeddedProof = (await snapshot()).capability_evidence.find(item => item.session_id === attached.id);
 assert.equal(embeddedProof?.entries.find(item => item.dimension === "display")?.status, "verified");
 await writeFile(join(evidence, "native-embedded-state.json"), JSON.stringify(native, null, 2));
 attached = await api("POST", `/api/software/sessions/${sessionId}/present`, {
  generation: (await current()).generation, operation_id: randomUUID(), mode: "independent",
 });
 assert.equal(attached.window.mode, "independent");
 await refresh();
 assert.equal(attached.window.identity.hwnd, targetA);
 check("embed and restore use the same real HWND and process");
 await click("嵌回忆涟千言"); await mode("embedded");
 attached = await showIndependent(); await mode("independent");
 assert.equal(attached.window.identity.hwnd, targetA);
 check("independent display preserves the same visible window even when foreground activation is unavailable");
 await click("嵌回忆涟千言"); await mode("embedded");
 await click("操作记录"); dialog = main.getByRole("dialog", {name: "软件操作记录"}); await dialog.waitFor();
 await mode("independent"); await shot("history-restores-external-window.png");
 await dialog.getByRole("button", {name: "关闭", exact: true}).click();
 check("opening actual history dialog restores native window before trusted modal");
 await click("嵌回忆涟千言"); await mode("embedded");
 await main.keyboard.press("Control+k"); await main.getByRole("combobox", {name: "搜索页面、执行快捷动作或切换主题……", exact: true}).waitFor();
 await mode("independent"); await main.keyboard.press("Escape");
 check("global command palette triggers independent native restoration");
 await click("嵌回忆涟千言"); await mode("embedded");
 await main.getByRole("link", {name: "更多", exact: true}).click();
 await mode("independent");
 await main.getByRole("link", {name: "软件挂载", exact: true}).click();
 await main.getByRole("heading", {name: "软件挂载", exact: true}).waitFor();
 await until(() => main.getByRole("button", {name: "停止 AI 操作", exact: true}).isEnabled(), Boolean, "returned active session");
 check("real route transition restores external window and session survives reopening");

 phase = "session-controls";
 await click("停止 AI 操作"); await state("stopped");
 await until(() => main.getByRole("button", {name: "恢复 AI 会话", exact: true}).isEnabled(), Boolean, "resume ready");
 await click("恢复 AI 会话"); await state("active");
 await showIndependent(); await mode("independent");
 assert.equal(target.exitCode, null);
 check("stop/resume and explicit independent display preserve the attached user-owned process");

 phase = "layout";
 for (const width of [1366, 960, 390]) await cssGeometry("software-workbench", width);
 await main.setViewportSize({width: 1366, height: 844});
 await click("登记与许可"); dialog = main.getByRole("dialog", {name: "软件登记与 AI 许可"});
 assert.equal(await dialog.getByRole("button", {name: "保存登记"}).isEnabled(), true, "Canonical Windows executable remains editable after a real save");
 for (const width of [1366, 960, 390]) await cssGeometry("registration-dialog", width);
 await main.setViewportSize({width: 1366, height: 844});
 await dialog.getByRole("button", {name: "取消", exact: true}).click();
 check("actual WebView2 workbench and modal 1366/960/390 CSS layouts have reachable controls without horizontal overflow");

 phase = "fallback";
 await releaseViaUi(); assert.equal(target.exitCode, null);
 await aiCoverage();
 phase = "fallback";
 const incompatible = await attachViaUi(targetB);
 assert.equal(incompatible.window.embedding_supported, false);
 assert.equal(await main.getByRole("button", {name: "嵌回忆涟千言", exact: true}).isDisabled(), true);
 await shot("mixed-dpi-independent-fallback.png");
 await releaseViaUi();
 check("mixed-DPI target remains independent and unavailable embedding is truthful; release never kills target");

 phase = "confirmation-revision";
 const old = registration;
 const updated = await api("PUT", `/api/software/registrations/${old.id}`, {revision: old.revision, config: {name: "CONTROLLED_NATIVE_SOFTWARE_REVISED", adapter: old.adapter, executable: old.executable, workspace_id: null, allow_observe: false, allow_actions: false}});
 const staleResponse = await fetch(`${base}/api/software/registrations/${old.id}/launch`, {method: "POST", headers: {"Content-Type": "application/json", "X-Yilian-Control-Session": token}, body: JSON.stringify({operation_id: randomUUID(), confirmed: true, revision: old.revision, expected_executable: old.executable})});
 assert.equal(staleResponse.status, 409, await staleResponse.text());
 registration = updated;
 assert.equal(target.exitCode, null);
 check("stale exact launch confirmation is rejected without launching another process");
 const final = await snapshot();
 assert.ok(final.sessions.every(item => item.state === "released"));
 assert.equal(target.exitCode, null);
 const expectedDiagnostics = item => item.type === "console" && (
  (item.url?.startsWith("https://fonts.googleapis.com/") && /ERR_NETWORK_ACCESS_DENIED|ERR_CONNECTION|Failed to load/.test(item.text))
  || (item.phase === "registration" && item.url === base + "/api/software/registrations" && /400/.test(item.text))
 );
 const unexpected = diagnostics.filter(item => !expectedDiagnostics(item));
 assert.deepEqual(unexpected, [], "Unexpected trusted UI diagnostic");
 check("all owned sessions released; attached target still alive; no unexpected UI errors");
 await writeFile(join(evidence, "final-snapshot.json"), JSON.stringify(final, null, 2));
} catch (error) {
 if (error !== diagnosticComplete) {failure = error; console.error(`[software-native] FAIL ${error.stack || error}`);}
 if (token) await writeFile(join(evidence, "failure-snapshot.json"), JSON.stringify(await snapshot().catch(error => ({error: String(error)})), null, 2));
 if (main && !main.isClosed()) await shot("failure.png").catch(() => {});
} finally {
 phase = "cleanup";
 if(startupDiagnostic) {try {await writeFile(join(evidence,"owned-window-state.json"),JSON.stringify(await ownedMainWindow(),null,2));}catch(error){await writeFile(join(evidence,"owned-window-probe-error.txt"),String(error.stack||error));}}
 if (token) {
  try {for (const session of (await snapshot()).sessions.filter(item => item.state !== "released")) {
   await api("POST", `/api/software/sessions/${session.id}/release`, {generation: session.generation, operation_id: randomUUID()});
  }} catch (error) {logs.push(`cleanup release: ${error}\n`);}
 }
 if (browser) await browser.close().catch(error => logs.push(`CDP disconnect: ${error}\n`));
 const cleanup = await Promise.allSettled(children.slice().reverse().map(closeOwned));
 if (model) {model.closeAllConnections(); await new Promise(resolve => model.close(resolve));}
 const portChecks = [];
 for (const port of [backendPort, frontendPort, debugPort]) {await until(() => portOpen(port), open => !open, `owned port ${port} closed`, 10000).catch(error => {if (!failure) failure = error;});portChecks.push({port, open: await portOpen(port)});}
 if (cleanup.some(item => item.status === "rejected") && !failure) failure = new Error("Owned service cleanup failed");
 const result = {status: failure ? "failed" : startupDiagnostic ? "diagnostic-complete" : "passed", failure: failure ? String(failure.stack || failure) : null, isolated, evidence, started, ended: Date.now(), checks, geometry, diagnostics, requests, debugProbes, modelCalls, modelErrors, toolEvidence, approvalEvidence,
  acceptanceStage,
  startupDiagnostic,
  targetTiming: targetAfterApp ? "after-application-ready" : "before-application-start",
  boundary: "Actual isolated Tauri/WebView2 with real Rust HTTP/SQLite, Gateway approvals, deterministic loopback model and self-created Win32 windows. No real WeChat, accounts, chats or paid model.",
  cleanup: cleanup.map(item => ({status: item.status, reason: item.status === "rejected" ? String(item.reason) : null})), portChecks, ports: {backendPort, frontendPort, debugPort}};
 await writeFile(join(evidence, "result.json"), JSON.stringify(result, null, 2));
 await writeFile(join(evidence, "source-end.json"), JSON.stringify(await sourceManifest(), null, 2));
 await writeFile(join(evidence, "services.log"), logs.join(""));
 console.log(`[software-native] ${result.status}; ${checks.length} checks; ${evidence}`);
}
if (failure) process.exitCode = 1;
