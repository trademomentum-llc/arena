# Arena Dev Integration - M1 (Rust Core) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Extend the arena Rust core so review/drift can run against a local OpenAI-compatible endpoint (the default path the `arenax` wrapper will drive), without breaking the existing 39-test baseline.

**Architecture:** Add an optional per-agent `base_url` to the integration config and thread it through both registration paths via the adapters' existing `with_config` constructor; register a local OpenAI-compatible worker in the CLI registry (the path `arenax` uses); add `@file`/stdin context ingestion so large diffs never transit `argv`; add a pure loopback guard so local mode cannot accidentally ship code to a non-loopback host.

**Tech Stack:** Rust 2021, tokio, clap v4, serde/serde_yaml, thiserror, tracing. Tests via `cargo test`.

**Scope note:** This plan is M1 only. M2 (the `arenax` Go wrapper) and M3 (git hooks + verification) depend on M1 and get their own plans once M1 is green. See "Subsequent Plans" at the end.

**Baseline:** root commit `dc6dcb8` on `main`; `cargo test` = 39 passed, 0 failed. Every task must keep that green.

**Conventions for all steps:**
- Run tests with: `cargo test --manifest-path /Users/nnos/Projects/arena/Cargo.toml <args>`
- Commit with: `git -C /Users/nnos/Projects/arena add <paths> && git -C /Users/nnos/Projects/arena commit -m "<msg>"` (the `git -C` form is required by this environment's bash guard; do not `cd`).
- Every commit message ends with the trailer: `Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>`
- No emojis in any source, config, or test.

---

## File Structure

| File | Responsibility | Action |
|------|----------------|--------|
| `src/adapters/endpoint.rs` | Pure loopback-validation helper for local `base_url` (NFR-4). New focused unit. | Create |
| `src/adapters/mod.rs` | Re-export the new `endpoint` module. | Modify |
| `src/integration/config.rs` | `AgentDef` gains `base_url` + `api_key_env`; `ArenaConfig::default()` literals updated. | Modify |
| `src/integration/runner.rs` | Registration uses `with_config` + loopback guard for `base_url`. | Modify |
| `src/cli/commands.rs` | `resolve_context` helper; wire it into `Create`; register a local OpenAI-compatible worker; `with_config` for default agents. | Modify |
| `config/default.yaml` | Add the local worker agent entry. | Modify |

Each unit has one responsibility: `endpoint.rs` validates, `resolve_context` ingests, `config.rs` declares, the registration sites wire. The two most error-prone bits (loopback parsing, context ingestion) are isolated as pure functions with their own tests.

---

## Task 1: Loopback guard (pure, TDD first)

**Files:**
- Create: `src/adapters/endpoint.rs`
- Modify: `src/adapters/mod.rs` (add `pub mod endpoint;`)
- Test: in `src/adapters/endpoint.rs` (`#[cfg(test)]`)

- [ ] **Step 1: Add the module declaration**

In `src/adapters/mod.rs`, add this line near the other `pub mod` declarations:

```rust
pub mod endpoint;
```

- [ ] **Step 2: Write the failing tests**

Create `src/adapters/endpoint.rs` with ONLY the tests first (no impl yet):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loopback_localhost_ok() {
        assert!(validate_local_endpoint("http://localhost:11434/v1", false).is_ok());
    }

    #[test]
    fn loopback_ipv4_ok() {
        assert!(validate_local_endpoint("http://127.0.0.1:1234/v1", false).is_ok());
        assert!(validate_local_endpoint("http://127.5.6.7:8080", false).is_ok());
    }

    #[test]
    fn loopback_ipv6_ok() {
        assert!(validate_local_endpoint("http://[::1]:1234/v1", false).is_ok());
    }

    #[test]
    fn non_loopback_rejected() {
        assert!(validate_local_endpoint("http://10.0.0.5:11434/v1", false).is_err());
        assert!(validate_local_endpoint("https://api.example.com/v1", false).is_err());
    }

    #[test]
    fn allow_remote_bypasses() {
        assert!(validate_local_endpoint("https://api.example.com/v1", true).is_ok());
    }

    #[test]
    fn unparseable_errors() {
        assert!(validate_local_endpoint("", false).is_err());
    }
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test --manifest-path /Users/nnos/Projects/arena/Cargo.toml endpoint`
Expected: FAIL to compile - `validate_local_endpoint` not found.

- [ ] **Step 4: Write the implementation**

Prepend to `src/adapters/endpoint.rs` (above the test module):

```rust
use std::net::{Ipv4Addr, Ipv6Addr};

/// Validate that a local-inference `base_url` targets a loopback host, unless
/// remote endpoints are explicitly allowed. Prevents source code from being
/// sent to a non-loopback endpoint in local mode (NFR-4).
pub fn validate_local_endpoint(base_url: &str, allow_remote: bool) -> Result<(), String> {
    if allow_remote {
        return Ok(());
    }
    let host = extract_host(base_url)?;
    if is_loopback_host(&host) {
        Ok(())
    } else {
        Err(format!(
            "base_url host '{}' is not loopback; set allow_remote_endpoint=true to permit",
            host
        ))
    }
}

/// Extract the host from a URL authority, tolerating scheme, userinfo, port,
/// and bracketed IPv6 literals. Deterministic string parsing, no DNS.
fn extract_host(base_url: &str) -> Result<String, String> {
    let after_scheme = base_url.split("://").nth(1).unwrap_or(base_url);
    let authority = after_scheme.split('/').next().unwrap_or("");
    let host_port = authority.rsplit('@').next().unwrap_or(authority);
    let host = if let Some(rest) = host_port.strip_prefix('[') {
        rest.split(']').next().unwrap_or("").to_string()
    } else {
        host_port.split(':').next().unwrap_or("").to_string()
    };
    if host.is_empty() {
        Err(format!("could not parse host from base_url '{}'", base_url))
    } else {
        Ok(host)
    }
}

fn is_loopback_host(host: &str) -> bool {
    host == "localhost"
        || host.parse::<Ipv4Addr>().map(|ip| ip.is_loopback()).unwrap_or(false)
        || host.parse::<Ipv6Addr>().map(|ip| ip.is_loopback()).unwrap_or(false)
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --manifest-path /Users/nnos/Projects/arena/Cargo.toml endpoint`
Expected: PASS - 6 tests ok.

- [ ] **Step 6: Confirm baseline still green**

Run: `cargo test --manifest-path /Users/nnos/Projects/arena/Cargo.toml`
Expected: 45 passed (39 baseline + 6 new), 0 failed.

- [ ] **Step 7: Commit**

```bash
git -C /Users/nnos/Projects/arena add src/adapters/endpoint.rs src/adapters/mod.rs
git -C /Users/nnos/Projects/arena commit -m "feat(adapters): add loopback endpoint guard for local inference

Pure validator that rejects non-loopback base_url unless allow_remote is set (NFR-4).

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 2: `@file`/stdin context ingestion (pure helper, TDD)

**Files:**
- Modify: `src/cli/commands.rs` (add `resolve_context` + a `#[cfg(test)]` module; wire into `Create`)
- Test: in `src/cli/commands.rs`

- [ ] **Step 1: Write the failing tests**

Add at the end of `src/cli/commands.rs`:

```rust
#[cfg(test)]
mod context_tests {
    use super::*;

    #[test]
    fn resolve_literal_passthrough() {
        assert_eq!(resolve_context(Some("hello".to_string())).unwrap(), Some("hello".to_string()));
    }

    #[test]
    fn resolve_none_stays_none() {
        assert_eq!(resolve_context(None).unwrap(), None);
    }

    #[test]
    fn resolve_at_file_reads_contents() {
        let path = std::env::temp_dir().join("arena_ctx_resolve_test.txt");
        std::fs::write(&path, "file body 123").unwrap();
        let arg = format!("@{}", path.display());
        let got = resolve_context(Some(arg)).unwrap();
        assert_eq!(got, Some("file body 123".to_string()));
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn resolve_missing_file_errors() {
        assert!(resolve_context(Some("@/no/such/file/xyz.txt".to_string())).is_err());
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --manifest-path /Users/nnos/Projects/arena/Cargo.toml context_tests`
Expected: FAIL to compile - `resolve_context` not found.

- [ ] **Step 3: Write the implementation**

Add this free function near the other helpers (e.g. just above `fn detect_backend`):

```rust
/// Resolve a `--context` argument: a literal string, `@<path>` to read the
/// file's contents, or `-` to read stdin. This removes the ARG_MAX ceiling on
/// large context (FR-11): callers pass `@<tempfile>` instead of a huge argv.
fn resolve_context(raw: Option<String>) -> Result<Option<String>, Box<dyn std::error::Error>> {
    use std::io::Read;
    match raw.as_deref() {
        None => Ok(None),
        Some("-") => {
            let mut buf = String::new();
            std::io::stdin().read_to_string(&mut buf)?;
            Ok(Some(buf))
        }
        Some(s) if s.starts_with('@') => {
            let content = std::fs::read_to_string(&s[1..])?;
            Ok(Some(content))
        }
        Some(s) => Ok(Some(s.to_string())),
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --manifest-path /Users/nnos/Projects/arena/Cargo.toml context_tests`
Expected: PASS - 4 tests ok.

- [ ] **Step 5: Wire `resolve_context` into the `Create` handler**

In `src/cli/commands.rs`, inside `Commands::Create { ... context } => {` (the destructuring is at approximately lines 187-194), insert the resolution as the FIRST statement of the arm, before `parse_session_type` is called:

```rust
        Commands::Create {
            session_type,
            mode,
            workers,
            council,
            task,
            context,
        } => {
            let context = resolve_context(context)?;   // <-- ADD THIS LINE
            let parsed_type = parse_session_type(&session_type, &task, &context)?;
```

This shadows `context` with the resolved `Option<String>`; the existing `context,` field in the `Task { ... }` literal (approximately line 233) now stores resolved content. No other change in the arm.

- [ ] **Step 6: Confirm build + baseline green**

Run: `cargo test --manifest-path /Users/nnos/Projects/arena/Cargo.toml`
Expected: 49 passed (45 + 4 new), 0 failed.

- [ ] **Step 7: Commit**

```bash
git -C /Users/nnos/Projects/arena add src/cli/commands.rs
git -C /Users/nnos/Projects/arena commit -m "feat(cli): resolve --context from @file or stdin (FR-11)

Adds resolve_context helper and wires it into create, removing the ARG_MAX
ceiling on large review context.

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 3: `AgentDef` gains `base_url` + `api_key_env`

**Files:**
- Modify: `src/integration/config.rs` (struct at lines 26-32; `ArenaConfig::default()` AgentDef literals at approximately lines 52-65)
- Test: in `src/integration/config.rs` (existing `#[cfg(test)] mod tests`)

- [ ] **Step 1: Write the failing test**

Add to the existing `mod tests` in `src/integration/config.rs`:

```rust
    #[test]
    fn agentdef_base_url_defaults_none_and_parses_some() {
        // Absent in YAML -> None (serde default)
        let yaml_no_url = "id: a\nbackend: openai\nmodel: m\ntier: worker\n";
        let a: AgentDef = serde_yaml::from_str(yaml_no_url).unwrap();
        assert_eq!(a.base_url, None);
        assert_eq!(a.api_key_env, None);

        // Present in YAML -> Some
        let yaml_url = "id: b\nbackend: openai\nmodel: m\ntier: worker\nbase_url: http://localhost:11434/v1\napi_key_env: LOCAL_API_KEY\n";
        let b: AgentDef = serde_yaml::from_str(yaml_url).unwrap();
        assert_eq!(b.base_url.as_deref(), Some("http://localhost:11434/v1"));
        assert_eq!(b.api_key_env.as_deref(), Some("LOCAL_API_KEY"));
    }
```

Note: this test uses `serde_yaml`. If `serde_yaml` is not already a dependency, add `serde_yaml = "0.9"` under `[dev-dependencies]` in `Cargo.toml` (it is only needed for the test).

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --manifest-path /Users/nnos/Projects/arena/Cargo.toml agentdef_base_url`
Expected: FAIL to compile - `base_url`/`api_key_env` fields do not exist.

- [ ] **Step 3: Add the fields to `AgentDef`**

Replace the struct (lines 26-32):

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
    /// Env var holding the API key; unused/ignored by loopback endpoints.
    #[serde(default)]
    pub api_key_env: Option<String>,
}
```

- [ ] **Step 4: Update the `ArenaConfig::default()` AgentDef literals**

In `ArenaConfig::default()` there are two `AgentDef { ... }` literals (approximately lines 53-64). Add the two new fields to BOTH so the struct literals compile:

```rust
                AgentDef {
                    id: "gpt-4-turbo".to_string(),
                    backend: "openai".to_string(),
                    model: "gpt-4-turbo".to_string(),
                    tier: "worker".to_string(),
                    base_url: None,
                    api_key_env: None,
                },
                AgentDef {
                    id: "claude-3-sonnet".to_string(),
                    backend: "anthropic".to_string(),
                    model: "claude-3-sonnet-20240229".to_string(),
                    tier: "worker".to_string(),
                    base_url: None,
                    api_key_env: None,
                },
```

- [ ] **Step 5: Run test + baseline**

Run: `cargo test --manifest-path /Users/nnos/Projects/arena/Cargo.toml`
Expected: 50 passed (49 + 1 new), 0 failed. In particular `test_default_config` still passes (the new fields default to `None`).

- [ ] **Step 6: Commit**

```bash
git -C /Users/nnos/Projects/arena add src/integration/config.rs Cargo.toml
git -C /Users/nnos/Projects/arena commit -m "feat(config): add base_url and api_key_env to AgentDef (FR-8)

serde(default) keeps existing YAML and ArenaConfig::default compatible.

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 4: Thread `base_url` through the integration registration

**Files:**
- Modify: `src/integration/runner.rs` (OpenAI branch lines 30-34; Anthropic branch lines 44-48)

- [ ] **Step 1: Update the OpenAI branch**

Replace the OpenAI registration body (lines 28-35, the `registry.register(... OpenAIAdapter::new(...))` call) with a version that validates the optional `base_url` and uses `with_config`:

```rust
                        if let Some(url) = &agent_def.base_url {
                            if let Err(e) = crate::adapters::endpoint::validate_local_endpoint(url, false) {
                                warn!(agent = %agent_def.id, error = %e, "Skipping agent: invalid local endpoint");
                                continue;
                            }
                        }
                        registry.register(
                            &agent_def.id,
                            Box::new(crate::adapters::openai::OpenAIAdapter::with_config(
                                key,
                                agent_def.model.clone(),
                                agent_def.base_url.clone(),
                                30_000,
                                3,
                            )),
                        );
                        info!(agent = %agent_def.id, "Registered OpenAI agent");
```

- [ ] **Step 2: Update the Anthropic branch**

Apply the analogous change to the Anthropic branch (lines 42-49): same loopback check on `agent_def.base_url`, then `AnthropicAdapter::with_config(key, agent_def.model.clone(), agent_def.base_url.clone(), 30_000, 3)`.

- [ ] **Step 3: Run the existing runner test + baseline**

Run: `cargo test --manifest-path /Users/nnos/Projects/arena/Cargo.toml`
Expected: 50 passed, 0 failed. `test_runner_creation_with_default_config` still passes (default agents have `base_url: None`, so registration is unchanged in behavior).

- [ ] **Step 4: Commit**

```bash
git -C /Users/nnos/Projects/arena add src/integration/runner.rs
git -C /Users/nnos/Projects/arena commit -m "feat(integration): pass base_url through agent registration (FR-8/FR-9)

OpenAI and Anthropic registration use with_config and validate local endpoints.

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 5: Register a local worker in the CLI registry (the path arenax drives)

**Files:**
- Modify: `src/cli/commands.rs` (registry init block, lines 138-184)

**Context:** the CLI `execute` builds its own registry (it does not load `ArenaConfig`). `arenax` runs `arena create --workers qwen-coder-local ...` then `arena run`, so the worker id `qwen-coder-local` must resolve to a registered adapter. The local endpoint/model are read from env with loopback-safe defaults.

- [ ] **Step 1: Add the local-agent registration block**

In `src/cli/commands.rs`, immediately after the mock-agent registrations (after line 182, before `let orchestrator = ...` at line 184), insert:

```rust
    // Register a local OpenAI-compatible worker (e.g. Ollama / llama.cpp / MLX server).
    // Endpoint and model are env-overridable; the API key is a placeholder local
    // runtimes ignore. Skips registration if the endpoint is non-loopback (NFR-4).
    {
        let endpoint = std::env::var("ARENA_LOCAL_ENDPOINT")
            .unwrap_or_else(|_| "http://localhost:11434/v1".to_string());
        let model = std::env::var("ARENA_LOCAL_MODEL")
            .unwrap_or_else(|_| "qwen2.5-coder:7b".to_string());
        let allow_remote = std::env::var("ARENA_ALLOW_REMOTE_ENDPOINT").is_ok();
        match crate::adapters::endpoint::validate_local_endpoint(&endpoint, allow_remote) {
            Ok(()) => {
                registry.register(
                    "qwen-coder-local",
                    Box::new(OpenAIAdapter::with_config(
                        std::env::var("ARENA_LOCAL_API_KEY").unwrap_or_else(|_| "local".to_string()),
                        model,
                        Some(endpoint),
                        30_000,
                        3,
                    )),
                );
            }
            Err(e) => {
                eprintln!("Warning: local agent not registered: {}", e);
            }
        }
    }
```

- [ ] **Step 2: Build + baseline**

Run: `cargo test --manifest-path /Users/nnos/Projects/arena/Cargo.toml`
Expected: 50 passed, 0 failed (no new test; this is a registration wiring change covered by the manual verification in Task 7).

- [ ] **Step 3: Manual smoke check (no network needed - registration only)**

Run: `cargo run --manifest-path /Users/nnos/Projects/arena/Cargo.toml -- create --session-type code-review --task "smoke" --workers "qwen-coder-local"`
Expected: prints `Session created: <UUID>` and `Use 'arena run ...'`. (Running it would require the local endpoint up; creation alone must succeed.)

- [ ] **Step 4: Commit**

```bash
git -C /Users/nnos/Projects/arena add src/cli/commands.rs
git -C /Users/nnos/Projects/arena commit -m "feat(cli): register local OpenAI-compatible worker (FR-9)

qwen-coder-local resolves to an env-configurable local endpoint with a loopback guard.

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 6: Add the local worker to `config/default.yaml`

**Files:**
- Modify: `config/default.yaml`

- [ ] **Step 1: Add the agent entry**

Under the `agents:` list in `config/default.yaml`, add (the morphlex comment block already documents the pattern):

```yaml
  - id: "qwen-coder-local"
    backend: "openai"
    model: "qwen2.5-coder:7b"
    tier: "worker"
    base_url: "http://localhost:11434/v1"
    api_key_env: "ARENA_LOCAL_API_KEY"
```

- [ ] **Step 2: Verify it parses (reuses Task 3's deserialization path)**

Run: `cargo test --manifest-path /Users/nnos/Projects/arena/Cargo.toml`
Expected: 50 passed, 0 failed. (Schema validated by `agentdef_base_url_defaults_none_and_parses_some`.)

- [ ] **Step 3: Commit**

```bash
git -C /Users/nnos/Projects/arena add config/default.yaml
git -C /Users/nnos/Projects/arena commit -m "chore(config): add qwen-coder-local agent to default.yaml

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 7: End-to-end offline verification (mock adapter)

**Files:** none (verification only)

- [ ] **Step 1: Full create -> run -> finalize using the keyless mock adapter (offline)**

```bash
cargo build --manifest-path /Users/nnos/Projects/arena/Cargo.toml
/Users/nnos/Projects/arena/target/debug/arena create --session-type code-review --task "verify M1" --workers "mock-reviewer-1,mock-reviewer-2"
```
Expected: prints `Session created: <UUID>`.

- [ ] **Step 2: Run and finalize**

Capture the UUID from Step 1, then:

```bash
/Users/nnos/Projects/arena/target/debug/arena run --session-id <UUID>
/Users/nnos/Projects/arena/target/debug/arena finalize --session-id <UUID> --decision approve --reasoning "M1 verification"
```
Expected: run prints agent responses (mock); finalize prints `Session finalized: <UUID>`. No network egress.

- [ ] **Step 3: `@file` context path**

```bash
printf 'diff --git a/x b/x\n+changed line\n' > /Users/nnos/.claude/jobs/66421fbb/tmp/m1_ctx.diff
/Users/nnos/Projects/arena/target/debug/arena create --session-type code-review --task "ctx via file" --workers "mock-reviewer-1" -x "@/Users/nnos/.claude/jobs/66421fbb/tmp/m1_ctx.diff"
```
Expected: `Session created: <UUID>`; the session's stored context equals the file body (inspect with `arena view --session-id <UUID>`).

- [ ] **Step 4: Final baseline + commit a verification note**

Run: `cargo test --manifest-path /Users/nnos/Projects/arena/Cargo.toml`
Expected: 50 passed, 0 failed.

Update `TODO.md` M1 rows to DONE, then:

```bash
git -C /Users/nnos/Projects/arena add TODO.md
git -C /Users/nnos/Projects/arena commit -m "docs: mark M1 tasks complete

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Subsequent Plans (not in this plan)

- **M2 - arenax Go wrapper:** `cmd/arenax` + `internal/{gitx,arena,drift,sizebound,config}`; commands `review-staged`, `review-range`, `drift`, `setup`, `doctor`. Depends on M1 (`@file` ingestion, `qwen-coder-local` worker). Note the verified arena stdout markers (TECHNICAL spec 3.2), incl. drift empty-output literal `"No drift detected. Implementations match specs."`. Authored as its own TDD plan once M1 is green.
- **M3 - hooks + verification:** advisory pre-commit/pre-push templates calling `arenax`; optional blocking mode; reversible install. Depends on M2.

---

## Self-Review

**Spec coverage (M1 scope):** FR-8 (base_url) -> Tasks 3,4,5; FR-9 (local worker) -> Tasks 5,6; FR-11 (`@file`/stdin) -> Task 2; NFR-4 (loopback guard) -> Tasks 1,4,5. The arena-core corrections from verification are honored: q4_0/YaRN/Flash-Attention are runtime settings (documented in spec 2.4, not code), so no M1 task; the drift-string literal (C-1) is an M2 parser concern, flagged in Subsequent Plans.

**Placeholder scan:** No TBD/TODO-in-code; every code step shows complete code; every test step shows the assertions; every run step shows the command and expected result.

**Type consistency:** `validate_local_endpoint(&str, bool) -> Result<(), String>` is defined in Task 1 and called identically in Tasks 4 and 5. `resolve_context(Option<String>) -> Result<Option<String>, Box<dyn Error>>` defined and wired in Task 2. `OpenAIAdapter::with_config` / `AnthropicAdapter::with_config` signatures match the verified source `(api_key, model, base_url: Option<String>, timeout_ms, max_retries)`. `AgentDef` new fields `base_url`/`api_key_env` (both `Option<String>`) are used consistently in Tasks 3,4,6.

**Baseline guard:** every implementation task ends by running the full suite and expects the running cumulative count (39 -> 45 -> 49 -> 50), so any regression in the existing 39 is caught immediately.
