# Arena Dev Integration - Requirements Specification

| Field | Value |
|-------|-------|
| Document | Requirements Specification (REQ) |
| Module | Arena Dev Integration (`arenax`) |
| Version | 1.0.0 |
| Date | 2026-06-05 |
| Author | Jason M Jarmacz, Trident Markets Group LLC |
| Status | Draft - pending review |
| Related | DESIGN_SPECIFICATION.md, TECHNICAL_SPECIFICATION.md |

---

## 1. Purpose and Scope

### 1.1 Purpose
This document specifies the requirements for integrating the existing **Arena** multi-agent
review system into a local development environment as a first-class developer tool. The
integration adds an ergonomics layer (`arenax`) around the compiled `arena` binary plus a
small, well-bounded set of changes to the arena core that enable **local, self-hosted model
inference** as the default backend.

### 1.2 Scope
In scope for this module (phase 1):

- A Go command-line wrapper, `arenax`, that composes Git repository state with `arena`
  invocations to deliver code review and spec-drift checks against working-tree changes.
- Minimal arena core changes to route inference to a local OpenAI-compatible endpoint
  (per-agent `base_url`), preserving existing API backends as on-demand escalation.
- Opt-in Git hooks (advisory by default) that invoke `arenax`.
- Bootstrap and environment-diagnostic commands.
- Version control initialization for the arena repository.

Explicitly **out of scope / deferred** (see Section 9):

- Devcontainer packaging (phase 2).
- A VS Code extension (deferred indefinitely unless inline-editor UX becomes a hard
  requirement).
- Any HTTP server or web portal layer for arena.

### 1.3 Locked design decisions (brainstorming outcomes)
These were resolved during brainstorming and are treated as fixed inputs:

| ID | Decision | Rationale |
|----|----------|-----------|
| D1 | Integration shape: host-native CLI + Git integration (Approach A) | Fastest to value, editor-agnostic, reuses arena's existing TUI, deterministic-language friendly. |
| D2 | Backend: hybrid, local-default | Local open-weight coder model as everyday workhorse; API and council modes available on demand. Removes per-call cost, network dependency, and keeps proprietary code on-device. |
| D3 | Wrapper language: Go | Per user doctrine (prefer deterministic languages); wrapper logic is pure and deterministic. |
| D4 | Devcontainer = phase 2 | Reproducibility upgrade layered after the host-native tool proves value. |

---

## 2. Background and Problem Statement

Arena is a Rust CLI/TUI multi-agent system that creates review sessions, dispatches a task to
multiple worker LLM agents across provider adapters, and resolves via human-in-loop or council
modes. It persists sessions as JSONL and exports Prometheus metrics. It is a network **client**
(uses `reqwest` for outbound HTTP); it has no server framework.

Today arena is invoked manually with verbose flags (`arena create --session-type ... --task ...
--workers "OC,AC" ...`), requires API keys for every run, and is not wired into the developer's
day-to-day Git workflow. The friction is twofold:

1. **Operational friction.** Constructing a review session from a code change is a multi-step,
   manual process (assemble the diff, choose workers, create, run, view, finalize).
2. **Backend friction.** Every review costs API tokens, depends on network availability, and
   sends source code off-device - a concern for proprietary work.

This module removes both frictions: `arenax` collapses the multi-step flow into single commands
driven by Git state, and the local-default backend makes everyday review free, offline, private,
and near-deterministic.

---

## 3. Stakeholders and Personas

| Persona | Description | Primary need |
|---------|-------------|--------------|
| Solo developer (primary) | Single user on an Apple Silicon workstation working across multiple repositories and forges (Gitea, GitHub, Forgejo). | One-command review of working-tree changes with no per-run cost or network dependency. |
| Repository maintainer | Same user, acting to enforce quality gates. | Optional automation (hooks) that surfaces issues without blocking flow by default. |

---

## 4. Definitions

