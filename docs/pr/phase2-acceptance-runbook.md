# Phase 2 acceptance runbook and gate predictions

**Status:** DeepSeek research-510 (read-only), 2026-09-23, against upstream `main` = `2105bfd`. Spot-checked by Claude: the no-baseline warnings (`phase2-acceptance.ps1:396-400`), the baseline requiring a signed visual review and three repetitions (`:488-492`), exit 1 unless exactly `accepted` (`:501`), and `phase2-visual-baseline.ps1:46` requiring schema 12. Everything about tonight's assets is prediction until measured.

**What it means for the plan:** a full campaign is two runs of about 5 h each and can only end `accepted` from a clean upstream commit with a signable visual review, which needs our held visual fixes merged upstream first. So the near-term runs are a single-repetition shakedown on `main` (to measure the bounds gate and the data) and GPU checks of the held branches; the real campaign waits for upstream merges.

## Headline

- **The first real run cannot end in `accepted`.** With no `acceptance-baseline.json` in the tree, the script records "no acceptance baseline was available" plus one warning per scenario (`scripts/phase2-acceptance.ps1:396`, `:400`), the verdict becomes `accepted-with-warnings`, and a non-quick campaign then **exits 1 by design** (`:501`). Non-zero from campaign #1 is expected, not a bug.
- **The baseline cannot be created without a signed visual review** (`:490` requires `$visualResult.passed`), and on `main` alone the review is not signable: the fork's own record says its shakedown "passed every gate except the visual review; the terrain fixes above are what the visual review needs" (`git show portal:AGENTS.md`, entry 2026-09-22), and RNG-004's evidence is top-down `rural.png` from that shakedown.
- **The one automated gate that can sink tomorrow's run on `main` is the runtime bounds gate** (`streaming.rs:942-944` → `transform_bounds_validation_failures`, a hard gate at `metrics.rs:351`). Upstream's converter deliberately writes *empty* glTF scenes for NIFs with no geometry block (`mesh.rs:87-101`), `integration.rs` then leaves `statics.bounds_valid = 0`, and the engine fails every reference to such a model. It also blocks the screenshot (`app.rs:1443-1456`), so a single hit costs 3 failures per run. Fix: `phase2/empty-model-bounds`. It is **predictable in minutes before spending the campaign** — step 3.3 below.
- Everything else is predicted green, on the strength of the fork's shakedown (all gates but the visual review, on equivalent assets) plus upstream's own fixtures.
- **`phase2-visual-baseline.ps1` is broken here**: it hard-codes `schema_version == 12` (`:46`) and throws on schema-14 assets. Do not use it tomorrow.

---

## 1. Runbook (PowerShell 7, from `C:\Modding\wkj-phase2`)

Session setup — once per window:

```powershell
cd C:\Modding\wkj-phase2
$PSVersionTable.PSVersion.ToString()      # must be 7.x
git rev-parse --short=12 HEAD             # must be 2105bfd...
git status --short                        # must print nothing
$Assets = 'C:\Modding\SkyrimConverted-upstream'
New-Item -ItemType Directory -Force target\preflight, C:\Modding\evidence | Out-Null
```

