param(
  [Parameter(Mandatory=$true)][string]$PublicRoot,
  [Parameter(Mandatory=$true)][string]$PrivateRoot,
  [Parameter(Mandatory=$true)][string]$EvidenceRoot
)

$ErrorActionPreference = "Stop"
$resolvedPublic = (Resolve-Path -LiteralPath $PublicRoot).Path
$resolvedPrivate = (Resolve-Path -LiteralPath $PrivateRoot).Path
if (-not (Test-Path -LiteralPath (Join-Path $resolvedPublic "backend\Cargo.toml"))) {
  throw "Invalid PublicRoot: backend/Cargo.toml is missing"
}
if (-not (Test-Path -LiteralPath (Join-Path $resolvedPrivate "Cargo.toml"))) {
  throw "Invalid PrivateRoot: Cargo.toml is missing"
}
New-Item -ItemType Directory -Path $EvidenceRoot -Force | Out-Null
$resolvedEvidence = (Resolve-Path -LiteralPath $EvidenceRoot).Path
$runId = "run-" + [Guid]::NewGuid().ToString("N").Substring(0, 12)

$testDbUrl = $env:YILIAN_TEST_DATABASE_URL
$hasTestDb = $false
if ($testDbUrl -and $testDbUrl -match "(/|db=)([^/?]+_test)($|\?)") {
  $hasTestDb = $true
}

function Invoke-TestCase {
  param(
    [string]$CaseId,
    [string]$WorkingDirectory,
    [string]$Program,
    [string[]]$Arguments,
    [bool]$RequiresDatabase = $false
  )
  $logPath = Join-Path $resolvedEvidence "$CaseId.log"
  if ($RequiresDatabase -and -not $hasTestDb) {
    "NOT_TESTED: YILIAN_TEST_DATABASE_URL must identify an isolated database ending in _test." |
      Set-Content -LiteralPath $logPath -Encoding UTF8
    return [pscustomobject]@{case_id=$CaseId;status="NOT_TESTED";exit_code=$null;evidence_file=$logPath}
  }
  Push-Location $WorkingDirectory
  try {
    $output = & $Program @Arguments 2>&1
    $exitCode = $LASTEXITCODE
    $output | Set-Content -LiteralPath $logPath -Encoding UTF8
    $joined = $output -join "`n"
    $zeroTests = $joined -match "running 0 tests"
    $rustSuccess = $Program -notlike "cargo*" -or $joined -match "test result: ok"
    $status = if ($exitCode -eq 2) {
      "NOT_TESTED"
    } elseif ($exitCode -eq 0 -and -not $zeroTests -and $rustSuccess) {
      "PASS"
    } else {
      "FAIL"
    }
    return [pscustomobject]@{case_id=$CaseId;status=$status;exit_code=$exitCode;evidence_file=$logPath}
  } catch {
    $_ | Out-String | Set-Content -LiteralPath $logPath -Encoding UTF8
    return [pscustomobject]@{case_id=$CaseId;status="FAIL";exit_code=1;evidence_file=$logPath}
  } finally {
    Pop-Location
  }
}

