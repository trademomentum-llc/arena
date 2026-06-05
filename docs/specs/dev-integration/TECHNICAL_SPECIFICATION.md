# Arena Dev Integration - Technical Specification

| Field | Value |
|-------|-------|
| Document | Technical Specification (TEC) |
| Module | Arena Dev Integration (`arenax`) |
| Version | 1.0.0 |
| Date | 2026-06-05 |
| Author | Jason M Jarmacz, Trident Markets Group LLC |
| Status | Draft - pending review |
| Related | REQUIREMENTS.md, DESIGN_SPECIFICATION.md |

---

## 1. Overview

This document specifies the exact implementation: arena core changes (Rust) with file and line
references, the `arenax` Go module, configuration schemas, deterministic algorithms, CLI
contracts, the test plan, and the verification procedure. Line numbers reference the arena source
as read on 2026-06-05 and are anchors, not guarantees; implementers must confirm against the file
at edit time.

---

## 2. Arena Core Changes (Rust)

All changes follow `design_specs/coding_standards.md`: `thiserror` errors, `?` propagation,
`tracing` logs, functions under 50 lines, table-driven tests.

### 2.1 Add `base_url` and `api_key_env` to `AgentDef` (FR-8, C-5)
File: `src/integration/config.rs` (struct at lines 27-32).

Before:
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentDef {
    pub id: String,
    pub backend: String,
    pub model: String,
    pub tier: String,
}
```

After:
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentDef {
    pub id: String,
    pub backend: String,
    pub model: String,
    pub tier: String,
    /// Optional override endpoint (e.g. http://localhost:11434/v1 for a local runtime).
    #[serde(default)]
    pub base_url: Option<String>,
    /// Env var holding the API key; ignored by loopback endpoints.
    #[serde(default)]
    pub api_key_env: Option<String>,
}
```

`#[serde(default)]` preserves backward compatibility: existing YAML without these fields still
deserializes.

### 2.2 Pass `base_url` through registration (FR-8, FR-9)
File: `src/integration/runner.rs` (registration loop, lines 24-61; OpenAI near line 30, Anthropic
near line 44).

Change the OpenAI branch from the hardcoded constructor to the configurable one:
```rust
// before: OpenAIAdapter::new(api_key, agent.model.clone())
let adapter = OpenAIAdapter::with_config(
    api_key,
    agent.model.clone(),
    agent.base_url.clone(),          // None => default api.openai.com; Some => local endpoint
    30_000,                          // timeout_ms, per existing default
    3,                               // max_retries, per existing default
);
```

Apply the analogous change to the Anthropic branch using `AnthropicAdapter::with_config(...)`
(the adapter already exposes it). No change is required to `src/adapters/openai.rs` or
`src/adapters/anthropic.rs` - both already implement `with_config`.

### 2.3 Mirror the change in the CLI create path (FR-8)
File: `src/cli/commands.rs` (adapter registration during `create`, lines ~141-164; OpenAI ~144,
Anthropic ~155). Replace the `::new(...)` calls with `::with_config(..., base_url, 30_000, 3)`,
sourcing `base_url` from the resolved `AgentDef`.

### 2.4 Register a default local worker (FR-9)
File: `config/default.yaml`. Add (uncommented) a local agent entry. Default is Qwen2.5-Coder-7B on
an Apple-Silicon-optimal runtime; the `model` id is the runtime's model name and `base_url` selects
the runtime (MLX example shown; Ollama is `http://localhost:11434/v1` with `model: "qwen2.5-coder:7b"`):
```yaml
  - id: "qwen-coder-local"
    backend: "openai"            # OpenAI-compatible wire format (MLX server / Ollama / LM Studio)
    model: "Qwen2.5-Coder-7B-Instruct-4bit"   # MLX community tag; Ollama uses "qwen2.5-coder:7b"
    tier: "worker"
    base_url: "http://localhost:1234/v1"       # MLX/LM Studio; Ollama: 11434
    api_key_env: "LOCAL_API_KEY"  # unused by local runtimes; present for symmetry
```
Fallback agents (same shape): `deepseek-coder:6.7b-instruct-q5_K_M` (light) and
`deepseek-coder-v2:16b-lite-instruct-q4_K_M` (higher quality). Note (C-5): the DeepSeek-V2 Ollama
tags ship with a low default `num_ctx` (~4K) - 128K is an architectural capability that must be
enabled explicitly per tag, not assumed. Registration auto-skips a local agent if `arenax doctor` /
the runtime reports the endpoint unreachable, so a missing runtime degrades gracefully rather than
erroring at startup. To reach a 128K window on 16 GB the runtime must (a) enable **YaRN rope-scaling**
for the 32K->128K extension, (b) use **q4_0 KV-cache quantization** (baseline; q8_0 needs the macOS
wired-memory cap raised), and (c) enable **Flash Attention** with a symmetric KV type on Metal
(DSN 5.3.1); these are runtime/Modelfile settings, not arena code.

