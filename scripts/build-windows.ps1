param(
    [Parameter(Mandatory = $false)]
    [ValidatePattern('^\d+\.\d+\.\d+$')]
    [string]$Version = '0.9.0',

    [Parameter(Mandatory = $false)]
    [switch]$SkipTests,

    [Parameter(Mandatory = $false)]
    [Alias('h')]
    [switch]$Help
)

$ErrorActionPreference = 'Stop'
$ProjectRoot = Split-Path -Parent $PSScriptRoot

if ($Help) {
    Write-Host 'Build the YiLianQianYan Windows x64 NSIS release bundle.'
    Write-Host ''
    Write-Host 'Usage:'
    Write-Host '  npm run build:windows'
    Write-Host '  npm run build:windows -- -Version 0.9.0'
    Write-Host '  npm run build:windows -- -SkipTests'
    exit 0
}

function Assert-Command {
    param([Parameter(Mandatory = $true)] [string]$Name)

    if (-not (Get-Command $Name -ErrorAction SilentlyContinue)) {
        throw "Required command is unavailable: $Name"
    }
}

function Invoke-NativeStep {
    param(
        [Parameter(Mandatory = $true)] [string]$Label,
        [Parameter(Mandatory = $true)] [string]$FilePath,
        [Parameter(Mandatory = $false)] [string[]]$Arguments = @()
    )

    Write-Host ''
    Write-Host "==> $Label"
    & $FilePath @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw "$Label failed with exit code $LASTEXITCODE"
    }
}

if ($env:OS -ne 'Windows_NT') {
    throw 'The Windows NSIS release must be built on Windows.'
}

Assert-Command 'node'
Assert-Command 'npm.cmd'
Assert-Command 'rustc'
Assert-Command 'cargo'

Push-Location $ProjectRoot
try {
    Write-Host "YiLianQianYan v$Version Windows release build"
    Write-Host "Project: $ProjectRoot"

    Invoke-NativeStep 'Verify release version metadata' 'powershell.exe' @(
        '-NoProfile',
        '-ExecutionPolicy', 'Bypass',
        '-File', (Join-Path $PSScriptRoot 'verify-release-version.ps1'),
        '-ExpectedVersion', $Version
    )

    if (-not $SkipTests) {
        Invoke-NativeStep 'Run frontend tests' 'npm.cmd' @('--prefix', 'frontend', 'test')
        Invoke-NativeStep 'Build frontend' 'npm.cmd' @('--prefix', 'frontend', 'run', 'build')
        Invoke-NativeStep 'Check Rust formatting' 'cargo.exe' @('fmt', '--all', '--', '--check')
        Invoke-NativeStep 'Check Rust workspace' 'cargo.exe' @('check', '--workspace', '--locked')
        Invoke-NativeStep 'Test Rust workspace' 'cargo.exe' @('test', '--workspace', '--locked')
    }

    Invoke-NativeStep 'Build Tauri NSIS bundle' 'npm.cmd' @('run', 'tauri:build')

    $metadataJson = & cargo.exe metadata --no-deps --format-version 1
    if ($LASTEXITCODE -ne 0) {
        throw "cargo metadata failed with exit code $LASTEXITCODE"
    }
    $targetDirectory = ($metadataJson | ConvertFrom-Json).target_directory
    if ([string]::IsNullOrWhiteSpace([string]$targetDirectory)) {
        throw 'cargo metadata did not return a target directory.'
    }

    $bundleDirectory = Join-Path $targetDirectory 'release\bundle\nsis'
    if (-not (Test-Path -LiteralPath $bundleDirectory -PathType Container)) {
        throw "NSIS bundle directory was not created: $bundleDirectory"
    }

    $installers = @(
        Get-ChildItem -LiteralPath $bundleDirectory -File |
            Where-Object { $_.Name -like "*_$($Version)_x64-setup.exe" }
    )
    if ($installers.Count -ne 1) {
        throw "Expected exactly one v$Version x64 NSIS installer in $bundleDirectory, found $($installers.Count)."
    }

    $installer = $installers[0]
    if ($installer.Length -le 0) {
        throw "Release installer is empty: $($installer.FullName)"
    }

    $hash = (Get-FileHash -LiteralPath $installer.FullName -Algorithm SHA256).Hash.ToUpperInvariant()
    $checksumPath = "$($installer.FullName).sha256"
    Set-Content -LiteralPath $checksumPath -Value "$hash  $($installer.Name)" -Encoding Ascii

    Write-Host ''
    Write-Host 'Windows release bundle created successfully.'
    Write-Host "Installer: $($installer.FullName)"
    Write-Host "Size:      $($installer.Length) bytes"
    Write-Host "SHA-256:   $hash"
    Write-Host "Checksum:  $checksumPath"
    Write-Host 'Signing:   unsigned'
}
finally {
    Pop-Location
}
