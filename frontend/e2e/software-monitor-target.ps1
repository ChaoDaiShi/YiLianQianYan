# Synthetic window lifecycle fixture. Only its own forms are created or modified.
param([Parameter(Mandatory=$true)][string]$FixtureDirectory)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Windows.Forms
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class MonitorFixtureWin32 {
    [StructLayout(LayoutKind.Sequential)]
    public struct Point { public int X, Y; }
    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] public static extern IntPtr WindowFromPoint(Point point);
    [DllImport("user32.dll")] public static extern bool IsIconic(IntPtr window);
    [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr window, IntPtr after, int x, int y, int width, int height, uint flags);
}
'@

$script:target = $null
$script:cover = $null
$script:lastCommand = ''
$commandFile = Join-Path $FixtureDirectory 'command.txt'
$stateFile = Join-Path $FixtureDirectory 'state.json'
$script:fixtureToasts = @()
$fixtureAppId = '{1AC14E77-02E7-4E5D-B744-2EB1AE5198B7}\WindowsPowerShell\v1.0\powershell.exe'

function Publish-FixtureNotification {
    $xml = [Windows.Data.Xml.Dom.XmlDocument, Windows.Data.Xml.Dom.XmlDocument, ContentType=WindowsRuntime]::new()
    $xml.LoadXml('<toast><visual><binding template="ToastGeneric"><text>SYNTHETIC_BODY_ONLY</text></binding></visual></toast>')
    $toast = [Windows.UI.Notifications.ToastNotification, Windows.UI.Notifications, ContentType=WindowsRuntime]::new($xml)
    $toast.Tag = [guid]::NewGuid().ToString('N').Substring(0, 12)
    $toast.Group = 'yilian-fixture'
    $toast.SuppressPopup = $true
    $toast.ExpirationTime = [DateTimeOffset]::Now.AddSeconds(45)
    $manager = [Windows.UI.Notifications.ToastNotificationManager, Windows.UI.Notifications, ContentType=WindowsRuntime]
    $manager::CreateToastNotifier($fixtureAppId).Show($toast)
    $script:fixtureToasts += $toast.Tag
}

function New-FixtureForm([string]$name, [int]$left, [int]$top, [int]$width, [int]$height) {
    $form = [System.Windows.Forms.Form]::new()
    $form.Text = $name
    $form.StartPosition = 'Manual'
    $form.Bounds = [System.Drawing.Rectangle]::new($left, $top, $width, $height)
    $form.Show()
    [void][MonitorFixtureWin32]::SetWindowPos($form.Handle, [IntPtr]::Zero, 0, 0, 0, 0, 0x57)
    return $form
}

function Publish-FixtureState([string]$action) {
    $handle = if ($script:target) { $script:target.Handle } else { [IntPtr]::Zero }
    $hit = [IntPtr]::Zero
    if ($script:target) {
        $point = [MonitorFixtureWin32+Point]::new()
        $point.X = $script:target.Left + [int]($script:target.Width / 2)
        $point.Y = $script:target.Top + [int]($script:target.Height / 2)
        $hit = [MonitorFixtureWin32]::WindowFromPoint($point)
    }
    [ordered]@{
        action = $action
        hwnd = $handle.ToInt64().ToString()
        alternate_hwnd = if ($script:cover) { $script:cover.Handle.ToInt64().ToString() } else { $null }
        visible = [bool]($script:target -and $script:target.Visible)
        foreground = ($handle -ne [IntPtr]::Zero -and [MonitorFixtureWin32]::GetForegroundWindow() -eq $handle)
        covered = [bool]($script:cover -and $hit -eq $script:cover.Handle)
        minimized = ($handle -ne [IntPtr]::Zero -and [MonitorFixtureWin32]::IsIconic($handle))
    } | ConvertTo-Json | Set-Content -LiteralPath $stateFile -Encoding utf8
}

$script:target = New-FixtureForm 'Synthetic monitored window' 90 90 320 230
Publish-FixtureState 'ready'
$timer = [System.Windows.Forms.Timer]::new()
$timer.Interval = 70
$timer.Add_Tick({
    if (!(Test-Path -LiteralPath $commandFile)) { return }
    $command = (Get-Content -LiteralPath $commandFile -Raw).Trim()
    if (!$command -or $command -eq $script:lastCommand) { return }
    $script:lastCommand = $command
    $action = $command.Split(':')[1]
    switch ($action) {
        'defocus' {
            $script:cover = New-FixtureForm 'Synthetic alternate window' 430 90 180 150
            $script:cover.Activate()
        }
        'cover' {
            $script:cover = New-FixtureForm 'Synthetic cover window' 75 75 370 280
            $script:cover.TopMost = $true
            $script:cover.Activate()
        }
        'close' {
            if ($script:cover) { $script:cover.Close(); $script:cover = $null }
            $script:target.Close()
            $script:target = $null
        }
        'rebuild' { $script:target = New-FixtureForm 'Synthetic rebuilt window' 90 90 320 230 }
        'minimize' { $script:target.WindowState = 'Minimized' }
        'notify' { Publish-FixtureNotification }
        'exit' { [System.Windows.Forms.Application]::ExitThread() }
    }
    Publish-FixtureState $command
})
$timer.Start()
[System.Windows.Forms.Application]::Run()
$timer.Dispose()
if ($script:fixtureToasts.Count -gt 0) {
    $manager = [Windows.UI.Notifications.ToastNotificationManager, Windows.UI.Notifications, ContentType=WindowsRuntime]
    foreach ($tag in $script:fixtureToasts) {
        try { $manager::History.Remove($tag, 'yilian-fixture', $fixtureAppId) } catch {}
    }
}