### 2.5 Context ingestion from file/stdin (FR-11, C-2)
File: `src/cli/commands.rs` (the `Create` command `context` field and its handler).

Design: accept the existing `--context <STRING>` and additionally interpret a leading `@` as a
file path, plus a bare `-` as "read stdin". Pseudocode for the resolver (a new <50-line helper):
```rust
fn resolve_context(raw: Option<String>) -> Result<Option<String>, AgentError> {
    match raw.as_deref() {
        None => Ok(None),
        Some("-") => {
            let mut buf = String::new();
            std::io::Read::read_to_string(&mut std::io::stdin(), &mut buf)?;
            Ok(Some(buf))
        }
        Some(s) if s.starts_with('@') => {
            let path = &s[1..];
            Ok(Some(std::fs::read_to_string(path)?))   // reads CONTENTS, not the path
        }
        Some(s) => Ok(Some(s.to_string())),
    }
}
```
This keeps the literal-string path working (no regression) while removing the `ARG_MAX` ceiling.

### 2.6 Loopback validation (NFR-4)
Add a guard, invoked when an agent has a `base_url`, that rejects non-loopback hosts unless an
`allow_remote_endpoint` flag is set in `ArenaConfig`. Loopback set: `127.0.0.0/8`, `::1`,
`localhost`. Returns `AgentError::Auth` with a clear message otherwise.

---

## 3. `arenax` Go Module

### 3.1 Layout
```
arenax/
  go.mod                      module trademomentum.com/arenax  (Go 1.22+)
  cmd/arenax/main.go          CLI root, subcommand dispatch
  internal/gitx/gitx.go       StagedDiff, RangeDiff, ChangedFiles
  internal/arena/arena.go     Create, Run, View, Finalize, DriftCheck, ExtractUUID
  internal/review/review.go   ReviewStaged, ReviewRange, Summarize
  internal/drift/drift.go     Classify, ExpandToFiles, RunDrift
  internal/sizebound/bound.go Fit, byteCap, countTokensMX (optional)
  internal/config/config.go   Load, Backend, HookMode
  hooks/pre-commit.advisory   template
  hooks/pre-push.advisory     template
  hooks/pre-commit.blocking   template
  *_test.go                   table-driven + property tests per package
```

### 3.2 Key signatures
```go
// internal/gitx
func StagedDiff(repo string) (string, error)           // git diff --cached
func RangeDiff(repo, a, b string) (string, error)       // git diff a..b
func ChangedFiles(repo string) ([]string, error)        // git diff --name-only HEAD

// internal/arena  (the CLI contract boundary; secrets via env, never argv)
type Client struct { Bin string; Env []string }
func (c Client) Create(spec SessionSpec) (uuid string, err error)
func (c Client) Run(uuid string) (Result, error)
func (c Client) DriftCheck(specs, impls []string, agent string) (DriftResult, error)
func ExtractUUID(stdout string) (string, error)         // parses "Session created: <UUID>"
// Exact arena stdout markers the parsers must match (verified against live source, C-1):
//   create:   "Session created: <UUID>"        (commands.rs:249)
//   finalize: "Session finalized: <UUID>"       (commands.rs:409)
//   drift, no findings: "No drift detected. Implementations match specs."  (commands.rs:438)
//   drift, findings:    "Drift findings (N):" then "  [Severity] description" per line (:440,:442)
// Parsers MUST match the full literal or use contains()/HasPrefix, not the truncated string.

// internal/drift
func Classify(paths []string, rules Ruleset) (specs, impls []string)   // pure
func ExpandToFiles(paths []string) ([]string, error)                   // dirs -> files

// internal/sizebound
func Fit(ctx string, cap int, counter TokenCounter) (string, Report)   // deterministic
```

