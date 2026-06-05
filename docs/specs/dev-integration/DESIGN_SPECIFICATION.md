# Arena Dev Integration - Design Specification

| Field | Value |
|-------|-------|
| Document | Design Specification (DSN) |
| Module | Arena Dev Integration (`arenax`) |
| Version | 1.0.0 |
| Date | 2026-06-05 |
| Author | Jason M Jarmacz, Trident Markets Group LLC |
| Status | Draft - pending review |
| Related | REQUIREMENTS.md, TECHNICAL_SPECIFICATION.md |

---

## 1. Design Goals

1. **Compose, do not modify (mostly).** Treat the compiled `arena` binary as a stable interface;
   the wrapper depends only on arena's CLI contract. The sole arena-core changes are the minimal
   set required to enable local inference and unbounded context ingestion (FR-8..FR-11).
2. **Keep the determinism boundary crisp.** All wrapper logic is pure and deterministic; the only
   non-deterministic element (LLM output) stays inside arena, and is itself pushed toward
   determinism via temperature 0 and the MX worker.
3. **Local-first, escalate on demand.** Free, offline, private review is the default path; API
   and council modes are explicit opt-ins for high-stakes work.
4. **Do no harm.** A broken wrapper or a down model endpoint must never corrupt arena state or
   block the developer's Git flow by default.

---

## 2. Architecture Overview

The integration is an ergonomics layer wrapped around arena, plus a thin local-inference seam
inside arena. Three primitives are all that arena's client architecture permits, and every
component is built from them: **invoke** the binary, **watch** its state (JSONL sessions,
Prometheus `:9092`), and **surface** results (terminal / TUI / Git).

```
        Developer workstation (Apple M1 Pro, single user)
  +-------------------------------------------------------------+
  |  Git working tree                                           |
  |     | git diff --cached / range / --name-only               |
  |     v                                                        |
  |  arenax (Go, deterministic)                                 |
  |   - review-staged / review-range / drift / setup / doctor   |
  |   - pure functions: diff scope, file classify, size bound,  |
  |     argv build, stdout/exit-code parse                      |
  |     | os/exec (argv only; secrets via env)                  |
  |     v                                                        |
  |  arena (Rust CLI/TUI)  --- writes --->  arena-sessions/*.jsonl
  |   - create / run / view / finalize / drift-check / council  |
  |   - AgentRegistry -> adapters                               |
  |        |                         |                          |
  |        | base_url=loopback       | API key (env)            |
  |        v                         v                          |
  |  Local OpenAI-compatible     Anthropic / OpenAI / ...       |
  |  endpoint (Ollama :11434)    (escalation only)              |
  |        ^                                                     |
  |        |  MX worker (libmorphlex.so) - deterministic        |
  |        |  baseline + token counter                          |
  |  Prometheus metrics on :9092  (existing, unchanged)         |
  +-------------------------------------------------------------+
```

Boundary contract (NFR-9): `arenax` knows only that `arena create` prints
`Session created: <UUID>`, that subcommands exit 0 on success and non-zero on failure, and that
`drift-check` prints a fixed-format findings block. It does not link against or read arena's
internal modules.

---

## 3. Component Inventory

| Component | Type | Responsibility | Depends on |
|-----------|------|----------------|------------|
| `arenax` CLI root | Go binary | Argument parsing, subcommand dispatch, structured logging. | `arena` binary on PATH. |
| `internal/gitx` | Go pkg | Pure Git queries: staged diff, range diff, changed file list. Wraps `git` read-only commands. | `git`. |
| `internal/review` | Go pkg | Build and run a `code-review` session from a diff; parse session id; render summary. | `internal/arena`, `internal/gitx`. |
| `internal/drift` | Go pkg | Classify changed files into specs vs impls; expand directories to files; build `drift-check` invocation. | `internal/gitx`, `internal/arena`. |
| `internal/arena` | Go pkg | Typed wrapper over the arena CLI contract: invoke, capture stdout/exit, extract UUID. Secrets via env. | `arena` binary. |
| `internal/sizebound` | Go pkg | Deterministic context-size guard (byte cap; optional MX token count). | optional MX via arena. |
| `internal/config` | Go pkg | Load `arenax` config (backend selection, endpoint, caps, hook mode). | filesystem. |
| `hooks/` | templates | Advisory (default) and blocking (opt-in) pre-commit / pre-push templates that call `arenax`. | `arenax`. |
| arena core delta | Rust | Per-agent `base_url`, local agent registration, `--context @file`/stdin ingestion. | existing adapters. |

