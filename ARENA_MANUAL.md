# Arena Multi-Agent System - Command Line Manual

## Overview
Arena is a multi-agent arena system for checks and balances in development. It provides a framework for creating sessions where multiple AI agents collaborate and review code, specifications, and implementations.

## Installation
```bash
cargo build --release
```
The binary will be available at `target/release/arena`.

## Usage
```bash
arena [SUBCOMMAND]
```

### Subcommands

#### `arena create`
Create a new arena session.

**Usage:**
```bash
arena create --session-type <TYPE> --task <DESCRIPTION> [OPTIONS]
```

**Options:**
- `--session-type <TYPE>`: Session type (code-review, implementation, validation, architecture, spec-drift)
- `--task <DESCRIPTION>`: Task description
- `--mode <MODE>`: Orchestration mode (human-in-loop, council) [default: human-in-loop]
- `--workers <IDS>`: Worker agents (comma-separated agent IDs using short codes)
- `--council <IDS>`: Council agents (comma-separated agent IDs using short codes, council mode only)
- `-x, --context <CONTEXT>`: Optional context

**Worker ID Short Codes:**
- `AC` = Anthropic Claude
- `OC` = OpenAI ChatGPT
- `GG` = Google Gemini
- `QQ` = QwenAI Qwen-Coder
- `PP` = PerplexityAI Perplexity
- `XG` / `xai` / `grok` = X.AI Grok (uses `XAI_API_KEY`, OpenAI-compatible at api.x.ai)
- `MK` = MoonshotAi Kimi
- `ML` = Meta Llama
- `HD` = Hangzhou DeepSeek
- `MX` = Morphlex (Return 42 deterministic tokenizer)

**Example:**
```bash
arena create --session-type code-review --task "Review authentication module" \
  --workers "OC,AC" --mode human-in-loop
```

**Morphlex Example:**
```bash
arena create --session-type code-review --task "Analyze code comments for sentiment" \
  --workers "MX" --mode human-in-loop
```

#### `arena run`
Run an arena session (dispatch task, collect responses).

**Usage:**
```bash
arena run --session-id <UUID>
```

**Options:**
- `--session-id <UUID>`: Session ID

**Example:**
```bash
arena run --session-id 123e4567-e89b-12d3-a456-426614174000
```

#### `arena list`
List active sessions.

**Usage:**
```bash
arena list [OPTIONS]
```

**Options:**
- `-a, --all`: Show all sessions (including finalized)

**Example:**
```bash
arena list --all
```

#### `arena view`
View session details.

**Usage:**
```bash
arena view --session-id <UUID>
```

**Options:**
- `--session-id <UUID>`: Session ID

**Example:**
```bash
arena view --session-id 123e4567-e89b-12d3-a456-426614174000
```

#### `arena finalize`
Finalize a session with a human decision.

**Usage:**
```bash
arena finalize --session-id <UUID> --decision <DECISION> --reasoning <TEXT> [OPTIONS]
```

**Options:**
- `--session-id <UUID>`: Session ID
- `--decision <DECISION>`: Decision (approve, approve-mods, reject, iterate)
- `--reasoning <TEXT>`: Reasoning for the decision
- `--agent <ID>`: Selected agent ID (if approving a specific agent's response)

**Example:**
```bash
arena finalize --session-id 123e4567-e89b-12d3-a456-426614174000 \
  --decision approve --reasoning "Code looks good and follows best practices"
```

#### `arena cancel`
Cancel a session.

**Usage:**
```bash
arena cancel --session-id <UUID>
```

**Options:**
- `--session-id <UUID>`: Session ID

**Example:**
```bash
arena cancel --session-id 123e4567-e89b-12d3-a456-426614174000
```

#### `arena drift-check`
Run spec drift check.

**Usage:**
```bash
arena drift-check --specs <PATHS> --impls <PATHS> [OPTIONS]
```

**Options:**
- `--specs <PATHS>`: Spec files (comma-separated paths)
- `--impls <PATHS>`: Implementation files (comma-separated paths)
- `--agent <ID>`: Agent to use for analysis [default: gpt-4-turbo]

**Example:**
```bash
arena drift-check --specs "docs/api.yaml,docs/schema.json" \
  --impls "src/api/,src/models/" --agent gpt-4-turbo
```

#### `arena metrics`
Export metrics in Prometheus format.

**Usage:**
```bash
arena metrics
```

**Example:**
```bash
arena metrics > metrics.prom
```

#### `arena council-evaluate`
Run council evaluation on an existing session.

**Usage:**
```bash
arena council-evaluate --session-id <UUID>
```

**Options:**
- `--session-id <UUID>`: Session ID

**Example:**
```bash
arena council-evaluate --session-id 123e4567-e89b-12d3-a456-426614174000
```

## Session Types

1. **code-review**: Review code changes (PRs, commits) for correctness, security, performance, and quality
2. **implementation**: Propose implementations based on specifications
3. **validation**: Review implementations against specifications
4. **architecture**: Analyze architectural options and provide recommendations
5. **spec-drift-check**: Compare implementations against specifications to identify drift

## Orchestration Modes

1. **human-in-loop**: Workers provide responses, human makes final decision
2. **council**: Workers provide responses, council agents evaluate and provide consensus

## Agent Configuration
Agents are configured via environment variables:
- `OPENAI_API_KEY`: For OpenAI models (gpt-4-turbo, gpt-4o, etc.)
- `ANTHROPIC_API_KEY`: For Anthropic models (claude-3-opus, claude-3-sonnet, etc.)

Default agents are registered automatically if API keys are available.

## Examples

### Creating a Code Review Session
```bash
arena create \
  --session-type code-review \
  --task "Review PR #42: Add OAuth2 authentication" \
  --workers "gpt-4-turbo,claude-3-sonnet" \
  --mode human-in-loop
```

### Running the Session
```bash
arena run --session-id $(arena list | grep "code-review" | head -1 | awk '{print $1}')
```

### Finalizing with Human Decision
```bash
arena finalize \
  --session-id 123e4567-e89b-12d3-a456-426614174000 \
  --decision approve \
  --reasoning "Implementation meets requirements and follows security best practices"
```

### Checking for Spec Drift
```bash
arena drift-check \
  --specs "docs/api_spec.yaml" \
  --impls "src/api/" \
  --agent gpt-4-turbo
```