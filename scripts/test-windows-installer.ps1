param(
    [Parameter(Mandatory = $true)]
    [string]$InstallerPath,

    [Parameter(Mandatory = $false)]
    [switch]$KeepInstallDirectory
)

$ErrorActionPreference = 'Stop'

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

$resolvedInstaller = (Resolve-Path -LiteralPath $InstallerPath).Path
if (-not (Test-Path -LiteralPath $resolvedInstaller -PathType Leaf)) {
    throw "Installer does not exist: $InstallerPath"
}
if ([System.IO.Path]::GetExtension($resolvedInstaller) -ne '.exe') {
    throw "Installer must be an executable: $resolvedInstaller"
}

$tempRoot = [System.IO.Path]::GetFullPath([System.IO.Path]::GetTempPath()).TrimEnd('\')
$installDirectory = [System.IO.Path]::GetFullPath(
    (Join-Path $tempRoot ("YiLianQianYan-installer-smoke-" + [Guid]::NewGuid().ToString('N')))
)
$tempPrefix = $tempRoot + [System.IO.Path]::DirectorySeparatorChar
if (-not $installDirectory.StartsWith($tempPrefix, [StringComparison]::OrdinalIgnoreCase)) {
    throw "Refusing to use a smoke directory outside the system temp directory: $installDirectory"
}

Write-Host "Installer smoke directory: $installDirectory"

try {
    $install = Start-Process -FilePath $resolvedInstaller `
        -ArgumentList @('/S', "/D=$installDirectory") `
        -Wait -PassThru -WindowStyle Hidden
    if ($install.ExitCode -ne 0) {
        throw "Silent install failed with exit code $($install.ExitCode)"
    }

    Wait-Until -TimeoutSeconds 20 -FailureMessage 'Installed application files did not appear.' -Condition {
        Test-Path -LiteralPath $installDirectory -PathType Container
    }

    $uninstaller = Get-ChildItem -LiteralPath $installDirectory -File -Filter 'Uninstall*.exe' |
        Select-Object -First 1
    $application = Get-ChildItem -LiteralPath $installDirectory -File -Filter '*.exe' |
        Where-Object { $_.Name -notlike 'Uninstall*.exe' } |
        Select-Object -First 1

    if (-not $application) {
        throw 'The installed application executable was not found.'
    }
    if (-not $uninstaller) {
        throw 'The installed uninstaller was not found.'
    }

    $forbiddenFiles = @(
        Get-ChildItem -LiteralPath $installDirectory -Recurse -File -ErrorAction Stop |
            Where-Object {
                $_.Name -eq '.env' -or
                $_.Extension -in @('.db', '.sqlite', '.sqlite3', '.pdb', '.pem', '.key')
            }
    )
    if ($forbiddenFiles.Count -gt 0) {
        $names = ($forbiddenFiles | ForEach-Object { $_.FullName }) -join ', '
        throw "Forbidden release files were installed: $names"
    }

    Write-Host "Installed application: $($application.Name)"
    Write-Host "Installed uninstaller: $($uninstaller.Name)"

    $uninstall = Start-Process -FilePath $uninstaller.FullName `
        -ArgumentList @('/S') `
        -Wait -PassThru -WindowStyle Hidden
    if ($uninstall.ExitCode -ne 0) {
        throw "Silent uninstall failed with exit code $($uninstall.ExitCode)"
    }

    Wait-Until -TimeoutSeconds 20 -FailureMessage 'Application executable remained after uninstall.' -Condition {
        -not (Test-Path -LiteralPath $application.FullName -PathType Leaf)
    }

    Write-Host 'Windows installer silent install/uninstall smoke passed.'
}
finally {
    if ((Test-Path -LiteralPath $installDirectory) -and -not $KeepInstallDirectory) {
        $resolvedCleanup = [System.IO.Path]::GetFullPath($installDirectory)
        if (-not $resolvedCleanup.StartsWith($tempPrefix, [StringComparison]::OrdinalIgnoreCase)) {
            throw "Refusing to clean a path outside the system temp directory: $resolvedCleanup"
        }
        Remove-Item -LiteralPath $resolvedCleanup -Recurse -Force
    }
}