$cases = @(
  @{Id="E01";Root=$resolvedPrivate;Program="cargo";Args=@("test","-p","yilian-cloud-core","--test","sync_postgres","test_response_lost_replay_is_idempotent","--","--exact","--ignored","--test-threads=1");Db=$true},
  @{Id="E02";Root=$resolvedPrivate;Program="cargo";Args=@("test","-p","yilian-cloud-core","--test","sync_postgres","test_same_owner_waits_before_sequence_allocation","--","--exact","--ignored","--test-threads=1");Db=$true},
  @{Id="E03";Root=$resolvedPublic;Program="cargo";Args=@("test","-p","yilian-backend","--test","v11_sync_atomicity","test_page_replay_after_crash_is_idempotent","--","--exact");Db=$false},
  @{Id="E04";Root=$resolvedPublic;Program="cargo";Args=@("test","-p","yilian-backend","--test","v11_sync_atomicity","test_page_failure_rolls_back_everything","--","--exact");Db=$false},
  @{Id="E05";Root=$resolvedPublic;Program="cargo";Args=@("test","-p","yilian-backend","--test","v11_profile_isolation","test_s3_1_profile_physical_isolation_prevents_business_data_leak","--","--exact");Db=$false},
  @{Id="E06";Root=$resolvedPublic;Program="cargo";Args=@("test","-p","yilian-backend","--test","v11_sync_atomicity","test_scope_switch_rejects_late_ack","--","--exact");Db=$false},
  @{Id="E07";Root=$resolvedPrivate;Program="cargo";Args=@("test","-p","yilian-cloud-core","--test","device_owner_postgres","test_device_owner_and_closing_state_in_postgres","--","--exact","--ignored","--test-threads=1");Db=$true},
  @{Id="E08";Root=$resolvedPublic;Program="cargo";Args=@("test","-p","yilian-backend","--test","v11_conflict_recovery","test_choose_remote_upsert_with_null_payload_fails","--","--exact");Db=$false},
  @{Id="E09";Root=$resolvedPublic;Program="cargo";Args=@("test","-p","yilian-backend","--test","v11_conflict_recovery","test_remote_tombstone_local_upsert_restore_requires_new_entity_and_keep_local_as_copy","--","--exact");Db=$false},
  @{Id="E10";Root=$resolvedPublic;Program="cargo";Args=@("test","-p","yilian-backend","--test","v11_review_regressions","failed_push_is_retried_with_the_same_idempotency_key","--","--exact");Db=$false},
  @{Id="E11";Root=$resolvedPrivate;Program="cargo";Args=@("test","-p","yilian-cloud-core","--test","lifecycle_files","test_delete_registered_object_permission_denied_classification","--","--exact");Db=$false},
  @{Id="E12";Root=$resolvedPrivate;Program="cargo";Args=@("test","-p","yilian-cloud-core","--test","lifecycle_postgres","test_lifecycle_jobs_postgres_integration","--","--exact","--ignored","--test-threads=1");Db=$true},
  @{Id="E13";Root=$resolvedPrivate;Program="cargo";Args=@("test","-p","yilian-cloud-core","--test","sync_postgres","test_closing_wins_against_late_push","--","--exact","--ignored","--test-threads=1");Db=$true},
  @{Id="E14";Root=$resolvedPrivate;Program="cargo";Args=@("test","-p","yilian-cloud-core","--test","lifecycle_postgres","test_export_download_is_owner_scoped_and_expires","--","--exact","--ignored","--test-threads=1");Db=$true},
  @{Id="E15";Root=$resolvedPrivate;Program="cargo";Args=@("test","-p","yilian-cloud-core","--test","review_v11_regressions","--","--test-threads=1");Db=$false},
  @{Id="E16";Root=$resolvedPublic;Program="cargo";Args=@("test","-p","yilian-backend","--test","v11_review_regressions","advertised_capability_usage_is_projected_on_pull","--","--exact");Db=$false},
  @{Id="E17";Root=$resolvedPublic;Program="cargo";Args=@("test","-p","yilian-backend","--test","v11_payload_policy","test_corrupt_grant_fields_fail_closed","--","--exact");Db=$false},
  @{Id="E18";Root=$resolvedPrivate;Program="cargo";Args=@("test","-p","yilian-cloud-core","--test","sync_postgres","test_legacy_client_is_blocked","--","--exact","--ignored","--test-threads=1");Db=$true},
  @{Id="E19";Root=$resolvedPublic;Program="cargo";Args=@("test","-p","yilian-backend","--test","v11_sync_atomicity","test_reconcile_repairs_skipped_history","--","--exact");Db=$false},
  @{Id="E20";Root=$resolvedPrivate;Program="powershell";Args=@("-NoProfile","-File",(Join-Path $resolvedPrivate "scripts\verify-backup-restore.ps1"),"-TestDatabaseUrl",$testDbUrl);Db=$true}
)

$results = [Collections.Generic.List[object]]::new()
foreach ($case in $cases) {
  $result = Invoke-TestCase -CaseId $case.Id -WorkingDirectory $case.Root -Program $case.Program -Arguments $case.Args -RequiresDatabase $case.Db
  $results.Add($result)
  Write-Host "[$($result.status)] $($result.case_id)"
}

foreach ($manual in @("E21", "E22")) {
  $logPath = Join-Path $resolvedEvidence "$manual.log"
  "NOT_TESTED: requires visible installer lifecycle or verified running deployment metadata." |
    Set-Content -LiteralPath $logPath -Encoding UTF8
  $results.Add([pscustomobject]@{case_id=$manual;status="NOT_TESTED";exit_code=$null;evidence_file=$logPath})
}

$results | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath (Join-Path $resolvedEvidence "result.json") -Encoding UTF8
$summary = [pscustomobject]@{
  run_id = $runId
  passed = @($results | Where-Object status -eq "PASS").Count
  failed = @($results | Where-Object status -eq "FAIL").Count
  not_tested = @($results | Where-Object status -eq "NOT_TESTED").Count
}
$summary | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $resolvedEvidence "summary.json") -Encoding UTF8
if ($summary.failed -gt 0) { exit 1 }
if ($summary.not_tested -gt 0) { exit 2 }
exit 0
