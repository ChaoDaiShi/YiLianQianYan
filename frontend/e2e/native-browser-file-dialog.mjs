import { spawn } from "node:child_process";
import { mkdtemp, writeFile, unlink, rmdir } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, win32 } from "node:path";

// Test-only standard Win32 CommonDialog driver. No global keys or coordinates.
// The caller owns appPid and must register this helper with its child cleanup.
export async function ownedNativeFileDialog({ appPid, action, path, suggestedName, recordChild }) {
  if (process.platform !== "win32") throw new Error("Native file-dialog fixtures require Windows");
  if (!Number.isSafeInteger(appPid) || appPid <= 0) throw new Error("Owned app PID required");
  if (action !== "cancel" && action !== "choose") throw new Error("Expected cancel or choose action");
  if (action === "choose" && (typeof path !== "string" || !win32.isAbsolute(path) || /[\u0000-\u001f]/.test(path))) throw new Error("Choose requires an absolute controlled fixture path");
  if (suggestedName !== undefined && (typeof suggestedName !== "string" || !suggestedName || /[\u0000-\u001f]/.test(suggestedName))) throw new Error("suggestedName must be the known native suggested filename");
  const directory = await mkdtemp(join(tmpdir(), "yilian-owned-file-dialog-"));
  const argsFile = join(directory, "arguments.json");
  const scriptFile = join(directory, "dialog.ps1");
  await writeFile(argsFile, JSON.stringify({ appPid, action, path: action === "choose" ? path : null, suggestedName: suggestedName ?? null }), "utf8");
  const script = String.raw`
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false)
trap { [Console]::Error.WriteLine($_.Exception.Message); exit 1 }
$inputArgs = [System.IO.File]::ReadAllText($env:YILIAN_OWNED_DIALOG_ARGS, [System.Text.Encoding]::UTF8) | ConvertFrom-Json
$fixtureProcessId = [int]$inputArgs.appPid
$fixtureProcess = Get-Process -Id $fixtureProcessId -ErrorAction Stop
$fixtureStart = $fixtureProcess.StartTime.ToUniversalTime().Ticks
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type -AssemblyName System.Windows.Forms
Add-Type -TypeDefinition @'
using System;
using System.Text;
using System.Collections.Generic;
using System.Runtime.InteropServices;
public static class OwnedFileDialogWindows {
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left,Top,Right,Bottom; }
  [StructLayout(LayoutKind.Sequential)] public struct COMBOBOXINFO { public int Size; public RECT Item,Button; public uint State; public IntPtr Combo,Edit,List; }
  public delegate bool EnumCallback(IntPtr hwnd, IntPtr value);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumCallback callback, IntPtr value);
  [DllImport("user32.dll")] public static extern bool EnumChildWindows(IntPtr parent, EnumCallback callback, IntPtr value);
  [DllImport("user32.dll")] public static extern IntPtr GetParent(IntPtr hwnd);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hwnd);
  [DllImport("user32.dll")] public static extern bool IsWindowEnabled(IntPtr hwnd);
  [DllImport("user32.dll")] public static extern bool IsChild(IntPtr parent, IntPtr child);
  [DllImport("user32.dll")] public static extern IntPtr GetDlgItem(IntPtr dialog, int id);
  [DllImport("user32.dll")] public static extern int GetDlgCtrlID(IntPtr hwnd);
  [DllImport("user32.dll")] public static extern bool GetComboBoxInfo(IntPtr hwnd, ref COMBOBOXINFO info);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hwnd, out RECT rect);
  [DllImport("user32.dll", EntryPoint="SendMessageTimeoutW", CharSet=CharSet.Unicode, SetLastError=true)] private static extern IntPtr SendNative(IntPtr hwnd,uint message,UIntPtr wparam,IntPtr lparam,uint flags,uint timeout,out UIntPtr result);
  [DllImport("user32.dll", EntryPoint="SendMessageTimeoutW", CharSet=CharSet.Unicode, SetLastError=true)] private static extern IntPtr SendText(IntPtr hwnd,uint message,UIntPtr wparam,string text,uint flags,uint timeout,out UIntPtr result);
  [DllImport("user32.dll", EntryPoint="SendMessageTimeoutW", CharSet=CharSet.Unicode, SetLastError=true)] private static extern IntPtr ReadText(IntPtr hwnd,uint message,UIntPtr wparam,StringBuilder text,uint flags,uint timeout,out UIntPtr result);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetClassName(IntPtr hwnd, StringBuilder text, int length);
  [DllImport("user32.dll")] public static extern IntPtr GetWindow(IntPtr hwnd, uint command);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hwnd, out uint pid);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr hwnd, StringBuilder text, int length);
  public static uint Pid(IntPtr hwnd) { uint pid; GetWindowThreadProcessId(hwnd,out pid); return pid; }
  public static string Title(IntPtr hwnd) { var text=new StringBuilder(512);GetWindowText(hwnd,text,text.Capacity);return text.ToString(); }
  public static string Class(IntPtr hwnd) { var text=new StringBuilder(256);GetClassName(hwnd,text,text.Capacity);return text.ToString(); }
  public static RECT Bounds(IntPtr hwnd) { RECT rect;if(!GetWindowRect(hwnd,out rect))throw new InvalidOperationException("Owned dialog bounds unavailable");return rect; }
  public static IntPtr[] ChildEdits(IntPtr dialog) {
    var result=new List<IntPtr>();uint expected=Pid(dialog);
    EnumChildWindows(dialog,(child,value)=> {if(Class(child)=="Edit"&&Pid(child)==expected&&IsChild(dialog,child))result.Add(child);return true;},IntPtr.Zero);
    return result.ToArray();
  }
  public static bool MatchesKnownText(IntPtr edit,string known) {
    UIntPtr length;
    if(SendNative(edit,0x000E,UIntPtr.Zero,IntPtr.Zero,0x23,2000,out length)==IntPtr.Zero)throw new InvalidOperationException("Owned filename length query failed or timed out");
    if(length.ToUInt64()!=(ulong)known.Length)return false;
    var text=new StringBuilder(known.Length+1);UIntPtr read;
    if(ReadText(edit,0x000D,new UIntPtr((uint)text.Capacity),text,0x23,2000,out read)==IntPtr.Zero)throw new InvalidOperationException("Owned filename identity query failed or timed out");
    return read.ToUInt64()==(ulong)known.Length&&String.Equals(text.ToString(),known,StringComparison.Ordinal);
  }
  public static IntPtr FilenameEdit(IntPtr anchor) {
    string name=Class(anchor);
    if(name=="Edit")return anchor;
    if(name=="ComboBoxEx32") { UIntPtr edit;if(SendNative(anchor,0x407,UIntPtr.Zero,IntPtr.Zero,0x23,2000,out edit)==IntPtr.Zero)throw new InvalidOperationException("CBEM_GETEDITCONTROL failed or timed out");return new IntPtr(unchecked((long)edit.ToUInt64())); }
    if(name=="ComboBox") { var info=new COMBOBOXINFO();info.Size=Marshal.SizeOf(typeof(COMBOBOXINFO));if(!GetComboBoxInfo(anchor,ref info))throw new InvalidOperationException("GetComboBoxInfo failed");return info.Edit; }
    throw new InvalidOperationException("Owned filename anchor has an unsupported standard class");
  }
  public static void SetKnownFilename(IntPtr edit,string path) {
    UIntPtr written;
    if(SendText(edit,0x000C,UIntPtr.Zero,path,0x23,2000,out written)==IntPtr.Zero||written==UIntPtr.Zero)throw new InvalidOperationException("Owned filename WM_SETTEXT failed or timed out");
    var text=new StringBuilder(path.Length+1);UIntPtr length;
    if(ReadText(edit,0x000D,new UIntPtr((uint)text.Capacity),text,0x23,2000,out length)==IntPtr.Zero)throw new InvalidOperationException("Owned filename WM_GETTEXT failed or timed out");
    if(!String.Equals(text.ToString(),path,StringComparison.Ordinal))throw new InvalidOperationException("Owned filename readback did not match the known Unicode fixture path");
  }
  public static bool ClickButton(IntPtr button) { UIntPtr result;return SendNative(button,0x00F5,UIntPtr.Zero,IntPtr.Zero,0x03,2000,out result)!=IntPtr.Zero; }
  public static IntPtr[] VisibleDialogs(string[] titles) {
    var allowed=new HashSet<string>(titles,StringComparer.Ordinal);
    var result=new List<IntPtr>();
    EnumWindows((hwnd,value)=> {if(IsWindowVisible(hwnd)&&allowed.Contains(Title(hwnd)))result.Add(hwnd);return true;},IntPtr.Zero);
    return result.ToArray();
  }
}
'@
$deadline = [DateTime]::UtcNow.AddSeconds(16)
$ownedProcesses = New-Object 'System.Collections.Generic.HashSet[uint32]'
[void]$ownedProcesses.Add([uint32]$fixtureProcessId)
# A common dialog can be hosted by a controlled WebView2 descendant.
# Creation time prevents a recycled parent PID from admitting an older process.
$processSnapshot = @(Get-CimInstance Win32_Process | Select-Object ProcessId,ParentProcessId,CreationDate)
do {
  $added = $false
  foreach ($process in $processSnapshot) {
    if ($ownedProcesses.Contains([uint32]$process.ParentProcessId) -and $process.CreationDate -and $process.CreationDate.ToUniversalTime().Ticks -ge $fixtureStart) {
      if ($ownedProcesses.Add([uint32]$process.ProcessId)) { $added = $true }
    }
  }
} while ($added)
function Get-Ownership([IntPtr]$handle) {
  $actual = [OwnedFileDialogWindows]::Pid($handle)
  if ($ownedProcesses.Contains($actual)) { return "process-tree" }
  $owner = [OwnedFileDialogWindows]::GetWindow($handle,4)
  $seen = New-Object 'System.Collections.Generic.HashSet[long]'
  while ($owner -ne [IntPtr]::Zero -and $seen.Add($owner.ToInt64())) {
    if ($ownedProcesses.Contains([OwnedFileDialogWindows]::Pid($owner))) { return "owner-chain" }
    $owner = [OwnedFileDialogWindows]::GetWindow($owner,4)
  }
  return $null
}
function Normalize-DialogName([string]$name) {
  return (($name -replace '&','' -replace '[（(]\s*[A-Za-z]\s*[)）]','').Trim())
}
function Describe-StandardControls([IntPtr]$window) {
  return @(foreach ($id in @(1,2,1148)) {
    $control = [OwnedFileDialogWindows]::GetDlgItem($window,$id)
    $metadata = @{id=$id;hwnd=$control.ToInt64();exists=($control -ne [IntPtr]::Zero)}
    if ($control -ne [IntPtr]::Zero) {
      $metadata.class = [OwnedFileDialogWindows]::Class($control)
      $metadata.actualId = [OwnedFileDialogWindows]::GetDlgCtrlID($control)
      $metadata.pid = [OwnedFileDialogWindows]::Pid($control)
      $metadata.childOfDialog = [OwnedFileDialogWindows]::IsChild($window,$control)
      $metadata.visible = [OwnedFileDialogWindows]::IsWindowVisible($control)
      $metadata.enabled = [OwnedFileDialogWindows]::IsWindowEnabled($control)
      if ($id -eq 1 -or $id -eq 2) { $metadata.buttonLabel = [OwnedFileDialogWindows]::Title($control) }
      try { $element = [System.Windows.Automation.AutomationElement]::FromHandle($control); $metadata.uiaType = $element.Current.ControlType.ProgrammaticName; $metadata.uiaOffscreen=$element.Current.IsOffscreen; $metadata.uiaEnabled=$element.Current.IsEnabled }
      catch { $metadata.uiaError=$_.Exception.Message }
    }
    $metadata
  })
}
function Standard-ControlHandle([int]$id) {
  $control = [OwnedFileDialogWindows]::GetDlgItem($handle,$id)
  if ($control -eq [IntPtr]::Zero) { return [IntPtr]::Zero }
  $class = [OwnedFileDialogWindows]::Class($control)
  $classes = if ($id -eq 1148) { @('Edit','ComboBox','ComboBoxEx32') } else { @('Button') }
  if ([OwnedFileDialogWindows]::GetDlgCtrlID($control) -ne $id -or -not [OwnedFileDialogWindows]::IsChild($handle,$control) -or [OwnedFileDialogWindows]::Pid($control) -ne $dialogProcessId -or $classes -notcontains $class) {
    throw ('Standard owned dialog control identity mismatch: '+(ConvertTo-Json -Compress -Depth 5 (Describe-StandardControls $handle)))
  }
  if ($id -ne 1148 -and $allowedNames -notcontains (Normalize-DialogName ([OwnedFileDialogWindows]::Title($control)))) { throw ('Standard owned button label mismatch: '+(ConvertTo-Json -Compress -Depth 5 (Describe-StandardControls $handle))) }
  if (-not [OwnedFileDialogWindows]::IsWindowVisible($control) -or -not [OwnedFileDialogWindows]::IsWindowEnabled($control)) { return [IntPtr]::Zero }
  return $control
}
function Describe-Dialog([IntPtr]$window) {
  $rect = [OwnedFileDialogWindows]::Bounds($window)
  $work = [System.Windows.Forms.Screen]::FromHandle($window).WorkingArea
  $edits = @([OwnedFileDialogWindows]::ChildEdits($window) | ForEach-Object { @{hwnd=$_.ToInt64();id=[OwnedFileDialogWindows]::GetDlgCtrlID($_);class=[OwnedFileDialogWindows]::Class($_);parentClass=[OwnedFileDialogWindows]::Class([OwnedFileDialogWindows]::GetParent($_));visible=[OwnedFileDialogWindows]::IsWindowVisible($_);enabled=[OwnedFileDialogWindows]::IsWindowEnabled($_)} })
  return @{ title=[OwnedFileDialogWindows]::Title($window); dialogPid=[OwnedFileDialogWindows]::Pid($window); windowRect=@{x=$rect.Left;y=$rect.Top;width=($rect.Right-$rect.Left);height=($rect.Bottom-$rect.Top)}; screenWorkArea=@{x=$work.X;y=$work.Y;width=$work.Width;height=$work.Height}; standardControls=@(Describe-StandardControls $window); nativeEdits=$edits }
}
function Assert-DialogOwnership {
  if (-not [OwnedFileDialogWindows]::IsWindowVisible($handle) -or -not (Get-Ownership $handle) -or [OwnedFileDialogWindows]::Title($handle) -cne $title) { throw 'Owned dialog changed before action; no UI action sent' }
  if ((Get-Process -Id $fixtureProcessId -ErrorAction Stop).StartTime.ToUniversalTime().Ticks -ne $fixtureStart) { throw 'Owned app process identity changed before action' }
}
$handle = [IntPtr]::Zero
$ownership = $null
while ([DateTime]::UtcNow -lt $deadline) {
  $currentProcess = Get-Process -Id $fixtureProcessId -ErrorAction Stop
  if ($currentProcess.StartTime.ToUniversalTime().Ticks -ne $fixtureStart) { throw 'Owned process identity changed; no UI action sent' }
  $matches = @()
  foreach ($candidate in [OwnedFileDialogWindows]::VisibleDialogs(@('保存下载文件','Open','打开'))) {
    $proof = Get-Ownership $candidate
    if ($proof) { $matches += @{handle=$candidate;proof=$proof} }
  }
  if ($matches.Count -gt 1) { throw 'Multiple owned file dialogs; no UI action sent' }
  if ($matches.Count -eq 1) { $handle=$matches[0].handle; $ownership=$matches[0].proof; break }
  Start-Sleep -Milliseconds 100
}
if ($handle -eq [IntPtr]::Zero) { throw 'No matching owned Open/打开/保存下载文件 dialog appeared before deadline' }
$title = [OwnedFileDialogWindows]::Title($handle)
$dialogProcessId = [OwnedFileDialogWindows]::Pid($handle)
$edit = [IntPtr]::Zero
$buttonId = if ($inputArgs.action -eq 'cancel') { 2 } else { 1 }
$allowedNames = if ($inputArgs.action -eq 'cancel') { @('Cancel','取消') } elseif ($title -eq '保存下载文件') { @('Save','保存') } else { @('Open','打开') }
$modernSave = $title -ceq '保存下载文件'
if ($inputArgs.action -eq 'choose' -and $modernSave -and -not $inputArgs.suggestedName) { throw 'Save choose requires the exact known suggestedName supplied to the native dialog' }
$button = [IntPtr]::Zero
$filename = [IntPtr]::Zero
# This driver deliberately uses the documented standard Win32 control protocol.
# UIA is diagnostic only because the observed provider exposes these HWNDs as Pane.
while ([DateTime]::UtcNow -lt $deadline) {
  Assert-DialogOwnership
  $button = Standard-ControlHandle $buttonId
  if ($inputArgs.action -eq 'choose') {
    if ($modernSave) {
      $matchingEdits = @([OwnedFileDialogWindows]::ChildEdits($handle) | Where-Object { [OwnedFileDialogWindows]::IsWindowVisible($_) -and [OwnedFileDialogWindows]::IsWindowEnabled($_) -and [OwnedFileDialogWindows]::MatchesKnownText($_,[string]$inputArgs.suggestedName) })
      if ($matchingEdits.Count -gt 1) { throw ('Known Save filename matches multiple owned Edit controls: '+(ConvertTo-Json -Compress -Depth 5 (Describe-Dialog $handle))) }
      $filename = if ($matchingEdits.Count -eq 1) { $matchingEdits[0] } else { [IntPtr]::Zero }
    } else { $filename = Standard-ControlHandle 1148 }
  }
  if ($button -ne [IntPtr]::Zero -and ($inputArgs.action -eq 'cancel' -or $filename -ne [IntPtr]::Zero)) { break }
  Start-Sleep -Milliseconds 100
}
if ($button -eq [IntPtr]::Zero -or ($inputArgs.action -eq 'choose' -and $filename -eq [IntPtr]::Zero)) { throw ('Owned standard dialog controls did not become usable before deadline: ' + (ConvertTo-Json -Compress -Depth 5 (Describe-Dialog $handle))) }
if ($inputArgs.action -eq 'choose') {
  Assert-DialogOwnership
  if ($modernSave) {
    if (-not [OwnedFileDialogWindows]::MatchesKnownText($filename,[string]$inputArgs.suggestedName)) { throw 'Owned Save filename no longer matches the known suggestedName' }
    $edit = $filename
  } else {
    if ((Standard-ControlHandle 1148) -ne $filename) { throw 'Owned filename anchor changed before setting path' }
    $edit = [OwnedFileDialogWindows]::FilenameEdit($filename)
  }
  if ($edit -eq [IntPtr]::Zero -or [OwnedFileDialogWindows]::Class($edit) -cne 'Edit' -or [OwnedFileDialogWindows]::Pid($edit) -ne $dialogProcessId -or -not [OwnedFileDialogWindows]::IsChild($handle,$edit) -or ($edit -ne $filename -and -not [OwnedFileDialogWindows]::IsChild($filename,$edit)) -or -not [OwnedFileDialogWindows]::IsWindowVisible($edit) -or -not [OwnedFileDialogWindows]::IsWindowEnabled($edit)) { throw 'Owned filename edit HWND identity or readiness mismatch' }
  Assert-DialogOwnership
  [OwnedFileDialogWindows]::SetKnownFilename($edit,[string]$inputArgs.path)
}
Assert-DialogOwnership
if ((Standard-ControlHandle $buttonId) -ne $button) { throw 'Owned action button changed before BM_CLICK' }
$buttonName = [OwnedFileDialogWindows]::Title($button)
$editControlId = if ($edit -ne [IntPtr]::Zero) { [OwnedFileDialogWindows]::GetDlgCtrlID($edit) } else { $null }
$messageAcknowledged = [OwnedFileDialogWindows]::ClickButton($button)
while ([OwnedFileDialogWindows]::IsWindowVisible($handle) -and [DateTime]::UtcNow -lt $deadline) { Start-Sleep -Milliseconds 100 }
if ([OwnedFileDialogWindows]::IsWindowVisible($handle)) { throw ('Owned file dialog did not close after BM_CLICK; message acknowledged='+$messageAcknowledged+'; no further UI action sent') }
@{ driver='win32-common-dialog'; appPid=$fixtureProcessId; dialogPid=$dialogProcessId; title=$title; ownership=$ownership; action=$inputArgs.action; buttonId=$buttonId; buttonName=$buttonName; filenameEditId=$editControlId; messageAcknowledged=$messageAcknowledged; closed=$true } | ConvertTo-Json -Compress
`;
  let child;
  let timer;
  try {
    // A file avoids Windows' command-line length limit; the BOM preserves Chinese in PS 5.1.
    await writeFile(scriptFile, "\uFEFF" + script, "utf8");
    child = spawn("powershell.exe", ["-NoProfile", "-NonInteractive", "-WindowStyle", "Hidden", "-ExecutionPolicy", "Bypass", "-File", scriptFile], {
      windowsHide: true, stdio: ["ignore", "pipe", "pipe"],
      env: { ...process.env, YILIAN_OWNED_DIALOG_ARGS: argsFile },
    });
    recordChild?.(child);
    let stdout = "", stderr = "";
    child.stdout.setEncoding("utf8"); child.stderr.setEncoding("utf8");
    child.stdout.on("data", value => { stdout += value; });
    child.stderr.on("data", value => { stderr += value; });
    await new Promise((resolve, reject) => {
      timer = setTimeout(() => { child.kill(); reject(new Error("Owned file-dialog helper timed out after 20 seconds")); }, 20000);
      child.once("error", reject);
      child.once("exit", code => code === 0 ? resolve() : reject(new Error(`Owned file-dialog helper failed (${code}): ${stderr}`)));
    });
    return JSON.parse(stdout.trim().replace(/^\uFEFF/, ""));
  } finally {
    clearTimeout(timer);
    if (child && child.exitCode === null && child.signalCode === null) child.kill();
    // Only these owned argument files are removed; no recursive directory delete.
    await unlink(argsFile);
    await unlink(scriptFile).catch(error => { if (error.code !== "ENOENT") throw error; });
    await rmdir(directory);
  }
}
