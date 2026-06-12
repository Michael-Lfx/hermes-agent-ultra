# Split cron WeCom/delivery work into 5 feature commits (PR0 -> PR4).
$ErrorActionPreference = "Stop"
$env:Path = "$env:USERPROFILE\.cargo\bin;" + $env:Path
Set-Location (Join-Path $PSScriptRoot "..")

$bak = Join-Path (Get-Location) ".cron-pr-backup"
New-Item -ItemType Directory -Force -Path $bak | Out-Null

function Backup-File([string]$rel) {
    $src = Join-Path (Get-Location) $rel
    if (-not (Test-Path $src)) { throw "missing backup source: $rel" }
    $dest = Join-Path $bak ($rel -replace "/", [IO.Path]::DirectorySeparatorChar)
    $parent = Split-Path $dest -Parent
    if (-not (Test-Path $parent)) { New-Item -ItemType Directory -Force -Path $parent | Out-Null }
    Copy-Item -Force $src $dest
}

function Restore-File([string]$rel) {
    $dest = Join-Path (Get-Location) $rel
    $src = Join-Path $bak ($rel -replace "/", [IO.Path]::DirectorySeparatorChar)
    $parent = Split-Path $dest -Parent
    if (-not (Test-Path $parent)) { New-Item -ItemType Directory -Force -Path $parent | Out-Null }
    Copy-Item -Force $src $dest
}

function Write-CronLibPr0() {
    $path = "crates/hermes-cron/src/lib.rs"
    $head = git show "HEAD:$path"
    if ($LASTEXITCODE -ne 0) { throw "git show lib.rs failed" }
    $extra = @"

pub mod python_job;
pub mod schedule;

pub use python_job::JobOrigin;
pub use schedule::{parse_schedule, ScheduleParseError, ScheduleSpec};
"@
    if ($head -notmatch "pub mod python_job") {
        $head = $head.TrimEnd() + $extra
    }
    [System.IO.File]::WriteAllText((Join-Path (Get-Location) $path), $head, (New-Object System.Text.UTF8Encoding $false))
}

function Strip-PersistenceWecomTest() {
    $path = Join-Path (Get-Location) "crates/hermes-cron/src/persistence.rs"
    $text = [System.IO.File]::ReadAllText($path)
    $pattern = '(?s)\r?\n    #\[test\]\r?\n    fn test_parse_per_job_json_wecom_deliver_target\(\) \{.*?\r?\n    \}\r?\n'
    $text = [regex]::Replace($text, $pattern, "`n")
    [System.IO.File]::WriteAllText($path, $text, (New-Object System.Text.UTF8Encoding $false))
}

function Write-DeliverSlugIntegrationTest() {
    $dir = Join-Path (Get-Location) "crates/hermes-cron/tests"
    New-Item -ItemType Directory -Force -Path $dir | Out-Null
    $path = Join-Path $dir "deliver_slug.rs"
    $content = @'
//! Regression: cron deliver JSON uses Python platform slugs (`wecom`), not `we_com`.

use hermes_cron::{CronJob, DeliverTarget};

#[test]
fn per_job_json_wecom_deliver_object() {
    let contents = r#"{
  "id": "d0b0cf77-bd3f-4ab7-9ac6-b4553cdfb76e",
  "schedule": "every 2h",
  "prompt": "喝水",
  "deliver": { "target": "wecom" },
  "origin": { "platform": "wecom", "chat_id": "wrPMNBUgAAxFJsvKPM6tTJ2csX586dqQ" },
  "created_at": "2026-05-17T17:27:05.435702300Z"
}"#;
    let job: CronJob = serde_json::from_str(contents).expect("wecom job file");
    assert_eq!(
        job.deliver.as_ref().map(|d| d.target),
        Some(DeliverTarget::WeCom)
    );
}