Each unit answers the three isolation questions: what it does (above), how it is used (a single
exported entry function per package - see Technical Spec), and what it depends on (right column).

---

## 4. Data Flow

### 4.1 Review pipeline (FR-1, FR-2)

```
review-staged:
  1. gitx.StagedDiff()         -> diff text (deterministic)
  2. sizebound.Fit(diff)       -> bounded context (cap or MX-counted)   [NFR-1, FR-12]
  3. arena.Create(             -> "Session created: <UUID>"
        type=code-review,
        task=<derived title>,
        context=<bounded diff>,         (passed via @file when FR-11 lands)
        workers=<backend selection>)    [FR-6, D2]
  4. arena.Run(session=<UUID>) -> worker responses (local inference)
  5. review.Summarize(stdout)  -> terminal summary + session id
  (human finalizes later via `arena finalize`, or via the TUI)
```

`review-range <a>..<b>` is identical except step 1 is `gitx.RangeDiff(a, b)`.

### 4.2 Drift pipeline (FR-3)

```
drift:
  1. gitx.ChangedFiles()        -> []path (working tree vs HEAD)
  2. drift.Classify(paths)      -> (specs[], impls[])  deterministic ruleset
  3. drift.ExpandToFiles(...)   -> only regular files (arena needs files, not dirs)  [C-3]
  4. arena.DriftCheck(specs, impls, agent=<backend>)
  5. render findings block + exit code
```

Classification ruleset (deterministic, table-driven):

| Predicate | Class |
|-----------|-------|
| path under `docs/`, `spec/`, `specs/`, or extension in {`.md`,`.yaml`,`.yml`,`.json` under a specs dir} | spec |
| path under `src/`, `lib/`, `internal/`, or a recognized source extension | impl |
| otherwise | ignored (logged) |

### 4.3 Failure flow (NFR-6)
Any precondition failure (no `arena` on PATH, endpoint unreachable in `local` mode, empty diff,
classification yields no impls) raises a typed error, is logged at `error`, and exits non-zero
with an actionable message. No partial arena session is created on a precondition failure.

---

## 5. Backend Strategy (FR-8..FR-12, D2)

### 5.1 The seam already exists
The OpenAI adapter already supports a configurable endpoint:
`OpenAIAdapter::with_config(api_key, model, base_url: Option<String>, timeout_ms, max_retries)`
builds requests against `base_url` for both `/chat/completions` and the health check. The only
gap is that the **integration config and registration path do not pass a `base_url` through** -
they call `::new()` (which hardcodes `https://api.openai.com/v1`). Closing that gap is the core
of FR-8.

### 5.2 Three selectable backends (FR-6)

| `--backend` | Workers used | Network | Cost | Determinism |
|-------------|--------------|---------|------|-------------|
| `local` (default) | local coder model (Ollama) + optional MX | none (loopback) | 0 | high (temp 0) |
| `api` | hosted frontier models (AC/OC/...) | egress | metered | lower |
| `council` | workers + council agents, consensus | egress (unless local council) | metered | mixed |

### 5.3 Local model selection (A2, C-7)

Decision (user-directed): the target is a **7B dense model that retains a 128K context window**, on
an **Apple-Silicon-optimal runtime (MLX or Metal)**, free and self-hostable. The model meeting all
of these is **Qwen2.5-Coder-7B-Instruct** (Apache-2.0): 7B dense, native 32K window **extended to
128K via YaRN rope-scaling**, ~4.7 GB resident at Q4_K_M. The small weight footprint is the enabler
- it leaves ~11 GB for the KV cache, which is what makes a 128K live window affordable on 16 GB (see
5.3.1). A 16B model cannot do this: its weights alone leave no room for a long KV cache. The pick is
confirmed by verification task V-MODEL (Section 12) before lock-in.

Runtime (Apple-Silicon-optimal): the local endpoint is any OpenAI-compatible server, chosen for
Metal/MLX acceleration. Because arena targets a configurable `base_url`, the runtime is swappable
with no code change.