| Term | Definition |
|------|------------|
| arena | The existing Rust multi-agent review binary. |
| `arenax` | The Go wrapper delivered by this module. |
| Worker | An LLM agent that produces a review response. |
| Council | Consensus agents that evaluate worker responses; auto-approve at confidence >= threshold (default 0.9). |
| Local-default | Routing inference to a loopback OpenAI-compatible endpoint (e.g. Ollama) by default. |
| MX worker | The Morphlex deterministic tokenizer worker, provided via `libs/libmorphlex.so` (already built). |
| Advisory hook | A Git hook that reports results but never blocks the commit/push. |
| Escalation | On-demand use of API models or council mode for high-stakes reviews. |

---

## 5. Functional Requirements

Priority uses MoSCoW (M = Must, S = Should, C = Could, W = Won't this phase).

### 5.1 Wrapper commands

| ID | Priority | Requirement |
|----|----------|-------------|
| FR-1 | M | `arenax review-staged` SHALL create and run a `code-review` session over the staged diff (`git diff --cached`) and report the session id and a result summary. |
| FR-2 | M | `arenax review-range <a>..<b>` SHALL create and run a `code-review` session over the diff for an arbitrary commit range. |
| FR-3 | M | `arenax drift` SHALL classify changed files into specifications and implementations and run `arena drift-check` over them. |
| FR-4 | M | `arenax setup` SHALL bootstrap the environment: build/locate the arena binary, place `arena` and `arenax` on PATH, install shell completions, write a `.env.example`, and (opt-in) install Git hooks. |
| FR-5 | M | `arenax doctor` SHALL verify the environment (arena binary present and runnable, local model endpoint reachable, `libmorphlex.so` present, required env vars) and report a deterministic pass/fail table. |
| FR-6 | S | All `arenax` commands SHALL accept a `--backend` selector (`local` default, `api`, `council`) to choose the inference path per invocation. |
| FR-7 | C | `arenax review-range` SHALL detect the active forge from the Git remote (Gitea / GitHub / Forgejo) for future PR-aware behavior. |

### 5.2 Backend and arena core

| ID | Priority | Requirement |
|----|----------|-------------|
| FR-8 | M | The arena core SHALL allow a per-agent `base_url` so an OpenAI-compatible agent can target a local endpoint (e.g. `http://localhost:11434/v1`). |
| FR-9 | M | The arena core SHALL register a default local worker agent when a local endpoint is configured and reachable. |
| FR-10 | M | Existing API backends (Anthropic, OpenAI, etc.) SHALL remain fully functional and selectable for escalation. |
| FR-11 | M | The arena core SHALL accept review context from a file reference (`@path`) or standard input, removing the single-string-argument size ceiling on `--context`. Mandatory in phase 1: the 128K-capable default model makes large context routine, so it must not transit argv. |
| FR-12 | C | `arenax` SHALL use the MX (Morphlex) worker to count tokens deterministically when enforcing context-size bounds. |

### 5.3 Git hooks

| ID | Priority | Requirement |
|----|----------|-------------|
| FR-13 | M | Git hooks SHALL be **advisory and opt-in by default**: they run `arenax`, print a summary and session id, and never block the commit or push. |
| FR-14 | S | A blocking mode SHALL be available behind an explicit configuration flag, blocking only on a council `reject` decision or council confidence below the auto-approve threshold. |
| FR-15 | M | Hook installation SHALL be reversible and SHALL NOT interfere with the existing `rr-verify-guard` / `rr-commit-guard` PreToolUse hooks. |

### 5.4 Documentation and project hygiene

| ID | Priority | Requirement |
|----|----------|-------------|
| FR-16 | M | The arena repository SHALL be placed under version control with an appropriate `.gitignore`. |
| FR-17 | M | `PROJECT_SUMMARY.md` and `TODO.md` SHALL be created and kept current. |
| FR-18 | M | This module SHALL ship Requirements, Design Specification, and Technical Specification documents (this triad). |

---

## 6. Non-Functional Requirements

| ID | Category | Requirement |
|----|----------|-------------|
| NFR-1 | Performance | A local review of a typical staged diff (<= 40 KB) SHALL complete in under 60 seconds on the target Apple M1 Pro with the default model (Qwen2.5-Coder-7B-Instruct). Capacity for large context up to the model window is a separate concern (see A2, DSN 5.3.1). |
| NFR-2 | Determinism | Wrapper logic (diff parsing, file classification, invocation construction, size bounding) SHALL be pure and deterministic; given identical inputs it SHALL produce identical arena invocations. |
| NFR-3 | Reproducibility | Local model inference SHALL run at temperature 0 by default to maximize verdict reproducibility. |
| NFR-4 | Privacy | In `local` backend mode, no source code SHALL leave the host; the configured endpoint SHALL be restricted to loopback unless a non-loopback host is explicitly opted in. |
| NFR-5 | Security | API keys SHALL be passed only via environment variables, never via process arguments (argv is world-readable via `ps`). Temporary files holding diffs SHALL be created with `0600` permissions and removed after use. |
| NFR-6 | Reliability | `arenax` SHALL fail loudly with a typed, actionable error on any precondition failure (missing binary, unreachable endpoint, empty diff); it SHALL NOT silently continue. |
| NFR-7 | Maintainability | The wrapper SHALL follow the existing arena coding standards (typed errors, structured logging, functions under 50 lines, table-driven tests, >80% coverage). |
| NFR-8 | Portability | The wrapper SHALL build to a single static binary and SHALL be editor-agnostic. |
| NFR-9 | Decoupling | The wrapper SHALL depend only on arena's CLI contract (subcommands, stdout markers, exit codes), not on arena internals. |

---

## 7. Constraints

| ID | Constraint | Source |
|----|------------|--------|
| C-1 | Arena has no server; integration is process-invocation + state-watching + result-surfacing only. | arena `Cargo.toml` (reqwest is client-only). |
| C-2 | `arena create --task` and `-x/--context` are plain `String` arguments; no stdin or file ingestion exists today. | `src/cli/commands.rs` create command. |
| C-3 | `arena drift-check` reads file contents itself, accepts comma-separated **file** paths (not directories), and prints a fixed-format result. | `src/cli/commands.rs`, `src/drift/detector.rs`. |
| C-4 | OS `ARG_MAX` bounds total argument size (macOS: 1,048,576 bytes). Large diffs passed as a single argument risk `E2BIG`. | POSIX `execve`. |
| C-5 | The integration AgentDef struct lacks `base_url`/`api_key_env` fields; these must be added to wire local inference. | `src/integration/config.rs`. |
| C-6 | The bundled `Qwen3.6/` directory is source only (LICENSE + README, no weights); a model runtime/weights must be acquired separately. | Repository inspection. |
| C-7 | Local target hardware is Apple M1 Pro, 16 GB unified memory shared with the OS and editor (~4-6 GB). Practical model ceiling is a ~16B-MoE/Q4 (~10 GB) or dense 7B (~5 GB) coder model, and the on-device context window must be capped to ~16K tokens regardless of the model's nominal maximum. | Host inspection; verification sweep 2026-06-05. |
| C-8 | No emojis in any artifact the code reads (source, config, specs). Documents must be UTF-8. | User doctrine. |
| C-9 | Bash hook policy: `brew install` only via the PreToolUse hook; runtime installation steps must respect this. | User environment. |

---

## 8. Assumptions

- A1: An OpenAI-compatible local runtime (Ollama recommended) will be installed and serving on
  `http://localhost:11434/v1`. Installation of the runtime is a setup step, not a build dependency.
- A2: The default local model is Qwen2.5-Coder-7B-Instruct (7B dense, Apache-2.0), Q4_K_M, at
  temperature 0. It is natively 32,768 tokens; a 128K window is reachable by enabling YaRN
  rope-scaling (factor 4.0) - opt-in, not out-of-the-box, and Qwen warns static YaRN can degrade
  short-context quality. A 128K window fits 16 GB with q4_0 KV-cache quantization as the no-tune
  baseline (q8_0 is marginal, requiring the macOS GPU wired-memory cap raised; fp16 does not fit) -
  see DSN 5.3.1. Runtime: an Apple-Silicon OpenAI-compatible server; Ollama/llama.cpp (Metal +
  Flash Attention) is the verified 128K path, MLX is the fastest path with an open KV-quant caveat
  on its server. Documented fallbacks: DeepSeek-Coder 6.7B-instruct (light, 16K) and
  DeepSeek-Coder-V2-Lite (16B MoE, 128K capability - not the Ollama default).
- A3: The user operates across multiple forges; forge-specific behavior is detected from the Git
  remote, not assumed.
- A4: The deterministic MX worker (`libs/libmorphlex.so`) is available for token counting and as
  a deterministic baseline worker.

---

## 9. Out of Scope and Deferred

| Item | Disposition | Reason |
|------|-------------|--------|
| Devcontainer packaging | Phase 2 | Reproducibility upgrade after host-native tool proves value (D4). |
| VS Code extension | Deferred | Most expensive shape; duplicates the existing TUI; conflicts with deterministic-language preference. |
| HTTP/web portal for arena | Excluded | Arena is a client; adding a server is a different project. |
| Multi-user / shared-server review | Excluded | Single-user scope (D1). |
| `arenax sessions`/list wrapper | Excluded | `arena list -a` and the TUI already cover it (YAGNI). |
| Metrics dashboard | Excluded | Arena already exposes Prometheus on `:9092` (YAGNI). |
| Large-diff chunking/summarization | Phase 2 | Phase 1 enforces a context-size bound instead (see Design Spec). |

---

## 10. Acceptance Criteria and Success Metrics

The module is accepted when all Must (M) requirements are demonstrably satisfied:

1. `arenax doctor` reports all-pass on a correctly configured host (AC for FR-5, FR-8, FR-9).
2. `arenax review-staged` on a repository with a staged change produces an arena session,
   runs it against the **local** backend with no network egress, and prints the session id and a
   summary (AC for FR-1, FR-8, NFR-4).
3. `arenax drift` correctly classifies a mixed set of changed files and produces a valid
   `arena drift-check` invocation (AC for FR-3).
4. An end-to-end create -> run -> finalize cycle passes offline using arena's keyless `mock`
   adapter in CI (AC for NFR-2, reliability).
5. Advisory hooks print results without blocking; blocking mode blocks only on the defined
   council conditions; neither interferes with existing PreToolUse guards (AC for FR-13..FR-15).
6. The arena repository is under version control; `PROJECT_SUMMARY.md` and `TODO.md` exist and
   are current (AC for FR-16, FR-17).

Success metrics:

- Median local review latency for a <= 96 KB diff: < 60 s (NFR-1).
- Per-review API cost in local mode: 0 (D2).
- Wrapper unit/property test coverage: > 80% (NFR-7).

---

## 11. Traceability

Each functional requirement maps forward to a design element and a technical change:

| Requirement | Design section | Technical artifact |
|-------------|----------------|--------------------|
| FR-1, FR-2 | DSN 4.1 (review pipeline) | `arenax/internal/review` |
| FR-3 | DSN 4.2 (drift pipeline) | `arenax/internal/drift` |
| FR-8, FR-9, FR-10 | DSN 5 (backend strategy) | arena `config.rs`, `runner.rs`, `commands.rs` |
| FR-11 | DSN 5.4 (context ingestion) | arena `commands.rs` create `--context @file`/stdin |
| FR-13, FR-14, FR-15 | DSN 6 (hook strategy) | `arenax/hooks/` templates |
| NFR-1..NFR-9 | DSN 7, 8, 9 | cross-cutting |

End of Requirements Specification.
