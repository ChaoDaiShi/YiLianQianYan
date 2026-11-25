import { spawn } from "node:child_process";

// WM_CLOSE is the native title-bar CloseRequested event, never BrowserAction.Close.
// PID + executable + fixed trusted title must all match this isolated fixture.
export async function ownedBrowserWindow({ appPid, expectedBinary, close = false, recordChild }) {
  if (!Number.isSafeInteger(appPid) || appPid <= 0 || !expectedBinary) throw new Error("Owned fixture identity required");
  const script = `
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new()
$fixtureProcessId = [int]$env:YILIAN_FIXTURE_WINDOW_PID
$fixtureProcess = Get-Process -Id $fixtureProcessId -ErrorAction Stop
$expectedBinary = [System.IO.Path]::GetFullPath($env:YILIAN_FIXTURE_WINDOW_BINARY)
if (-not [string]::Equals($fixtureProcess.Path, $expectedBinary, [System.StringComparison]::OrdinalIgnoreCase)) { throw 'Fixture process executable mismatch; no message sent' }
Add-Type -TypeDefinition @'
using System;
using System.Text;
using System.Collections.Generic;
using System.Runtime.InteropServices;
public static class OwnedFixtureWindow {
  public delegate bool EnumCallback(IntPtr hwnd, IntPtr value);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumCallback callback, IntPtr value);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hwnd, out uint pid);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr hwnd, StringBuilder text, int length);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hwnd);
  [DllImport("user32.dll", SetLastError=true)] public static extern bool PostMessage(IntPtr hwnd, uint message, IntPtr wparam, IntPtr lparam);
  public static IntPtr[] Find(uint owner, string title) {
    var found = new List<IntPtr>();
    EnumWindows((hwnd,value) => { uint actual; GetWindowThreadProcessId(hwnd,out actual); if(actual != owner) return true; var text=new StringBuilder(1024);GetWindowText(hwnd,text,text.Capacity);if(text.ToString()==title)found.Add(hwnd);return true; },IntPtr.Zero);
    return found.ToArray();
  }
}
'@
$fixtureWindows = [OwnedFixtureWindow]::Find([uint32]$fixtureProcessId, '忆涟千言 · 浏览器')
$visible = @($fixtureWindows | Where-Object { [OwnedFixtureWindow]::IsWindowVisible($_) })
$request = $env:YILIAN_FIXTURE_WINDOW_CLOSE -eq '1'
if ($request) {
  if ($visible.Count -ne 1) { throw 'Expected one visible owned shell; no message sent' }
  if (-not [OwnedFixtureWindow]::PostMessage($visible[0], 0x0010, [IntPtr]::Zero, [IntPtr]::Zero)) { throw 'Owned WM_CLOSE failed' }
}
@{ pid=$fixtureProcessId; matchedCount=$fixtureWindows.Count; visibleCount=$visible.Count; closeRequested=$request } | ConvertTo-Json -Compress
`;
  const child = spawn("powershell.exe", ["-NoProfile", "-NonInteractive", "-WindowStyle", "Hidden", "-EncodedCommand", Buffer.from(script, "utf16le").toString("base64")], {
    windowsHide: true, stdio: ["ignore", "pipe", "pipe"],
    env: { ...process.env, YILIAN_FIXTURE_WINDOW_PID: String(appPid), YILIAN_FIXTURE_WINDOW_BINARY: expectedBinary, YILIAN_FIXTURE_WINDOW_CLOSE: close ? "1" : "0" },
  });
  recordChild?.(child);
  let stdout = "", stderr = "";
  child.stdout.on("data", value => stdout += value);
  child.stderr.on("data", value => stderr += value);
  await new Promise((resolve, reject) => {
    const timer = setTimeout(() => { child.kill(); reject(new Error("Owned window helper timed out")); }, 12000);
    child.once("error", error => { clearTimeout(timer); reject(error); });
    child.once("exit", code => { clearTimeout(timer); code === 0 ? resolve() : reject(new Error(`Owned window helper failed (${code}): ${stderr.slice(0,2000)}`)); });
  });
  return JSON.parse(stdout.trim().replace(/^\uFEFF/, ""));
}
