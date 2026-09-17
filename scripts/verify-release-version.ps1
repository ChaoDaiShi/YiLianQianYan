param(
    [Parameter(Mandatory = $false)]
    [ValidatePattern('^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$')]
    [string]$ExpectedVersion = '1.0.0-rc.1'
)

$ErrorActionPreference = 'Stop'
$ProjectRoot = Split-Path -Parent $PSScriptRoot

function Assert-Version {
    param(
        [Parameter(Mandatory = $true)] [string]$Label,
        [Parameter(Mandatory = $true)] [string]$Actual
    )

    if ($Actual -ne $ExpectedVersion) {
        throw "$Label version mismatch: expected $ExpectedVersion, found $Actual"
    }

    Write-Host "  PASS $Label = $Actual"
}

function Read-JsonVersion {
    param([Parameter(Mandatory = $true)] [string]$RelativePath)

    $path = Join-Path $ProjectRoot $RelativePath
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
        throw "Missing release metadata file: $RelativePath"
    }

    $document = Get-Content -LiteralPath $path -Raw | ConvertFrom-Json
    if ([string]::IsNullOrWhiteSpace([string]$document.version)) {
        throw "Missing version in $RelativePath"
    }

    return [string]$document.version
}

function Read-CargoPackageVersion {
    param([Parameter(Mandatory = $true)] [string]$RelativePath)

    $path = Join-Path $ProjectRoot $RelativePath
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
        throw "Missing Cargo manifest: $RelativePath"
    }

    $content = Get-Content -LiteralPath $path -Raw
    $match = [regex]::Match(
        $content,
        '(?ms)^\[package\]\s*.*?^version\s*=\s*"([^"]+)"'
    )
    if (-not $match.Success) {
        throw "Missing [package] version in $RelativePath"
    }

    return $match.Groups[1].Value
}

function Read-LockPackageVersion {
    param([Parameter(Mandatory = $true)] [string]$PackageName)

    $path = Join-Path $ProjectRoot 'Cargo.lock'
    $content = Get-Content -LiteralPath $path -Raw
    $escapedName = [regex]::Escape($PackageName)
    $match = [regex]::Match(
        $content,
        "(?ms)^\[\[package\]\]\s*^name\s*=\s*`"$escapedName`"\s*^version\s*=\s*`"([^`"]+)`""
    )
    if (-not $match.Success) {
        throw "Missing $PackageName package entry in Cargo.lock"
    }

    return $match.Groups[1].Value
}

Write-Host "Verifying YiLianQianYan release metadata ($ExpectedVersion)"

Assert-Version 'package.json' (Read-JsonVersion 'package.json')
Assert-Version 'frontend/package.json' (Read-JsonVersion 'frontend/package.json')
Assert-Version 'src-tauri/tauri.conf.json' (Read-JsonVersion 'src-tauri/tauri.conf.json')
Assert-Version 'backend/Cargo.toml' (Read-CargoPackageVersion 'backend/Cargo.toml')
Assert-Version 'src-tauri/Cargo.toml' (Read-CargoPackageVersion 'src-tauri/Cargo.toml')
Assert-Version 'Cargo.lock:yilian-backend' (Read-LockPackageVersion 'yilian-backend')
Assert-Version 'Cargo.lock:yi-lian-qian-yan' (Read-LockPackageVersion 'yi-lian-qian-yan')

Write-Host 'Release metadata is consistent.'
