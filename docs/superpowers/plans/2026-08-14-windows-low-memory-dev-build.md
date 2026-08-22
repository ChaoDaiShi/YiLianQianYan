# Windows Low-Memory Dev Build Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the Windows Tauri development client compile and start reliably under constrained available memory by serializing Cargo work and minimizing dev-profile compiler memory pressure.

**Architecture:** Keep the existing Tauri/Vite/backend startup chain unchanged. Apply the low-memory policy at the Cargo workspace boundary so direct Cargo commands and Tauri CLI child commands inherit the same limits without developer-specific environment setup.

**Tech Stack:** Cargo workspace configuration, Rust dev profiles, Python 3.11 `tomllib` configuration assertions, Tauri 2, Vite 5, PowerShell.

## Global Constraints

- Work on `fix/windows-low-memory-dev-build`, based on `develop@115126c` plus the approved design commit.
- Treat v0.3.1 as a maintenance milestone only; do not change package or Tauri version fields.
- Modify only `.cargo/config.toml` and the root `Cargo.toml` for the implementation.
- Preserve the pre-existing untracked `.claude/` directory and exclude it from every commit.
- Do not modify `backend/Cargo.toml`, `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`, `Cargo.lock`, Rust source, or frontend source.
- Do not run `cargo clean`, `cargo update`, delete `target`, or change Windows page-file settings.
- Use `npm.cmd` rather than `npm.ps1` on Windows.
- A successful `cargo check` is not desktop-startup acceptance; the Tauri window must actually open.
- If the low-memory build still reports OOM, stop changing code and collect the evidence defined in the approved design.

---

### Task 1: Serialize Cargo compilation

**Files:**
- Create: `.cargo/config.toml`
- Test: one-shot Python 3.11 TOML assertion executed from the repository root

**Interfaces:**
- Consumes: Cargo's repository-local configuration discovery from the workspace root.
- Produces: `[build].jobs = 1` for direct Cargo commands and Tauri CLI child Cargo commands.

- [ ] **Step 1: Run the failing configuration assertion**

Run:

```powershell
python -c "import pathlib,tomllib; p=pathlib.Path('.cargo/config.toml'); d=tomllib.loads(p.read_text(encoding='utf-8')); assert d['build']['jobs'] == 1"
```

Expected: FAIL with `FileNotFoundError` because `.cargo/config.toml` does not exist.

- [ ] **Step 2: Add the minimal repository Cargo configuration**

Create `.cargo/config.toml` with exactly:

```toml
[build]
jobs = 1
```

- [ ] **Step 3: Re-run the configuration assertion**

Run:

```powershell
python -c "import pathlib,tomllib; p=pathlib.Path('.cargo/config.toml'); d=tomllib.loads(p.read_text(encoding='utf-8')); assert d['build']['jobs'] == 1"
```

Expected: PASS with exit code `0` and no output.

- [ ] **Step 4: Commit the serialized build policy**

```powershell
git add .cargo/config.toml
git commit -m "fix(build): serialize Cargo compilation"
```

Expected: the commit contains only `.cargo/config.toml`.

---

### Task 2: Minimize Rust dev-profile memory pressure

**Files:**
- Modify: `Cargo.toml:5-9`
- Test: one-shot Python 3.11 TOML assertion executed from the repository root

**Interfaces:**
- Consumes: the workspace `[profile.dev]` inherited by `backend` and `src-tauri` packages.
- Produces: top-level dev settings `debug = 0`, `incremental = false`, and `codegen-units = 1`; dependency-package dev settings `debug = 0` and `codegen-units = 1`.

- [ ] **Step 1: Run the failing dev-profile assertion**

Run:

```powershell
python -c "import pathlib,tomllib; d=tomllib.loads(pathlib.Path('Cargo.toml').read_text(encoding='utf-8')); dev=d['profile']['dev']; deps=dev['package']['*']; assert dev['debug'] == 0; assert dev['incremental'] is False; assert dev['codegen-units'] == 1; assert deps['debug'] == 0; assert deps['codegen-units'] == 1"
```

Expected: FAIL with `AssertionError` because the current user change has `debug = "line-tables-only"` and does not define `incremental` or `codegen-units`.

- [ ] **Step 2: Apply the minimal dev profile**

Replace the existing profile block in the root `Cargo.toml` with:

```toml
[profile.dev]
debug = 0
incremental = false
codegen-units = 1

[profile.dev.package."*"]
debug = 0
codegen-units = 1
```

- [ ] **Step 3: Re-run the dev-profile assertion**

Run:

```powershell
python -c "import pathlib,tomllib; d=tomllib.loads(pathlib.Path('Cargo.toml').read_text(encoding='utf-8')); dev=d['profile']['dev']; deps=dev['package']['*']; assert dev['debug'] == 0; assert dev['incremental'] is False; assert dev['codegen-units'] == 1; assert deps['debug'] == 0; assert deps['codegen-units'] == 1"
```

