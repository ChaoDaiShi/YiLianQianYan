param(
    [Parameter(Mandatory = $true)]
    [string]$InstallerPath,

    [Parameter(Mandatory = $false)]
    [string]$ScreenshotPath = (Join-Path ([System.IO.Path]::GetTempPath()) 'YiLianQianYan-v0.9.0-gui-acceptance.png'),

    [Parameter(Mandatory = $false)]
    [string]$StaleDirectory
)

$ErrorActionPreference = 'Stop'
$process = $null
$installDirectory = $null
$uninstaller = $null

function Wait-Until {
    param(
        [Parameter(Mandatory = $true)] [scriptblock]$Condition,
        [Parameter(Mandatory = $true)] [int]$TimeoutSeconds,
        [Parameter(Mandatory = $true)] [string]$FailureMessage
    )

    $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
    while ([DateTime]::UtcNow -lt $deadline) {
        if (& $Condition) {
            return
        }
        Start-Sleep -Milliseconds 250
    }

    throw $FailureMessage
}

function Assert-TempChildPath {
    param([Parameter(Mandatory = $true)] [string]$Path)

    $tempRoot = [System.IO.Path]::GetFullPath([System.IO.Path]::GetTempPath()).TrimEnd('\')
    $resolved = [System.IO.Path]::GetFullPath($Path)
    $prefix = $tempRoot + [System.IO.Path]::DirectorySeparatorChar
    if (-not $resolved.StartsWith($prefix, [StringComparison]::OrdinalIgnoreCase)) {
        throw "Path is outside the system temp directory: $resolved"
    }
    return $resolved
}

$resolvedInstaller = (Resolve-Path -LiteralPath $InstallerPath).Path
if (-not (Test-Path -LiteralPath $resolvedInstaller -PathType Leaf)) {
    throw "Installer does not exist: $InstallerPath"
}

$existingListener = Get-NetTCPConnection -LocalAddress '127.0.0.1' -LocalPort 9420 -State Listen -ErrorAction SilentlyContinue
if ($existingListener) {
    throw 'Port 9420 is already in use; GUI acceptance cannot attribute health to the packaged application.'
}

if (-not [string]::IsNullOrWhiteSpace($StaleDirectory)) {
    $resolvedStale = Assert-TempChildPath $StaleDirectory
    $runningFromStale = Get-Process -ErrorAction SilentlyContinue |
        Where-Object {
            try {
                -not [string]::IsNullOrWhiteSpace($_.Path) -and
                    $_.Path.StartsWith($resolvedStale, [StringComparison]::OrdinalIgnoreCase)
            }
            catch {
                $false
            }
        }
    if ($runningFromStale) {
        throw "A process is still running from the stale acceptance directory: $resolvedStale"
    }
    if (Test-Path -LiteralPath $resolvedStale) {
        Remove-Item -LiteralPath $resolvedStale -Recurse -Force
        Write-Host "Removed stale GUI acceptance directory: $resolvedStale"
    }
}

$installDirectory = Assert-TempChildPath (
    Join-Path ([System.IO.Path]::GetTempPath()) (
        'YiLianQianYan-gui-acceptance-' + [Guid]::NewGuid().ToString('N')
    )
)
$resolvedScreenshot = Assert-TempChildPath $ScreenshotPath

Write-Host "GUI acceptance directory: $installDirectory"

try {
    $install = Start-Process -FilePath $resolvedInstaller `
        -ArgumentList @('/S', "/D=$installDirectory") `
        -Wait -PassThru -WindowStyle Hidden
    if ($install.ExitCode -ne 0) {
        throw "Silent install failed with exit code $($install.ExitCode)"
    }

    $applicationPath = Join-Path $installDirectory 'yi-lian-qian-yan.exe'
    $uninstaller = Get-ChildItem -LiteralPath $installDirectory -File -Filter 'Uninstall*.exe' |
        Select-Object -First 1
    if (-not (Test-Path -LiteralPath $applicationPath -PathType Leaf)) {
        throw "Installed application executable was not found: $applicationPath"
    }
    if (-not $uninstaller) {
        throw 'Installed uninstaller was not found.'
    }

    $process = Start-Process -FilePath $applicationPath -PassThru
    Wait-Until -TimeoutSeconds 30 -FailureMessage 'Packaged app did not create a visible main window.' -Condition {
        if ($process.HasExited) {
            throw "Packaged app exited early with code $($process.ExitCode)"
        }
        $process.Refresh()
        return $process.MainWindowHandle -ne 0
    }
    Write-Host "Visible packaged window: PID $($process.Id), handle $($process.MainWindowHandle), title '$($process.MainWindowTitle)'"

    $script:health = $null
    Wait-Until -TimeoutSeconds 20 -FailureMessage 'Packaged backend health endpoint did not become ready.' -Condition {
        try {
            $script:health = Invoke-RestMethod -Uri 'http://127.0.0.1:9420/api/health' -TimeoutSec 2
            return $script:health.status -eq 'healthy' -and $script:health.version -eq '0.9.0'
        }
        catch {
            return $false
        }
    }
    $health = $script:health
    Write-Host "Packaged health: $($health.status), version $($health.version)"

    Add-Type -AssemblyName System.Drawing
    if (-not ('YiLianWindowCapture' -as [type])) {
        Add-Type @'
using System;
using System.Runtime.InteropServices;

public struct YiLianRect
{
    public int Left;
    public int Top;
    public int Right;
    public int Bottom;
}

public static class YiLianWindowCapture
{
    [DllImport("user32.dll")]
    [return: MarshalAs(UnmanagedType.Bool)]
    public static extern bool GetWindowRect(IntPtr handle, out YiLianRect rect);

    [DllImport("user32.dll")]
    [return: MarshalAs(UnmanagedType.Bool)]
    public static extern bool PrintWindow(IntPtr handle, IntPtr deviceContext, uint flags);

    [DllImport("user32.dll")]
    [return: MarshalAs(UnmanagedType.Bool)]
    public static extern bool ShowWindow(IntPtr handle, int command);

    [DllImport("user32.dll")]
    [return: MarshalAs(UnmanagedType.Bool)]
    public static extern bool SetForegroundWindow(IntPtr handle);
}
'@
    }

    $rect = New-Object YiLianRect
    if (-not [YiLianWindowCapture]::GetWindowRect([IntPtr]$process.MainWindowHandle, [ref]$rect)) {
        throw 'Failed to read the packaged app window bounds.'
    }
    $width = $rect.Right - $rect.Left
    $height = $rect.Bottom - $rect.Top
    if ($width -lt 800 -or $height -lt 600) {
        throw "Packaged app window is unexpectedly small: ${width}x${height}"
    }
    Write-Host "Capturing packaged window: ${width}x${height}"

    $windowHandle = [IntPtr]$process.MainWindowHandle
    $null = [YiLianWindowCapture]::ShowWindow($windowHandle, 9)
    $null = [YiLianWindowCapture]::SetForegroundWindow($windowHandle)
    Start-Sleep -Seconds 2

    $bitmap = New-Object System.Drawing.Bitmap($width, $height)
    $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
    try {
        $deviceContext = $graphics.GetHdc()
        try {
            $rendered = [YiLianWindowCapture]::PrintWindow($windowHandle, $deviceContext, 2)
        }
        finally {
            $graphics.ReleaseHdc($deviceContext)
        }
        if (-not $rendered) {
            throw 'PrintWindow could not capture the packaged app window.'
        }

        $bitmap.Save($resolvedScreenshot, [System.Drawing.Imaging.ImageFormat]::Png)

        $sampledColors = New-Object 'System.Collections.Generic.HashSet[int]'
        $sampledPixels = 0
        $nonDarkPixels = 0
        for ($x = 0; $x -lt $bitmap.Width; $x += 24) {
            for ($y = 0; $y -lt $bitmap.Height; $y += 24) {
                $pixel = $bitmap.GetPixel($x, $y)
                $null = $sampledColors.Add($pixel.ToArgb())
                $sampledPixels += 1
                if (($pixel.R + $pixel.G + $pixel.B) -gt 30) {
                    $nonDarkPixels += 1
                }
            }
        }
        $nonDarkRatio = $nonDarkPixels / [double]$sampledPixels
        if ($sampledColors.Count -lt 8 -or $nonDarkRatio -lt 0.05) {
            throw "Packaged app screenshot is blank: colors=$($sampledColors.Count), nonDarkRatio=$nonDarkRatio"
        }
        Write-Host "Screenshot content check: $($sampledColors.Count) sampled colors, $([Math]::Round($nonDarkRatio * 100, 1))% non-dark"
    }
    finally {
        $graphics.Dispose()
        $bitmap.Dispose()
    }

    $screenshot = Get-Item -LiteralPath $resolvedScreenshot
    if ($screenshot.Length -le 1024) {
        throw "GUI acceptance screenshot is unexpectedly small: $($screenshot.Length) bytes"
    }

    [pscustomobject]@{
        ProcessId = $process.Id
        WindowHandle = $process.MainWindowHandle
        WindowTitle = $process.MainWindowTitle
        WindowSize = "${width}x${height}"
        HealthStatus = $health.status
        HealthService = $health.service
        HealthVersion = $health.version
        Database = $health.database
        Screenshot = $resolvedScreenshot
        ScreenshotSize = $screenshot.Length
    } | Format-List

    if (-not $process.CloseMainWindow()) {
        throw 'Packaged app did not accept a normal window close request.'
    }
    if (-not $process.WaitForExit(15000)) {
        throw 'Packaged app did not exit within 15 seconds after a normal close request.'
    }

    Wait-Until -TimeoutSeconds 10 -FailureMessage 'Backend port remained open after packaged app exit.' -Condition {
        -not (Get-NetTCPConnection -LocalAddress '127.0.0.1' -LocalPort 9420 -State Listen -ErrorAction SilentlyContinue)
    }

    $uninstall = Start-Process -FilePath $uninstaller.FullName `
        -ArgumentList @('/S') `
        -Wait -PassThru -WindowStyle Hidden
    if ($uninstall.ExitCode -ne 0) {
        throw "Silent uninstall failed with exit code $($uninstall.ExitCode)"
    }

    Write-Host 'Packaged GUI launch, health, normal close, and uninstall acceptance passed.'
}
finally {
    if ($process -and -not $process.HasExited) {
        Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue
        $null = $process.WaitForExit(10000)
    }

    if ($installDirectory -and (Test-Path -LiteralPath $installDirectory)) {
        $resolvedCleanup = Assert-TempChildPath $installDirectory
        $removed = $false
        for ($attempt = 1; $attempt -le 20 -and -not $removed; $attempt++) {
            try {
                Remove-Item -LiteralPath $resolvedCleanup -Recurse -Force -ErrorAction Stop
                $removed = $true
            }
            catch {
                Start-Sleep -Milliseconds 250
            }
        }
        if (-not $removed -and (Test-Path -LiteralPath $resolvedCleanup)) {
            Write-Warning "GUI acceptance cleanup could not remove: $resolvedCleanup"
        }
    }
}