### 3.3 SessionSpec and backend selection
```go
type Backend string
const ( BackendLocal Backend = "local"; BackendAPI = "api"; BackendCouncil = "council" )

type SessionSpec struct {
    Type     string   // "code-review" | "implementation" | ...
    Task     string   // derived human-readable title
    Context  string   // bounded diff; written to 0600 temp file and passed as @path
    Workers  []string // resolved from Backend
    Mode     string   // "human-in-loop" | "council"
}
```
Worker resolution: `local` -> `["qwen-local"]` (plus `MX` if enabled); `api` ->
`["gpt-4-turbo","claude-3-sonnet"]`; `council` -> workers + council agents with `Mode="council"`.

---

## 4. Configuration Schema (`arenax`)

File: `~/.config/arenax/config.yaml` (override with `--config`).
```yaml
arena_bin: "arena"                       # resolved on PATH if relative
backend: "local"                          # local | api | council
local_endpoint: "http://localhost:1234/v1"   # MLX/LM Studio; Ollama: 11434
local_runtime: "mlx"                      # mlx (Apple-optimal) | ollama | lmstudio
local_model: "Qwen2.5-Coder-7B-Instruct-4bit"   # Ollama: "qwen2.5-coder:7b" (DSN 5.3)
num_ctx: 16384                            # conservative floor; raise toward 131072 (needs YaRN
                                          # rope-scaling enabled in the runtime) per DSN 5.3.1
kv_cache_type: "q4_0"                     # baseline: only no-tune fit for 128K on 16 GB. q8_0 is
                                          # marginal (needs iogpu.wired_limit_mb raised). Requires
                                          # Flash Attention + symmetric type on Metal (DSN 5.3.1)
max_context_bytes: 40960                  # derived from num_ctx via DSN 9.2; scales with num_ctx
allow_remote_endpoint: false              # NFR-4 loopback guard
use_mx_token_count: true                  # FR-12; exact count via libmorphlex.so
hook_mode: "advisory"                     # advisory | blocking
council_threshold: 0.9                    # mirrors arena auto_approve_threshold
```
All keys have defaults; an absent file yields the documented defaults (deterministic).

---

## 5. Deterministic Algorithms

### 5.1 UUID extraction (NFR-2)
Input: arena `create` stdout. Contract marker: line beginning `Session created: `.
```
parse(stdout):
  for line in stdout.lines():
     if line.hasPrefix("Session created: "):
        cand = trim(line[len("Session created: "):])
        if isRFC4122(cand): return cand
  return ErrSessionParse
```
`isRFC4122` is a fixed 8-4-4-4-12 hex/hyphen check. Pure; covered by a table test including
malformed lines.

### 5.2 File classification (FR-3)
Deterministic predicate table (DSN 4.2). Implementation is an ordered slice of
`{matcher func(path) bool, class}`; first match wins; unmatched paths are logged and ignored. No
network, no clock, no randomness -> identical inputs yield identical partition.

### 5.3 Size bound (DSN 9.1, 9.2)
```
Fit(ctx, cap, counter):
  if counter != nil:
     n = counter.Count(ctx)             // exact MX tokens
     if n <= tokenBudget: return ctx, {fit:true}
     return truncateToTokens(ctx, tokenBudget), {fit:false, dropped:n-tokenBudget}
  if len(ctx) <= cap: return ctx, {fit:true}
  return ctx[:cap], {fit:false, droppedBytes:len(ctx)-cap}
```
Truncation is from the tail with a clear marker; phase 2 replaces truncation with chunking.

---

## 6. CLI Contract (`arenax`)

| Command | Effect | Exit codes |
|---------|--------|-----------|
| `arenax review-staged [--backend B]` | Review staged diff. | 0 ok; 3 no arena; 4 endpoint down; 5 no changes; 6 parse. |
| `arenax review-range <a>..<b> [--backend B]` | Review range. | as above. |
| `arenax drift [--backend B]` | Classify + drift-check changed files. | 0 ok; 5 no impls; 4/3 as above. |
| `arenax setup [--install-hooks] [--uninstall-hooks]` | Bootstrap / hook management. | 0 ok; non-zero on failure. |
| `arenax doctor` | Environment check table. | 0 all-pass; 1 any-fail. |

Blocking-mode hooks additionally map a council `reject` / sub-threshold confidence to a non-zero
exit (FR-14).

