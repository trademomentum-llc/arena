# Arena - Project Summary

| Field | Value |
|-------|-------|
| Project | Arena multi-agent review system + Dev Integration |
| Owner | Jason M Jarmacz, Trident Markets Group LLC |
| Last updated | 2026-06-05 |
| License | Proprietary |

## What Arena Is

Arena is a Rust CLI/TUI multi-agent "checks-and-balances" system for development. It creates
review **sessions** (code-review, implementation, validation, architecture, spec-drift),
dispatches each task to multiple worker LLM agents across provider adapters, and resolves via
either **human-in-loop** (human finalizes) or **council** (consensus agents, auto-approve at
confidence >= 0.9) modes. Sessions persist as JSONL; Prometheus metrics are exported on port 9092.

Arena is a network **client** (uses `reqwest` for outbound HTTP). It has no server framework.

## Architecture at a Glance

- `src/cli/` - command parsing and execution (create, run, list, view, finalize, cancel,
  drift-check, metrics, council-evaluate).
- `src/integration/` - orchestration engine, runner, config.
- `src/adapters/` - provider adapters (anthropic, openai, morphlex FFI, mock keyless adapter)
  behind a unified `AgentAdapter` trait.
- `src/arena`, `src/session`, `src/drift`, `src/metrics` - domain modules.
- `libs/libmorphlex.so` - built deterministic tokenizer (MX worker), ready to use.
- Worker short codes: AC, OC, GG, QQ, PP, XG, MK, ML, HD, MX (see ARENA_MANUAL.md).

## Current Initiative: Dev Integration (`arenax`)

Integrating arena into the local development environment as a first-class CLI tool, with a
**local-default** inference backend.

- **Shape:** host-native Go wrapper (`arenax`) + Git integration (Approach A). Devcontainer is
  phase 2.
- **Backend:** hybrid, local-default - everyday review/drift runs on a local open-weight coder
  model via Ollama at `localhost:11434/v1`, with API and council modes available on demand. Local
  mode is free, offline, private, and near-deterministic at temperature 0.
- **Default model:** Qwen2.5-Coder-7B-Instruct (7B dense, Apache-2.0), chosen to keep a 128K
  context window (native 32K extended via YaRN) on a 7B footprint - the workload is big-context /
  long-inference, and a 7B leaves enough memory for a long KV cache where a 16B model does not.
  Runs on an Apple-Silicon-optimal MLX (or Ollama/Metal) OpenAI-compatible runtime. 128K on 16 GB
  is feasible with q8_0/q4_0 KV-cache quantization (being verified). Fallbacks: DeepSeek-Coder
  6.7B-instruct (light, 16K) and DeepSeek-Coder-V2-Lite (16B, 128K).
- **Core delta:** add per-agent `base_url` (+ `api_key_env`) to the integration config and pass
  it through registration; add `--context @file`/stdin ingestion; add a loopback guard.
- **Hooks:** advisory and opt-in by default; optional blocking mode behind a config flag.

Specifications (Requirements, Design, Technical) live in
`docs/specs/dev-integration/`.

## Status

| Workstream | State |
|------------|-------|
| Arena core (review engine, adapters, TUI) | Built and runnable (`arena --help` works; no sessions yet). |
| MX deterministic worker | `libs/libmorphlex.so` present and built. |
| Dev Integration specs | Drafted (this triad); pending review. |
| Local-inference core delta | DONE (M1): base_url, @file/stdin, loopback guard, qwen-coder-local reg, xAI/Grok (XG) adapter support added. |
| `arenax` wrapper | M2 scaffolded (gitx/arena/review/drift/etc + core cmds + E2E plumbing verified). Full setup/doctor/hooks M3 next. |
| Version control | To be initialized. |

## Constraints and Doctrine

- Deterministic logic preferred; wrapper is Go. No emojis in any artifact the code reads.
- Three-document spec set precedes implementation.
- Multi-forge: Gitea, GitHub, Forgejo - detect forge from the Git remote.
- Target hardware: Apple M1 Pro, 16 GB shared with OS/editor; default model is a 7B/Q4 (~4.7 GB)
  whose small footprint leaves ~11 GB for the KV cache, enabling a 128K window with KV-cache
  quantization. Apple-Silicon runtimes (MLX/Metal) preferred.

## Pointers

- Manual: `ARENA_MANUAL.md`
- Coding standards: `design_specs/coding_standards.md`
- Specs: `docs/specs/dev-integration/{REQUIREMENTS,DESIGN_SPECIFICATION,TECHNICAL_SPECIFICATION}.md`
- Backlog: `TODO.md`
