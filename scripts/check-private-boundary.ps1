[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$repositoryRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$trackedFiles = @(git -C $repositoryRoot ls-files)

$forbiddenRootPatterns = @(
    '^cloud-api/',
    '^admin-api/',
    '^admin-web/',
    '^workers/',
    '^deployment/private/',
    '^migrations/cloud/'
)

$violations = New-Object System.Collections.Generic.List[string]
foreach ($file in $trackedFiles) {
    $normalized = $file -replace '\\', '/'
    foreach ($pattern in $forbiddenRootPatterns) {
        if ($normalized -match $pattern) {
            $violations.Add("forbidden private implementation path: $normalized")
        }
    }
    if ($normalized -match '(^|/)\.env($|\.)' -and $normalized -notmatch '\.example$') {
        $violations.Add("tracked environment secret file: $normalized")
    }
}

$sourceExtensions = @('.rs', '.ts', '.tsx', '.js', '.jsx', '.mjs', '.cjs')
foreach ($file in $trackedFiles) {
    $normalized = $file -replace '\\', '/'
    if ($normalized -match '^(docs|cloud-contract|scripts)/') { continue }
    if ($sourceExtensions -notcontains [IO.Path]::GetExtension($normalized)) { continue }
    $absolute = Join-Path $repositoryRoot $file
    if (-not (Test-Path -LiteralPath $absolute -PathType Leaf)) { continue }
    $content = Get-Content -LiteralPath $absolute -Raw
    if ($content -match '/internal/admin(?:/|\b)') {
        $violations.Add("private Admin route in public executable source: $normalized")
    }
    if ($content -match '\b(?:AdminRbac|SupportAccessToken|PrivateAdminApi)\b') {
        $violations.Add("private Admin implementation type in public executable source: $normalized")
    }
}

if ($violations.Count -gt 0) {
    Write-Error ("PRIVATE_ADMIN_BOUNDARY_VIOLATION`n" + ($violations -join "`n"))
    exit 1
}

Write-Output "PUBLIC_PRIVATE_BOUNDARY_OK tracked_files=$($trackedFiles.Count)"
