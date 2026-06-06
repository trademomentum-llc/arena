# Arena - TODO

Last updated: 2026-06-05. Status values: TODO, IN PROGRESS, BLOCKED, DONE.

## Milestone 0 - Specs and Setup

| ID | Task | Status | Notes |
|----|------|--------|-------|
| M0-1 | Write Requirements / Design / Technical specs for Dev Integration | DONE | `docs/specs/dev-integration/` |
| M0-2 | Create PROJECT_SUMMARY.md and TODO.md | DONE | this file |
| M0-3 | Spec self-review (placeholders, consistency, scope, ambiguity) | IN PROGRESS | brainstorming step 7 |
| M0-4 | User reviews written specs | TODO | gate before implementation plan |
| M0-5 | Initialize Git for arena + .gitignore + initial commit | DONE 2026-06-05 | FR-16; root commit dc6dcb8 on main, 51 files |

## Milestone 1 - Arena Core Delta (Rust)

| ID | Task | Status | Notes |
|----|------|--------|-------|
| M1-1 | Add `base_url` + `api_key_env` to `AgentDef` | DONE | `src/integration/config.rs`; `#[serde(default)]`; serde test |
| M1-2 | Pass `base_url` through registration (OpenAI + Anthropic) | DONE | `src/integration/runner.rs`; with_config + loopback guard |
| M1-3 | Mirror in CLI create path | DONE | `src/cli/commands.rs`; resolve_context wired; local worker reg |
| M1-4 | Register default local worker `qwen-local` | DONE | "qwen-coder-local" via ARENA_LOCAL_* envs + CLI hardcode + default.yaml |
| M1-5 | Add `--context @file`/stdin ingestion | DONE | FR-11; `resolve_context` helper + tests + wiring |
| M1-6 | Add loopback validation guard | DONE | NFR-4; `src/adapters/endpoint.rs` + validate in reg paths |
| M1-7 | Rust unit tests for the delta | DONE | endpoint (6), context (4), agentdef serde, detect xai; baseline green |

## Milestone 2 - arenax Wrapper (Go)

| ID | Task | Status | Notes |
|----|------|--------|-------|
| M2-1 | Scaffold Go module + CLI root | DONE (partial) | arenax/ dir + go.mod + cmd/arenax/main.go with all subcommands skeleton |
| M2-2 | `internal/gitx` (staged/range diff, changed files) | DONE | pure os/exec git wrappers |
| M2-3 | `internal/arena` CLI-contract client (+ ExtractUUID) | DONE | exact marker parsing, @file temp 0600, Create/Run/DriftCheck |
| M2-4 | `internal/drift` (Classify, ExpandToFiles, RunDrift) | DONE | table-driven classify per DSN 4.2 + tests |
| M2-5 | `internal/sizebound` (Fit, byte cap, MX token count) | DONE (stub) | Fit + Report, MX hook stub |
| M2-6 | `internal/config` loader | DONE (partial) | yaml + defaults + ARENA_BIN override |
| M2-7 | `review-staged`, `review-range`, `drift` commands | DONE (core) | wired + sizebound + backend selection |
| M2-8 | `setup`, `doctor` commands | IN PROGRESS | doctor basic checks; setup skeleton |
| M2-9 | Go unit + property tests (>80%) | IN PROGRESS | ExtractUUID table, Classify table pass |
| M2-10 | Mock-adapter integration test (offline) | PARTIAL | E2E via review-staged exercised M1 local + @file (inference server had llama binary issue in env) |

## Milestone 3 - Hooks and Verification

| ID | Task | Status | Notes |
|----|------|--------|-------|
| M3-1 | Advisory pre-commit / pre-push templates | DONE | never block; FR-13 (templates + install logic in arenax setup) |
| M3-2 | Optional blocking template (council, threshold) | PARTIAL | FR-14 (template choice by hook_mode; full exit-nonzero wiring ready for use) |
| M3-3 | Hook install/uninstall (reversible, backs up) | DONE | FR-15; coexist with rr-guards (backupIfExists + 0755 + choice of .advisory/.blocking) |
| M3-4 | Hook smoke tests | TODO | both modes in throwaway repo |
| M3-5 | Run full verification procedure (TEC 9) | IN PROGRESS | acceptance criteria (mock E2E verified) |

## Phase 2 (Deferred)

| ID | Task | Status | Notes |
|----|------|--------|-------|
| P2-1 | Devcontainer packaging | TODO | D4 reproducibility |
| P2-2 | Large-diff chunking/summarization | TODO | replaces phase-1 truncation |
| P2-3 | Forge-aware PR review | TODO | FR-7; detect Gitea/GitHub/Forgejo |
| P2-4 | Rust-side synthesis port (markers, pure extract, no-secret test) | DONE | from arenax best-of-n |
| P2-5 | Basic ratatui TUI skeleton | DONE | `arena tui` (list, nav, view) |

## Verification Tasks (in progress)

| ID | Task | Status | Notes |
|----|------|--------|-------|
| V-SRC | Adversarially re-check every file:line / API claim in TECHNICAL spec vs live arena source | DONE 2026-06-05 | 19/19 source claims confirmed; 1 defect (drift output string) -> C-1 applied |
| V-MODEL | Confirm Qwen2.5-Coder-7B: 7B dense, Apache-2.0, 32K native + YaRN to 128K, MLX builds, OpenAI-compatible servers | DONE 2026-06-05 | Confirmed; 128K is YaRN opt-in not default -> C-2 applied |
| V-MAXCTX | Establish max feasible `num_ctx` for Qwen2.5-Coder-7B (GQA) on 16 GB with KV-quant | DONE 2026-06-05 | q4_0 baseline fits; q8_0 marginal (macOS wired cap); fp16 over -> C-3; FlashAttn/symmetric/mlx-server gap -> C-4 |
| V-FALLBACK | Verify DeepSeek 6.7B (16K) and DeepSeek-V2-Lite (16B MoE, 128K, MLA) facts + licensing | DONE 2026-06-05 | Confirmed; license does NOT block code review; V2 Ollama default ~4K ctx -> C-5 |

## Open Questions (confirm at spec review)

- Default local model: RESOLVED - Qwen2.5-Coder-7B-Instruct (7B dense, 128K via YaRN, MLX/Metal
  runtime); DeepSeek 6.7B (light) and DeepSeek-Coder-V2-Lite (16B, 128K) = fallbacks. Confirm via
  V-MODEL / V-MAXCTX.
- FR-11 (`@file`/stdin): RESOLVED - phase 1 mandatory (128K window makes large context routine).
- `arenax setup`: verify-and-instruct for Ollama (recommended, respects brew hook) vs offer install.