> The conversion must have been produced by the converter from `phase2/pruned-texture-complete`, never from `main` (`main`'s converter cannot produce `complete: true` on a stock install — 225 dangling URIs across 247 of 25,388 GLBs). Build it in a scratch worktree so this checkout stays on `main`: `git worktree add ..\wkj-p2-converter phase2/pruned-texture-complete`.

### Step 1 — asset contract (no campaign) — ~2 min

```powershell
$m = Get-Content "$Assets\conversion-manifest.json" -Raw | ConvertFrom-Json
$m.schema_version; $m.complete                      # expect 14, True
$i = Get-Content "$Assets\integration-report.json" -Raw | ConvertFrom-Json
$i.schema_version; $i.passed                        # expect 3, True
$i.missing_model_count; $i.invalid_model_count; $i.missing_texture_count   # expect 0,0,0
$i.terrain_cells; $i.cache_cells                    # expect equal
'target\release\engine.exe','skyrim_world.db','cell_cache.rkyv','vfs\meshes','vfs\textures','meshes','textures' |
  ForEach-Object { "$_ -> $(Test-Path (Join-Path $Assets $_))" }
```

Produces: nothing (terminal answers). **If `complete` is not `True`, stop** — preflight rejects the campaign before a frame is measured.

### Step 2 — release build of `main` — 15–40 min cold

```powershell
cargo build --workspace --release -j1
```

Produces `target\release\engine.exe`, `world-inspect.exe`, `asset-closure.exe`, `runtime-asset-audit.exe` (all four are needed: the script builds the same thing at `:194` and refuses without `engine.exe` at `:196`). `converter.exe` is already there from today.

### Step 3 — preflight tools + the bounds-gate predictor — 10–25 min

```powershell
.\target\release\asset-closure.exe $Assets target\preflight\asset-closure.json "$Assets\vfs\meshes" "$Assets\vfs\textures"
.\target\release\runtime-asset-audit.exe $Assets target\preflight\runtime-asset-audit.json

.\target\release\world-inspect.exe $Assets 0x3c 0  0   --radius 7 --output target\preflight\rural.json
.\target\release\world-inspect.exe $Assets 0x3c 4 -21  --radius 6 --output target\preflight\dense.json
.\target\release\world-inspect.exe $Assets 0x3c 21 -3  --radius 2 --output target\preflight\water.json
```

(Radii cover the flown footprint: the auto-flight is bounded to ±4 cells on the world **-Z** axis, i.e. +grid-Y, plus `stream_radius`.)

Step 3.3 — the prediction that decides tomorrow:

```powershell
foreach ($n in 'rural','dense','water') {
  $w = Get-Content "target\preflight\$n.json" -Raw | ConvertFrom-Json
  "== $n =="; $w.summary | Format-List references,references_without_model,missing_models,invalid_models,models_with_missing_textures
  @($w.references | Where-Object {
      -not $_.source_bounds -and $_.source_model -and
      (($_.source_model -replace '\\','/').ToLowerInvariant() -notmatch '^(meshes/)?(sky|markers|effects)/') -and
      ((($_.source_model -replace '\\','/') -split '/')[-1].ToLowerInvariant() -notmatch 'marker')
    }) | Group-Object source_model | Sort-Object Count -Descending | Select-Object Count,Name
}
```

Expect empty. **Any row here is a model the engine will spawn and fail to bound** (the filter is the engine's own exclusion rule, `streaming.rs:1285-1293`), meaning every world scenario that loads that cell fails its bounds gate, its screenshot, and logs an `ERROR engine…` line.

### Step 4 — plumbing check, no assets, no gates — 1–3 min

```powershell
.\scripts\phase2-acceptance.ps1 -Quick -SkipQualityGates -SkipRobustness -SkipBuild -OutputRoot target\acceptance-plumbing
```

Produces a synthetic-only campaign dir; exit 0. Confirms pwsh/cargo/git/window/HZB path before touching real data.

### Step 5 — single-repetition real-asset shakedown — 8–20 min

```powershell
.\scripts\phase2-acceptance.ps1 -Quick -SkipQualityGates -SkipRobustness -SkipBuild `
  -Assets $Assets -RuralGridX 0 -RuralGridY 0 -DenseGridX 4 -DenseGridY -21 -WaterGridX 21 -WaterGridY -3 `
  -OutputRoot target\acceptance-shakedown
```

`-Quick` forces 1 repetition with 3–5 s durations but still runs all 11 scenarios, the real preflight (closure + audit), and captures screenshots. Produces `target\acceptance-shakedown\<stamp>-<hardware>\` with `preflight.json`, `functional-results.json`, `screenshots.json`, `acceptance-report.json`, `acceptance-summary.md`, `profiling\<scenario>\run-1\`, `screenshots\*.png`. Exit 0 unless rejected.

Read it:

```powershell
$c = (Get-ChildItem target\acceptance-shakedown -Directory | Sort-Object Name | Select-Object -Last 1).FullName
Get-Content "$c\acceptance-summary.md"
Get-ChildItem "$c\profiling" -Recurse -Filter acceptance.json | ForEach-Object {
  $j = Get-Content $_.FullName -Raw | ConvertFrom-Json
  '{0,-16} passed={1,-6} bounds={2} terrain={3} assets={4} fps={5:N1}' -f $_.Directory.Parent.Name,$j.passed,
    $j.streaming.transform_bounds_validation_failures,$j.streaming.terrain_validation_failures,
    $j.streaming.asset_load_failures,$j.average_fps }
```

### Step 6 — human review of the shakedown screenshots — 20–40 min

Fill in `C:\Modding\evidence\visual-review.json` from `$c\screenshots\*.png` (section 3). Only honest if **nothing in the renderer changes between this shakedown and campaign #1**.

### Step 7 — full campaign #1 (baseline creator) — 4.5–6.5 h

```powershell
.\scripts\phase2-acceptance.ps1 -Assets $Assets `
  -RuralGridX 0 -RuralGridY 0 -DenseGridX 4 -DenseGridY -21 -WaterGridX 21 -WaterGridY -3 `
  -Repetitions 3 -VisualReview C:\Modding\evidence\visual-review.json -RequireVisualSignoff -UpdateBaseline
```

Full quality gates, robustness, 3 repetitions at 120/300/600/1800 s. Produces `target\acceptance\<stamp>-<hardware>\` and writes `acceptance-baseline.json` at the repo root. Expect `accepted-with-warnings` + exit 1; the baseline is written anyway (`:488-497`). Leave the baseline **untracked** (untracked files do not mark the tree dirty for this script, `:124`).

### Step 8 — full campaign #2 (the comparable run) — 4.5–6.5 h

```powershell
.\scripts\phase2-acceptance.ps1 -Assets $Assets `
  -RuralGridX 0 -RuralGridY 0 -DenseGridX 4 -DenseGridY -21 -WaterGridX 21 -WaterGridY -3 `
  -Repetitions 3 -VisualReview C:\Modding\evidence\visual-review.json -RequireVisualSignoff
```

Same commit, clean tree, same baseline. Expect `accepted` (exit 0) if section 2 holds. Any regression >5 % warns, >10 % fails.

### Step 9 — validate and close — ~5 min

```powershell
$campaign = (Get-ChildItem target\acceptance -Directory | Sort-Object Name | Select-Object -Last 1).FullName
.\scripts\phase2-close.ps1 -Campaign $campaign -ValidateOnly
$url = Read-Host 'Retained bundle URL (absolute https)'
.\scripts\phase2-close.ps1 -Campaign $campaign -BundleUri $url
git add -A; git commit        # close edits README.md, 02-core-engine.md, 02-completion-plan.md, writes 02-release-evidence.md
```

`phase2-close` demands the campaign commit **== current HEAD** and a clean tree (`:47-51`), zero warnings, all 11 screenshots hash-matching, and an HTTPS `-BundleUri`. Without a bundle host you can only `-ValidateOnly`; the roadmap then cannot be marked complete.

### Optional — profiling campaign — 3.5–4.5 h

```powershell
.\scripts\phase2-profile.ps1 -Scenario all -Assets $Assets `
  -RuralGridX 0 -RuralGridY 0 -DenseGridX 4 -DenseGridY -21 -WaterGridX 21 -WaterGridY -3 -Repetitions 3
```

Duplicates evidence campaign #1/#2 already collect (`--profile-output` bundles), for a separate `profiling-baseline.json`. Skip unless the roadmap explicitly wants it.

**Time**: campaign #1 ≈ 3 h 21 min of measured scenario time (33 runs) + 30–60 min process/asset-load overhead + 1.5–3 h quality/robustness/release build on `-j1` ≈ **5–6.5 h**; the CI job allows 360 min, so run it locally, not through the workflow.

---

## 2. Gate table

Legend for the last column: which local branch changes the prediction.

### Preflight (`Add-Preflight`)

| Gate | Checks | Predicted on `main` + tonight's assets | Branch |
|---|---|---|---|
| `operating-system` | `$IsWindows` (`:146`) | pass | — |
| `powershell-version` | `$PSVersionTable.PSVersion.Major -ge 7` (`:147`) | pass if `pwsh` 7; **unverified** (I could not run pwsh). Under 5.1 the run dies later at `-AsHashtable` (`:381`), not here | — |
| `cargo`, `git` | on PATH (`:148-149`) | pass | — |
| `disk-space` | ≥10 GiB free **on the repo drive root** (`:144`, `:150`) | pass (check C: by hand; says nothing about the assets drive) | — |
| `worktree` | clean, recording only (`:151`) | pass (warning if dirty) | — |
| `assets-directory` | `-Assets` resolves (`:155`) | pass | — |
| `representative-world-coordinates` | 3 distinct pairs (`:158-161`) | **fails if the coordinates are omitted** — defaults are all `0,0` | — |
| `asset-conversion-manifest.json`, `asset-integration-report.json`, `asset-skyrim_world.db`, `asset-cell_cache.rkyv` | present (`:162-165`) | pass | — |
| `converter-schema` | `schema_version == 14` (`:168`) | pass with tonight's set; **fails** if the set was converted by `material-publication` (schema 15) | `phase2/material-publication` (bad direction) |
| `conversion-complete` | `manifest.complete` (`:169`) | pass **only because** tonight's conversion uses the prune fix; upstream's converter cannot be complete on a stock install | `phase2/pruned-texture-complete` (already applied to the assets) |
| `database-schema` | `== 3` (`:173`) | pass | — |
| `integration-report-passed` | `integration.passed` (`:174`) | pass expected (pruned URIs are no longer failures → `missing_texture_count 0`; `cache_cells == terrain_cells`). **Read the JSON first** — a single missing model/invalid model/missing texture fails the whole campaign | `phase2/pruned-texture-complete` |
| `asset-closure-tool`, `runtime-asset-audit-tool` | binaries exist (`:205-206`) | pass after step 2 | — |
| `asset-closure` | tool exit 0 over DB-reachable `STAT/MSTT/FURN` (`:208-213`) | pass expected. Note it **tolerates** non-renderable models (`asset-closure.rs:199-210`, `:242-248`) — so it does *not* predict the runtime bounds gate | — |
| `runtime-glb-ktx2-audit` | every published GLB + KTX2 parses (`:216-217`) | pass expected | — |

### Quality + release (`Invoke-RecordedCommand` names)

| Gate | Checks | Predicted | Branch |
|---|---|---|---|
| `format` | `cargo fmt --all -- --check` (`:188`) | pass | — |
| `tests` | `cargo test --workspace --all-targets -j1` (`:189`) | pass expected; Windows/`no --all-features` is a combination CI never runs (CI = Linux nextest, `--all-features`) | — |
| `clippy` | `-- -D warnings` (`:190`) | pass | — |
| `release-build` | `cargo build --workspace --release -j1` (`:194`) | pass (all four binaries needed downstream) | — |
| engine executable | `target\release\engine.exe` exists (`:196`) | pass | — |

### Robustness (`:225-246`)

| Gate | Checks | Predicted | Branch |
|---|---|---|---|
| 14 named tests (`app-tests-rejects_stale_or_incomplete_runtime_assets` … `profiling-tests-writes_complete_profile_bundle`, names mangled `::`→`-` at `:241`) | each `cargo test -p engine <name> -j1`, exit 0 | pass — all 14 exist on `main` (verified) | — |
| `runtime-asset-audit-negative` | `cargo test -p converter --bin runtime-asset-audit -j1` (`:243`) | pass | — |
| `runtime-rejects-missing-assets` | engine against a non-existent assets dir must exit **non-zero** (`:245-246`) | pass (`validate_runtime_assets`, `app.rs:1179-1186`) | — |

### Functional / performance — one entry per run in `functional-results.json` (`:307-332`)

| Gate | Checks | Predicted | Branch |
|---|---|---|---|
| `materials` | canonical fixture, `canonical_fixture_validated` (`metrics.rs:358`) | pass | — |
| `terrain-water` | fixture, 4 quadrants + water material (`metrics.rs:359`) | pass | — |
| `transform-bounds` | fixture hierarchy + 8-corner bounds (`metrics.rs:360`) | pass (synthetic; the real-world equivalent is the risky one) | — |
| `renderer` | HZB / indirect path active, fixture validated (`metrics.rs:184-185`) | pass | — |
| `streaming` | fixture DB/cache in `target\test-temp`, lifecycle invariants (`:177-182`) | pass | — |
| `synthetic` | 250k instances, ≥60 FPS, P95 ≤16.67 ms | pass — release measured 122.78 FPS / P95 10.12 ms (`docs/roadmap/02-visual-fidelity-implementation-plan.md`) | — |
| `rural`, `dense`, `water`, `stress`, `stability` | engine `passed` (`metrics.rs:190-197`) incl. `terrain_validation_failures == 0` and `transform_bounds_validation_failures == 0` (`metrics.rs:345-351`) | **fail if step 3.3 finds any spawnable unbounded model** (`streaming.rs:942-944`): report `passed=false`, engine exit ≠ 0, screenshot never captured (`app.rs:1443-1456`), plus `ERROR engine` log line → 3 failures each. If step 3.3 is clean: pass predicted, incl. terrain seams (the only vanilla seams the fork found are at Tamriel (18,18)/(18,20), outside the flown footprint) | `phase2/empty-model-bounds`; `phase2/terrain-seam-weld` only if the coordinates move to those cells |
| `rural`/`dense` etc., terrain gate specifically | 0.01-unit edge continuity vs resident neighbours (`streaming.rs:1592-1601`) | pass at (0,0), (4,-21), (21,-3) | `phase2/terrain-seam-weld` |

### Aggregate gates

| Gate | Checks | Predicted | Branch |
|---|---|---|---|
| scenario completeness | every expected scenario present, exactly `Repetitions` runs (`:350-360`) | pass | — |
| screenshots | `screenshots.json` present for every scenario, hash recorded (`:361-374`) | **fails for every world scenario that trips the bounds gate** | `phase2/empty-model-bounds` |
| baseline | file exists, `hardware` string equal (`:375-397`) | **first run: no baseline → warning (`:396`) + one warning per scenario (`:400`) → never `accepted`** | — |
| regression comparison | >5 % warn, >10 % fail with noise floors (`:78-101`, `:398-417`) | second campaign only | — |
| visual sign-off | 11 checkpoints at `pass` + reviewer/timestamp/signature (`:438-449`) | **cannot honestly be signed on `main`**: top-down screenshots show edge-clamped/blocked layer blending, flat lighting (no cascades), alpha-tested foliage as dots, wrong reference rotations | `terrain-samplers-weights`, `vertex-alpha-not-opacity`, `sun-shadow-cascades`, `refr-rotation`, `material-publication` |
| runtime log clean | no panic/`ERROR engine`/strict-validation line (`:286-294`) | follows the scenario result | — |
| verdict / exit | `rejected` / `accepted-with-warnings` / `accepted` (`:460`), exit 1 unless `-Quick` or exactly `accepted` (`:501`) | campaign #1 `accepted-with-warnings` (exit 1); campaign #2 `accepted` only if step 3.3 was clean **and** the review was signed | — |

`phase2/shared-converter-schema` changes no prediction (pure refactor, still 14).

---

## 3. `visual-review.json`

The script writes `visual-review-template.json` (`:420-436`, format 2) into the campaign dir; the file you pass to `-VisualReview` is copied in as `visual-review.json` (`:446`) and must contain:

```json
{ "format_version": 2,
  "reviewer": "<name>", "reviewed_at": "<ISO-8601>", "signature": "<name> — approved",
  "checkpoints": [ { "scenario": "materials", "status": "pass", "notes": "…" }, … ] }
```

- **All 11** scenarios need a checkpoint, each with `status: "pass"` exactly once — `materials`, `terrain-water`, `transform-bounds`, `renderer`, `streaming`, `synthetic`, `rural`, `dense`, `water`, `stress`, `stability` (`:442-445`; Marco 7 says "rural, dense, water and stress" — **the script wins, all 11**). Empty reviewer/`reviewed_at`/signature fails.
- `phase2-close.ps1` re-checks the same file (`:66-81`) against `screenshots.json`: each snapshot must exist at its recorded path and **still hash to the recorded SHA-256**.
- Screenshots are the five world ones as **top-down views 20,000 units above the cell centre** (`app.rs:1262-1268` when `--acceptance-screenshot` is set) — judge terrain seams, layer blending, water plane, and object placement there. The six fixture screenshots are close-ups (metres) — judge material class/cutout/blend/emissive, the four terrain quadrants + flow normal, hierarchy/bounds, occlusion and indirect behaviour, streaming teleports. On `main` expect to *refuse* sign-off for: blocky/edge-clamped terrain layers, no sun shadows, foliage as scattered dots, and mis-rotated references.

---

## 4. Linux/CI and machine-specific traps

1. **`phase2-visual-baseline.ps1:46` throws on tonight's assets** — it requires manifest schema **12**. Dead script until patched; not needed for the campaign.
2. **PowerShell 7 is mandatory** but only *recorded* as a gate (`:147`). Under 5.1, `ConvertFrom-Json -AsHashtable` (`:381`) and `[Convert]::ToHexString` (`phase2-close.ps1:131`) fail hard. `$IsWindows` (`:146`) is `$null` under 5.1.
3. **Quality gates here are not what CI runs.** CI (`.github/workflows/run_tests.yml`) is ubuntu-latest nextest with `--all-features`; the script runs Windows `cargo test --workspace --all-targets -j1` without `--all-features`. Never exercised in CI; `-j1` makes the build the slowest part of the night.
4. **Hardware identity is fragile**: derived from the *first* `Win32_VideoController` (`:49-57`), e.g. `AMD-Ryzen-7-5800X-8-Core-Processor-NVIDIA-GeForce-RTX-3090-driver-32.0.16.1692` (the fork's shakedown path). An iGPU enumerating first, or a **GPU driver update between campaigns**, changes the string → `same_hardware` false (`:385`) → no `accepted`, and `phase2-close` refuses (`:36`). Do not update drivers mid-sequence.
5. **The log filter is tuned for this machine's ReShade** — it whitelists `ReShade64.dll` and `wgpu_hal::vulkan::instance` only (`:287-289`). Any *other* injected-DLL line, or a ReShade overlay altering the visible window, fails runs and corrupts screenshots. Disable ReShade/overlays for the campaign.
6. **A real window is required** — `--headless` is never passed; a disconnected RDP/headless session can stall presentation. Run at the console, nothing else on the GPU, High-performance power plan.
7. `:34-36` redirects `TEMP`/`TMP` to `target\test-temp` (streaming fixture DB goes to the repo drive), while the `disk-space` gate (`:144-150`) only measures the repo drive root — the assets drive is unchecked.
8. `git diff --quiet HEAD` (`:124`, and `phase2-close.ps1:50`) ignores **untracked** files — the untracked root `acceptance-baseline.json` is fine; a *tracked* change is not.
9. `phase2-close.ps1` needs an absolute HTTPS `-BundleUri` (`:139-142`) and rewrites README/roadmap docs; it will leave the tree dirty.
10. `world-inspect.exe` uses an **older, unfiltered** `converted_model_path` (`world-inspect.rs:694-705`) with no markers/sky/effects exclusion, so it over-reports spawnable models — use the filter from step 3.3, not raw counts.

---

## 5. What I could not determine

1. **Anything about tonight's actual conversion** — `C:\Modding\SkyrimConverted-upstream` does not exist yet, and reads outside `C:\Modding\wkj-phase2` / the Skyrim install are blocked by policy. So `conversion-complete`, `integration-report-passed`, `asset-closure`, and above all the **bounds gate population are inferences from upstream code plus the fork's measurements, not measurements of this data**. Step 3.3 resolves the decisive one.
2. **The machine's PowerShell version, cargo/git presence, free disk, and GPU identity** — could not execute `pwsh`/`Get-CimInstance` (command denied). Hardware above is quoted from the fork's evidence path, not verified.
3. **`cargo test --workspace --all-targets -j1` and clippy on Windows** — never run in this combination (CI is Linux); the fork ran the equivalent gates on its superset tree, which is evidence but not proof for `2105bfd`.
4. **Whether any other LAND seam exists beyond (18,18)/(18,20)** in the tested footprint — the fork measured two; the audit explicitly lists the per-cell seam distribution as unmeasured.
5. **Per-run startup overhead** — my 30–60 min per campaign and 8–20 min shakedown estimates are inferred, not measured.
6. **Whether the fork's "visual review needs the terrain fixes" verdict still holds** at the acceptance coordinates — research-004's evidence is the shakedown's top-down `rural.png`, which I could not open (it lives in the fork's `local/`, outside the readable set).