#[test]
fn deliver_string_and_object_roundtrip_wecom() {
    let ts = "2026-05-17T17:27:05Z";
    let object = format!(
        r#"{{"id":"x","schedule":"every 2h","prompt":"p","created_at":"{ts}","deliver":{{"target":"wecom"}}}}"#
    );
    let job: CronJob = serde_json::from_str(&object).expect("object deliver");
    assert_eq!(
        job.deliver.as_ref().map(|d| d.target),
        Some(DeliverTarget::WeCom)
    );

    let string = format!(
        r#"{{"id":"y","schedule":"every 2h","prompt":"p","created_at":"{ts}","deliver":"wecom"}}"#
    );
    let job: CronJob = serde_json::from_str(&string).expect("string deliver");
    assert_eq!(
        job.deliver.as_ref().map(|d| d.target),
        Some(DeliverTarget::WeCom)
    );

    let json = serde_json::to_string(&job).unwrap();
    assert!(json.contains(r#""target":"wecom""#));
    assert!(!json.contains("we_com"));
}
'@
    [System.IO.File]::WriteAllText($path, $content, (New-Object System.Text.UTF8Encoding $false))
}

function Do-Commit([string]$msg, [string[]]$paths) {
    git add -- @paths
    if ($LASTEXITCODE -ne 0) { throw "git add failed" }
    git commit -m $msg
    if ($LASTEXITCODE -ne 0) { throw "git commit failed: $msg" }
}

$allFiles = @(
    "crates/hermes-cron/src/python_job.rs",
    "crates/hermes-cron/src/schedule.rs",
    "crates/hermes-cron/src/delivery.rs",
    "crates/hermes-cron/src/job.rs",
    "crates/hermes-cron/src/persistence.rs",
    "crates/hermes-cron/src/runner.rs",
    "crates/hermes-cron/src/scheduler.rs",
    "crates/hermes-cron/src/lib.rs",
    "crates/hermes-cli/src/cron_delivery.rs",
    "crates/hermes-cli/src/commands.rs",
    "crates/hermes-cli/src/lib.rs",
    "crates/hermes-cli/src/main.rs"
)
foreach ($f in $allFiles) { Backup-File $f }

git checkout HEAD -- `
    crates/hermes-cron/src/job.rs `
    crates/hermes-cron/src/persistence.rs `
    crates/hermes-cron/src/runner.rs `
    crates/hermes-cron/src/scheduler.rs `
    crates/hermes-cron/src/lib.rs `
    crates/hermes-cli/src/commands.rs `
    crates/hermes-cli/src/lib.rs `
    crates/hermes-cli/src/main.rs
if ($LASTEXITCODE -ne 0) { throw "git checkout failed" }

@(
    "crates/hermes-cron/src/python_job.rs",
    "crates/hermes-cron/src/schedule.rs",
    "crates/hermes-cron/src/delivery.rs",
    "crates/hermes-cli/src/cron_delivery.rs"
) | ForEach-Object {
    $p = Join-Path (Get-Location) $_
    if (Test-Path $p) { Remove-Item -Force $p }
}

# PR0
Restore-File "crates/hermes-cron/src/python_job.rs"
Restore-File "crates/hermes-cron/src/schedule.rs"
Restore-File "crates/hermes-cron/src/persistence.rs"
Strip-PersistenceWecomTest
Restore-File "crates/hermes-cli/src/commands.rs"
Write-CronLibPr0
Do-Commit "feat(cron): load Python jobs.json alongside per-job JSON files" @(
    "crates/hermes-cron/src/python_job.rs",
    "crates/hermes-cron/src/schedule.rs",
    "crates/hermes-cron/src/persistence.rs",
    "crates/hermes-cron/src/lib.rs",
    "crates/hermes-cli/src/commands.rs"
)

# PR1
Restore-File "crates/hermes-cron/src/job.rs"
Restore-File "crates/hermes-cron/src/scheduler.rs"
Restore-File "crates/hermes-cron/src/lib.rs"
Do-Commit "feat(cron): structured schedule parsing and stale next_run recovery" @(
    "crates/hermes-cron/src/job.rs",
    "crates/hermes-cron/src/scheduler.rs",
    "crates/hermes-cron/src/lib.rs"
)

# PR2 (cron crate + gateway adapter; main.rs wired in PR3)
Restore-File "crates/hermes-cron/src/delivery.rs"
Restore-File "crates/hermes-cron/src/runner.rs"
Restore-File "crates/hermes-cli/src/cron_delivery.rs"
Restore-File "crates/hermes-cron/src/lib.rs"
Restore-File "crates/hermes-cli/src/lib.rs"
Do-Commit "feat(cron): gateway delivery backend for cron job results" @(
    "crates/hermes-cron/src/delivery.rs",
    "crates/hermes-cron/src/runner.rs",
    "crates/hermes-cron/src/lib.rs",
    "crates/hermes-cli/src/cron_delivery.rs",
    "crates/hermes-cli/src/lib.rs"
)

# PR3: deliver resolution helpers already in delivery.rs; wire CLI deliver parsing + cron edit refresh
Restore-File "crates/hermes-cli/src/main.rs"
Do-Commit "feat(cron): deliver target resolution and wecom:chat_id CLI parsing" @(
    "crates/hermes-cli/src/main.rs"
)

# PR4: serde slug fix tests (wecom); persistence unit test + integration test crate
Restore-File "crates/hermes-cron/src/persistence.rs"
Write-DeliverSlugIntegrationTest
Do-Commit "fix(cron): deliver platform slugs match Python (wecom, dingtalk)" @(
    "crates/hermes-cron/src/persistence.rs",
    "crates/hermes-cron/tests/deliver_slug.rs"
)

Write-Host "`nCreated commits:"
git log --oneline -6

Remove-Item -Recurse -Force $bak -ErrorAction SilentlyContinue
