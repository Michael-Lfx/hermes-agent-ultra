# 12 PR-granular commits for Phase A contract tests.
$ErrorActionPreference = "Stop"
$env:Path = "$env:USERPROFILE\.cargo\bin;" + $env:Path
Set-Location (Join-Path $PSScriptRoot "..")

$fullPath = "crates\hermes-agent\tests\phase_a_pr2.rs"
$utf8Enc = New-Object System.Text.UTF8Encoding $false
$all = Get-Content $fullPath -Encoding Unicode

function Write-Slice([int]$endLine, [string]$dest) {
    $slice = $all[0..($endLine - 1)]
    [System.IO.File]::WriteAllLines((Join-Path (Get-Location) $dest), $slice, $utf8Enc)
}

function Do-Commit([string]$msg, [string[]]$paths) {
    git add -- @paths
    if ($LASTEXITCODE -ne 0) { throw "git add failed" }
    git commit -m $msg
    if ($LASTEXITCODE -ne 0) { throw "git commit failed" }
}

git restore crates/hermes-tools/.hermes-agent-ultra/logs/tool-policy-counters.json 2>$null

# PR1 (248 lines: helpers + A-1)
Write-Slice 248 "crates\hermes-agent\tests\run_agent_phase_a.rs"
Do-Commit "parity(run_agent): phase-a-1 contract new session on_session_start" @(
    "crates/hermes-agent/tests/run_agent_phase_a.rs"
)

# PR2 (+ A-2, 288 lines)
Write-Slice 288 "crates\hermes-agent\tests\run_agent_phase_a.rs"
Do-Commit "parity(run_agent): phase-a-2 contract continue session stored_system_prompt" @(
    "crates/hermes-agent/tests/run_agent_phase_a.rs"
)

# PR3 (+ A-3, 309 lines)
Write-Slice 309 "crates\hermes-agent\tests\run_agent_phase_a.rs"
Do-Commit "parity(run_agent): phase-a-3 contract budget caution at 70 percent" @(
    "crates/hermes-agent/tests/run_agent_phase_a.rs"
)

# PR4 (+ A-4, 327 lines)
Write-Slice 327 "crates\hermes-agent\tests\run_agent_phase_a.rs"
Do-Commit "parity(run_agent): phase-a-4 contract budget warning at 90 percent" @(
    "crates/hermes-agent/tests/run_agent_phase_a.rs"
)

# PR5 alignment A-5 only (base + a5, without a10)
Copy-Item "crates\hermes-agent\tests\alignment_pr5.rs" "crates\hermes-agent\tests\alignment_contracts.rs" -Force
Do-Commit "parity(run_agent): phase-a-5 contract strip budget plain text tail" @(
    "crates/hermes-agent/tests/alignment_contracts.rs"
)

# PR6 (+ A-7, 398 lines)
Write-Slice 398 "crates\hermes-agent\tests\run_agent_phase_a.rs"
Do-Commit "parity(run_agent): phase-a-7 contract empty llm retry without append" @(
    "crates/hermes-agent/tests/run_agent_phase_a.rs"
)

# PR7 (+ A-8, 501 lines + Cargo dev-dep)
Write-Slice 501 "crates\hermes-agent\tests\run_agent_phase_a.rs"
Do-Commit "parity(run_agent): phase-a-8 contract stream interrupt forwards deltas" @(
    "crates/hermes-agent/tests/run_agent_phase_a.rs",
    "crates/hermes-agent/Cargo.toml"
)

# PR8 hooks
Do-Commit "parity(run_agent): phase-a-9 contract pre post llm and tool hooks" @(
    "crates/hermes-agent/tests/run_agent_hooks.rs"
)

# PR9 (+ A-10 integration, 576 lines; alignment adds serialize test)
Write-Slice 576 "crates\hermes-agent\tests\run_agent_phase_a.rs"
$alignFull = Get-Content "crates\hermes-agent\tests\alignment_full.rs" -Encoding Unicode
[System.IO.File]::WriteAllLines((Join-Path (Get-Location) "crates\hermes-agent\tests\alignment_contracts.rs"), $alignFull, $utf8Enc)
Do-Commit "parity(run_agent): phase-a-10 contract agent result cost and interrupted" @(
    "crates/hermes-agent/tests/run_agent_phase_a.rs",
    "crates/hermes-agent/tests/alignment_contracts.rs"
)

# PR10 run_conversation stream
Do-Commit "parity(run_agent): phase-a-11 contract run_conversation stream callback" @(
    "crates/hermes-agent/tests/run_conversation_contracts.rs"
)

# PR11 (+ A-13, full file)
Write-Slice $all.Length "crates\hermes-agent\tests\run_agent_phase_a.rs"
Do-Commit "parity(run_agent): phase-a-13 contract steer pre api tool injection" @(
    "crates/hermes-agent/tests/run_agent_phase_a.rs"
)

# PR12 docs
Do-Commit "docs(run_agent): phase-a contract map in run_conversation sop" @(
    "docs/sop/run_conversation.md",
    "crates/hermes-agent/src/python_alignment.rs"
)

Write-Host "Created commits:"
git log --oneline -12