---

## 7. Test Plan

### 7.1 Go unit / property tests (NFR-7)
| Test | Asserts |
|------|---------|
| `TestExtractUUID_table` | valid/invalid/multiline/missing-marker cases. |
| `TestClassify_table` | spec/impl/ignored partition for representative path sets. |
| `TestFit_byteCap` | output length <= cap for all inputs (property). |
| `TestBuildArgv_noSecret` | constructed argv never contains an env-key value (property; security). |
| `TestExpandToFiles` | directories expand to regular files only; symlink/dir edge cases. |
| `TestLoopbackGuard` | non-loopback base_url rejected unless allow flag set. |

### 7.2 arena CLI integration (offline, deterministic)
Using the keyless `mock` adapter: `create -> run -> finalize` happy path; plus failure paths
(missing binary, unreachable endpoint simulated, empty diff). Asserts the documented stdout
markers and exit codes that `arenax` depends on (NFR-9 contract pinning).

### 7.3 arena core delta (Rust)
| Test | Asserts |
|------|---------|
| `agentdef_base_url_parses` | YAML with `base_url` populates the field; without it, `None`. |
| `registration_uses_base_url` | a local AgentDef builds an OpenAIAdapter whose endpoint is the local URL. |
| `resolve_context_at_file` | `@path` reads file contents; `-` reads stdin; literal string unchanged. |
| `loopback_guard_rejects_remote` | non-loopback rejected without the allow flag. |

### 7.4 Hook smoke tests
Throwaway repo: advisory pre-commit exits 0 even when arena reports issues; blocking pre-commit
exits non-zero on a forced `reject`; uninstall restores prior hooks.

---

## 8. Build, Install, Version Control

### 8.1 Build
```
# arena core (release, per ARENA_MANUAL.md)
cargo build --release            # -> target/release/arena

# arenax wrapper
go build -o bin/arenax ./cmd/arenax
```

### 8.2 Install (FR-4)
`arenax setup` performs, idempotently:
1. Locate or build `arena`; verify `arena --help` runs.
2. Symlink/copy `arena` and `arenax` into a PATH directory.
3. Emit shell completions (bash/zsh).
4. Write `.env.example` (key names only, never values).
5. With `--install-hooks`, copy the advisory templates into `.git/hooks`, backing up any existing
   hook first (reversible per FR-15).

### 8.3 Version control (FR-16)
Initialize the arena repository and add a `.gitignore` covering at minimum:
```
/target/
/arena-sessions/
.env
*.log
```
Initial commit includes source, specs (this triad), `PROJECT_SUMMARY.md`, `TODO.md`. Note: the
existing `rr-commit-guard` / `rr-verify-guard` PreToolUse hooks may execute on commit; expect the
verify gate to run the test suite (allow time, or use the documented bypass only when intended).

---

## 9. Verification Procedure (maps to Acceptance Criteria)

End-to-end, runnable checks:

1. `arenax doctor` -> all rows PASS (FR-5, FR-8, FR-9).
2. With Ollama serving locally and the network **off**:
   `arenax review-staged --backend local` on a repo with a staged change -> prints
   `Session created: <UUID>` and a summary; `arena view --session-id <UUID>` shows local-worker
   responses; zero network egress (NFR-4).
3. `arenax drift` on a mixed change set -> correct spec/impl partition; valid `drift-check` run.
4. CI: `go test ./...` (> 80% coverage) and the mock-adapter integration test pass offline
   (NFR-2).
5. Advisory hook: stage a deliberately weak change, commit -> review printed, commit succeeds.
   Blocking hook (config `blocking`): same change -> commit blocked with the council reasoning.
6. `git -C arena status` clean after initial commit; `PROJECT_SUMMARY.md` and `TODO.md` present.

### 9.1 Automation assessment (user doctrine)
- **Safe to fully automate:** build, `go test`, the mock-adapter integration test, classification
  and bound checks - all deterministic and side-effect-free. These belong in CI.
- **Automate with a guard:** hook installation (reversible, backs up existing hooks) and VCS init
  (one-time, idempotent).
- **Do not silently automate:** runtime (Ollama) installation - gated by the `brew` PreToolUse
  hook (C-9); `arenax setup` verifies and instructs rather than installing. Blocking-mode hooks
  remain opt-in because they couple a probabilistic verdict to the commit flow.

End of Technical Specification.