Expected: PASS with exit code `0` and no output.

- [ ] **Step 4: Verify the implementation diff remains in scope**

Run:

```powershell
git diff -- Cargo.toml
git status --short
```

Expected: `Cargo.toml` contains only the approved profile keys; `.claude/` remains untracked and unstaged.

- [ ] **Step 5: Commit the dev profile**

```powershell
git add Cargo.toml
git commit -m "fix(build): reduce Rust dev memory pressure"
```

Expected: the commit contains only `Cargo.toml`.

---

### Task 3: Prove the build and desktop startup

**Files:**
- Modify: none
- Test: real Rust/Tauri build and runtime observation

**Interfaces:**
- Consumes: Tasks 1 and 2 configuration, current Rust 1.97.1 toolchain, existing frontend and backend startup chain.
- Produces: evidence that the main desktop path compiles, starts its backend, and creates a visible Tauri window without an allocation failure.

- [ ] **Step 1: Record pre-build memory headroom and toolchain**

Run:

```powershell
rustc --version --verbose
cargo --version
$samples = (Get-Counter '\Memory\Committed Bytes','\Memory\Commit Limit','\Memory\Available MBytes').CounterSamples
$samples | Select-Object Path,CookedValue | Format-Table -AutoSize
```

Expected: Rust and Cargo versions print successfully; memory counters provide committed bytes, commit limit, and available MB.

- [ ] **Step 2: Build the actual desktop package with one job**

Run:

```powershell
cargo build -p yi-lian-qian-yan -j 1
```

Expected: PASS with `Finished dev profile`; no `memory allocation failed`, LLVM OOM, or `.rmeta` mmap failure.

- [ ] **Step 3: Start the real Tauri development client**

Run in an interactive terminal:

```powershell
npm.cmd run tauri dev
```

Expected terminal evidence:

```text
VITE ... ready
Running DevCommand (`cargo run ...`)
Finished `dev` profile
Running `target\debug\yi-lian-qian-yan.exe`
```

Keep the command running for the next step.

- [ ] **Step 4: Verify backend health and the desktop window**

Run from a second PowerShell process:

```powershell
$health = Invoke-WebRequest -UseBasicParsing 'http://127.0.0.1:9420/api/health'
if ($health.StatusCode -ne 200) { throw "backend health returned $($health.StatusCode)" }
$desktop = Get-Process -Name 'yi-lian-qian-yan' -ErrorAction Stop | Where-Object { $_.MainWindowHandle -ne 0 }
if (-not $desktop) { throw 'Tauri process has no visible main window' }
$desktop | Select-Object Id,ProcessName,MainWindowTitle,MainWindowHandle
```

Expected: HTTP status `200` and at least one `yi-lian-qian-yan` process with a non-zero `MainWindowHandle`.

- [ ] **Step 5: Stop only the development session started in Step 3**

Press `Ctrl+C` in the Step 3 interactive terminal. Then run:

```powershell
Get-Process -Name 'yi-lian-qian-yan' -ErrorAction SilentlyContinue
```

Expected: no remaining desktop process from this validation session.

---

### Task 4: Run regressions and audit the delivery

**Files:**
- Modify: none
- Test: workspace and frontend regression suites plus Git scope audit

**Interfaces:**
- Consumes: the two implementation commits and existing repository test suites.
- Produces: evidence that the low-memory configuration did not change application behavior or include unrelated files.

- [ ] **Step 1: Run Rust formatting, compile checks, and tests**

Run:

```powershell
cargo fmt --check
cargo check --workspace
cargo test --workspace
```

Expected: all three commands exit `0`; the full Rust workspace test suite passes.

- [ ] **Step 2: Run frontend tests and the production build**

Run:

```powershell
Set-Location frontend
npm.cmd test
npm.cmd run build
Set-Location ..
```

Expected: Vitest reports all tests passing and Vite production build exits `0`.

- [ ] **Step 3: Check whitespace, commit scope, and branch state**

Run:

```powershell
git diff --check develop...HEAD
git diff --name-status develop...HEAD
git status --short --branch
```

Expected tracked changes relative to `develop`:

```text
A  .cargo/config.toml
M  Cargo.toml
A  docs/superpowers/specs/2026-08-14-windows-low-memory-dev-build-design.md
A  docs/superpowers/plans/2026-08-14-windows-low-memory-dev-build.md
```

The only tolerated unrelated worktree entry is the pre-existing untracked `.claude/` directory. `Cargo.lock`, Rust source, frontend source, and Tauri configuration must not appear.

- [ ] **Step 4: Report the verified result without pushing or merging**

Report modified and new files, behavior change, memory trade-off, exact validation commands and results, known environment limits, and the recommended next step. Do not push or merge unless the user explicitly requests it.