| Runtime | Acceleration | Endpoint (example) | Note |
|---------|--------------|--------------------|------|
| Ollama / llama.cpp | llama.cpp Metal | `http://localhost:11434/v1` | **Default for the 128K path** - verified to support quantized KV cache (Flash Attention + symmetric q4_0), which is required to fit 128K on 16 GB (5.3.1). |
| MLX (`mlx_lm.server` or LM Studio MLX backend) | MLX (Apple unified-memory native) | `http://localhost:1234/v1` | Fastest Apple-Silicon path; mlx-community ships Qwen2.5-Coder-7B MLX quants. Caveat: `mlx_lm.server` lacked KV-quant (`--kv-bits`) as of Mar 2026 (mlx-lm #1043) - re-confirm before using for 128K; `mlx_lm.generate` and LM Studio are unaffected. |

Fallbacks (documented, user-approved, selectable via `--backend`/config): **DeepSeek-Coder
6.7B-instruct** (~4.8 GB, 16K window - very light and fast when 128K is not needed) and
**DeepSeek-Coder-V2-Lite** (16B MoE, 128K via MLA compressed KV, higher quality but tighter KV
headroom). Qwen2.5-Coder is also retained as a self-evident free option. Models that exceed 16 GB
are not candidates.

#### 5.3.1 128K on 16 GB is feasible with KV-cache quantization (verification task V-MAXCTX)
At long context the binding cost is the KV cache, not the weights. For Qwen2.5-Coder-7B (GQA: 28
layers, 4 KV heads, head_dim 128), KV memory per token at fp16 is approximately
`2 (K+V) * 28 layers * 4 KV-heads * 128 head_dim * 2 bytes = 57,344 bytes/token`. For a 128K window:

| KV precision | KV bytes/token | KV cache @128K | Weights (Q4) | + OS/editor (~5 GB) | Total | Fits 16 GB? |
|--------------|----------------|----------------|--------------|---------------------|-------|-------------|
| fp16 | 57,344 | ~7.0 GB | ~4.7 GB | ~5 GB | ~16.7 GB | No (over budget) |
| q8_0 | ~28,672 | ~3.7-3.9 GB | ~4.7 GB | ~5 GB | ~13.5 GB | Marginal - needs OS tuning (see below) |
| q4_0 | ~14,336 | ~2.0-2.2 GB | ~4.7 GB | ~5 GB | ~11.8 GB | Yes - baseline (no tuning) |

(q8_0/q4_0 block formats store a per-block scale, so effective KV runs ~5-15% above the naive
1.0 / 0.5 bytes/token - reflected above.) Conclusions, verified by V-MAXCTX:
- **fp16 KV at 128K does not fit** (~16.7 GB) - excluded.
- **q4_0 is the baseline**: ~11.8 GB total fits with no OS changes; cost is ~+0.2-0.25 perplexity vs
  fp16 (occasional long-range slips, acceptable for code review).
- **q8_0 is marginal**: ~13.5 GB exceeds the default macOS GPU wired-memory cap
  (`iogpu.wired_limit_mb`, ~67% / ~10.7 GB on 16 GB), so it requires raising that limit or it swaps.

Operational prerequisites for the 128K path (V-MAXCTX / correction C-4):
- 128K is reachable only by enabling **YaRN rope-scaling** (factor 4.0, original_max 32768); the
  model is natively 32,768 and Qwen warns static YaRN can degrade short-context quality - enable it
  only when long context is needed.
- Quantized KV in llama.cpp/Ollama **requires Flash Attention**, and on Apple-Silicon Metal must use
  **symmetric** types (q4_0/q4_0 or q8_0/q8_0) to stay on the supported fused kernel.
- Verified runtime for this path is **Ollama/llama.cpp**; the MLX-server KV-quant gap (5.3 runtime
  table) is an open dependency to re-confirm before relying on the MLX server for 128K.

V-MAXCTX confirms the exact achievable ceiling on the chosen runtime; the figures above are
first-order estimates. Because a 128K window admits a large context, the size bound (Section 9) is
expressed parametrically in `W` with a conservative guaranteed-safe floor of `num_ctx = 16,384`, and
FR-11 (`@file`/stdin context) becomes mandatory in
phase 1 (Section 5.4) so large context never transits argv.

### 5.4 Context ingestion (FR-11, C-2, C-4)
Arena's `--context` is a single `String` today. Because the default model carries a 128K window,
large context is routine, so the design makes file/stdin ingestion the **primary phase-1
mechanism**, not a later enhancement:

1. **Primary (phase 1, FR-11 mandatory):** add `--context @<path>` and stdin (`-`) support to
   arena's `create` command so context is read from a file/stream rather than argv. This removes
   the `ARG_MAX` ceiling entirely (DSN 9.1 becomes moot for the diff payload). `arenax` writes the
   bounded diff to a `0600` temp file and passes `@path`; the file is removed after use (NFR-5).
2. **Belt-and-suspenders:** `arenax` still applies the deterministic size bound (Section 9.2,
   parametric in the model window `W`) before writing the temp file, so context never exceeds the
   model's usable budget regardless of ingestion path.

### 5.5 Deterministic baseline (FR-12, A4)
`libs/libmorphlex.so` is present and built. The MX worker is used two ways: (a) as an always
available, network-free deterministic worker that can participate in any session, and (b) as an
exact token counter for the size bound, replacing the bytes/4 heuristic with a precise count.

---

## 6. Git Hook Strategy (FR-13..FR-15)

### 6.1 Default: advisory, opt-in
Hooks are not installed unless the user runs `arenax setup --install-hooks`. When installed, the
default templates are **advisory**: they invoke `arenax review-staged` (pre-commit) or
`arenax review-range origin/<branch>..HEAD` (pre-push), print the summary and session id, and
**always exit 0**. The LLM, being non-deterministic and network-or-compute bound, is kept out of
the commit-blocking path.

### 6.2 Optional: blocking mode
Behind `hook_mode: blocking` in config, the pre-commit/pre-push hook runs the session in
**council** mode and blocks (non-zero exit) only when the council decision is `reject` or council
confidence is below `auto_approve_threshold` (default 0.9). This reuses arena's existing
`council-evaluate` and threshold semantics rather than inventing a new gate.

### 6.3 Coexistence with existing guards (FR-15)
The existing `rr-verify-guard` / `rr-commit-guard` are **PreToolUse** hooks in Claude Code
settings; arena hooks are **Git** hooks in `.git/hooks`. They operate at different layers and do
not collide. Arena hooks are additive and reversible (`arenax setup --uninstall-hooks`).

`Design rationale:` advisory-by-default honors the principle that a deterministic test gate
(rr-verify-guard) is the right thing to block commits, while a probabilistic review is the right
thing to *inform* them. Blocking mode remains available because local + temperature-0 + council
makes it defensible - but it is never the silent default.

---

## 7. Error Handling

| Failure | Detection | Response |
|---------|-----------|----------|
| `arena` not on PATH | `internal/arena` lookup | typed `ErrArenaMissing`; exit 3; suggest `arenax setup`. |
| Local endpoint unreachable (`local` mode) | health probe to `base_url` | typed `ErrEndpointDown`; exit 4; suggest start Ollama or `--backend api`. |
| Empty diff | `gitx` returns zero bytes | typed `ErrNoChanges`; exit 5; informational. |
| Context exceeds bound and no `@file` support | `sizebound` | warn + bounded truncation in phase 1; exact via `@file` after FR-11. |
| arena non-zero exit | exit code capture | propagate arena stderr; map to typed error; non-zero exit. |
| Malformed `Session created:` line | UUID parse | typed `ErrSessionParse`; exit 6; print raw stdout for diagnosis. |

All errors use Go typed sentinels wrapped with `%w`, mirroring arena's `thiserror` discipline.

---

## 8. Security Design (NFR-4, NFR-5, constraint C-9)

- **Secrets via env only.** API keys are read by arena from `OPENAI_API_KEY` / `ANTHROPIC_API_KEY`.
  `arenax` passes the environment through to the child process and **never** places a key in argv
  (argv is visible to any local user via `ps`). This is an explicit design rule, validated by a
  test that asserts no key-shaped string appears in constructed argv.
- **Loopback restriction.** In `local` mode the configured `base_url` host must resolve to a
  loopback address (`127.0.0.0/8`, `::1`) unless `allow_remote_endpoint: true` is explicitly set.
  This prevents source code from being shipped to an unintended host - an SSRF-style guard applied
  to our own egress.
- **Temp-file hygiene.** Diff temp files (for `@file` ingestion) are created with `0600` in the
  job temp directory and removed in a deferred cleanup, never in world-readable `/tmp` with default
  perms.
- **No code egress in local mode (NFR-4).** With loopback-only endpoints, FR-13 advisory hooks, and
  the MX worker, an everyday review produces zero outbound connections - verifiable by running
  `arenax review-staged --backend local` with the network down.

---

## 9. Mathematical Validation

### 9.1 Context-size bound is safe with respect to ARG_MAX (C-4)
Let `A` = OS `ARG_MAX` = 1,048,576 bytes (macOS `kern.argmax`). For `execve`, the sum of all
argument bytes plus all environment bytes plus their pointer overhead must be <= `A`. Let:

- `E` = environment size, conservatively bounded by 64 KB = 65,536 bytes on a developer shell,
- `F` = fixed `arenax`->`arena` argument overhead (subcommand, flags, UUID, model id), bounded by
  4 KB = 4,096 bytes,
- `Cmax` = chosen context cap = 40 KB = 40,960 bytes (derived from the 16K-token window in 9.2).

Total worst case `T = E + F + Cmax = 65,536 + 4,096 + 40,960 = 110,592` bytes.

Since `T = 110,592 < 1,048,576 = A`, we have `T / A approximately 0.105`. The invocation uses at
most ~10.5% of `ARG_MAX`, leaving a >9x safety margin. Therefore an `E2BIG` failure is impossible
under the phase-1 bound, independent of normal environment variation. QED.

(When FR-11 `@file` ingestion lands, context bytes leave argv entirely and this bound becomes
irrelevant - the guarantee then holds for arbitrary diff sizes.)

### 9.2 Context fits the local model window
Let the on-device context window be `W = 16,384` tokens. This is the design cap from Section 5.3:
the V2-Lite default supports 128K but is capped to 16K to fit 16 GB, and the 6.7B light profile is
natively 16K - so 16K is the common, hardware-honest bound for both. Reserve `S` for the system
prompt + task (<= 1,536 tokens) and `R` for the response (<= 4,096 tokens). The usable context
budget is `B = W - S - R = 16,384 - 1,536 - 4,096 = 10,752` tokens.

Using the conservative code heuristic of 4 bytes per token, a 40 KB diff is
`40,960 / 4 = 10,240` tokens. Since `10,240 <= 10,752`, the bounded diff fits with a
`10,752 - 10,240 = 512`-token margin. The bytes/4 figure is a heuristic; FR-12 replaces it with an
exact MX token count, after which the bound is enforced on true tokens rather than an estimate,
eliminating the approximation error. (Under an 8K `num_ctx` profile the same derivation yields
`B = 2,560` tokens, so the cap scales down with the window; the size bound is therefore expressed
as a function of `W`, not a fixed constant.)

### 9.3 Determinism of the wrapper (NFR-2)
Each wrapper transform is a pure function of its inputs:
`StagedDiff` reads immutable Git object state; `Classify` is a fixed predicate table; `Fit` is a
deterministic truncation/count; `BuildArgv` is string assembly. Composition of pure functions is
pure, so `f = Summarize . Run . Create . BuildArgv . Fit . (Classify|Diff)` is deterministic in
its inputs (the repository state and config). The only non-determinism is inside `arena.Run` (LLM
sampling), which is isolated behind the process boundary and driven to temperature 0. Hence
identical repository state + config yields identical arena **invocations** (the property under
test), even when model **outputs** vary.

---

## 10. Testing Strategy (NFR-2, NFR-7)

| Layer | Method | Coverage target |
|-------|--------|-----------------|
| Pure functions (`gitx`, `drift.Classify`, `sizebound`, UUID parse, argv build) | Go table-driven unit tests + property tests (e.g. argv never contains a secret; bound always <= cap) | > 80% |
| arena CLI contract | Integration test invoking the real binary with the keyless `mock` adapter (offline, deterministic) for a full create -> run -> finalize cycle | full happy path + 3 failure paths |
| Hooks | Smoke test in a throwaway Git repo: advisory hook never blocks; blocking hook blocks on simulated reject | both modes |
| arena core delta | Rust unit tests: config parse populates `base_url`; registration builds an adapter pointed at the local endpoint; `--context @file` reads file contents | per changed function |

---

## 11. Phasing

| Phase | Scope | Exit criterion |
|-------|-------|----------------|
| 1 (this module) | `arenax` commands, arena local-inference seam, advisory hooks, VCS + docs | All Must requirements met; acceptance criteria pass. |
| 2 | Devcontainer packaging (D4); large-diff chunking; forge-aware PR review (FR-7) | "Clone and go" reproducibility. |
| 3 (conditional) | VS Code extension - only if inline-editor UX becomes a hard requirement | n/a unless triggered. |

---

## 12. Open Design Questions (to confirm at spec review)

1. Default local model: RESOLVED 2026-06-05 - Qwen2.5-Coder-7B-Instruct (7B dense, Apache-2.0),
   chosen to keep a 128K window on a 7B footprint, run on an MLX/Metal runtime. Fallbacks:
   DeepSeek-Coder 6.7B-instruct (light, 16K) and DeepSeek-Coder-V2-Lite (16B, 128K). Confirmation
   pending verification task V-MODEL (and V-MAXCTX for the achievable 128K ceiling on 16 GB).
2. FR-11 (`@file`/stdin context): RESOLVED - phase 1 mandatory. The 128K-capable default makes
   large context routine, so context must not transit argv.
3. Whether `arenax setup` should offer to install Ollama (gated by the brew PreToolUse hook) or
   only verify and instruct. Recommendation: verify + instruct, to respect C-9.

End of Design Specification.
