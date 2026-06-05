# Arena — Multi-Agent Arena System

Multi-agent arena for checks and balances in development. Orchestrates multiple AI models to review code, propose implementations, validate against specs, and detect drift.

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                     CLI / TUI Interface                      │
├─────────────────────────────────────────────────────────────┤
│                    Arena Orchestrator                         │
│  ┌───────────┐  ┌────────────┐  ┌──────────────────────────┐ │
│  │ Session   │  │ Consensus  │  │  Drift Detector           │ │
│  │ Manager   │  │ Engine     │  │  (spec vs impl check)     │ │
│  └───────────┘  └────────────┘  └──────────────────────────┘ │
├─────────────────────────────────────────────────────────────┤
│                    Agent Adapter Layer                       │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌────────────┐  │
│  │ Morphlex │  │ OpenAI   │  │Anthropic │  │ Future     │  │
│  │ (ONNX)   │  │ (GPT-4)  │  │ (Claude) │  │ Adapters   │  │
│  └──────────┘  └──────────┘  └──────────┘  └────────────┘  │
└─────────────────────────────────────────────────────────────┘
```

## Orchestration Modes

### Human-in-the-Loop
```
Human poses task → Arena dispatches to N agents → 
Results presented to Human → Human decides/merges/approves
```

### Council Mode
```
Task → N worker agents respond → Council agents (higher-tier) evaluate → 
Council reaches consensus → Human reviews (optional override)
```

## Session Types

| Type | Purpose | Flow |
|---|---|---|
| `code-review` | Multi-agent code review | All agents review same diff, findings aggregated |
| `implementation` | Competing implementations | Workers propose, council scores, human picks |
| `validation` | Implementation vs spec validation | One implements, others critique |
| `architecture` | Design decision debates | Council debates, human mediates |
| `spec-drift` | Spec compliance checking | All agents compare impl vs spec docs |

## Quick Start

```bash
cd ~/Projects/arena

# Build
cargo build --release

# Set API keys
export OPENAI_API_KEY="sk-..."
export ANTHROPIC_API_KEY="sk-ant-..."

# Create a session
cargo run -- create \
  --session-type code-review \
  --mode human-in-loop \
  --workers "gpt-4-turbo,claude-3-sonnet" \
  --task "Review PR #42 in pitchfork/backend"

# Run the session
cargo run -- run --session-id <SESSION_ID>

# List sessions
cargo run -- list

# View session details
cargo run -- view --session-id <SESSION_ID>

# Finalize with human decision
cargo run -- finalize \
  --session-id <SESSION_ID> \
  --decision approve \
  --reasoning "GPT-4's review was most thorough"
```

## CLI Commands

```
arena create       Create a new arena session
arena run          Run an arena session (dispatch + collect)
arena list         List sessions
arena view         View session details
arena finalize     Finalize with human decision
arena cancel       Cancel a session
arena drift-check  Run spec drift detection
arena metrics      Export Prometheus metrics
arena council      Run council evaluation
```

## Agent Adapters

| Backend | Models | Setup |
|---|---|---|
| OpenAI | gpt-4-turbo, gpt-4o, gpt-4o-mini, o1 | `OPENAI_API_KEY` |
| Anthropic | claude-3-opus, claude-3-sonnet, claude-3-haiku | `ANTHROPIC_API_KEY` |
| Morphlex | morphlex-local (in-house ONNX) | Compile from trademomentum-llc/apps |

## Metrics

Arena exposes Prometheus metrics at `/metrics` (port 9092):

| Metric | Description |
|---|---|
| `arena_sessions_total` | Total sessions created |
| `arena_active_sessions` | Currently active sessions |
| `arena_agent_responses_total` | Responses by agent |
| `arena_agent_response_latency_seconds` | Latency histogram |
| `arena_agent_response_cost_cents_total` | Cost tracking |
| `arena_consistency_score` | Agreement across agents |
| `arena_council_evaluations_total` | Council recommendations |
| `arena_drift_findings_total` | Spec drift findings |
| `arena_human_decisions_total` | Human decision tracking |

## Integration with Pitchfork

Arena integrates with the Forgejo fork's event handlers (SP5 integration layer):

1. PR opened → trigger `code-review` arena session
2. PR updated → trigger `validation` arena session
3. Architectural issue → trigger `architecture` arena session

The integration hooks are defined in `agent/integration/` in the pitchfork repo.

## Design Properties

- **No autonomous actions**: Arena never modifies code without human approval
- **Deterministic sessions**: Same task + same agents = same outputs (replayable)
- **Transparent reasoning**: All agent reasoning preserved
- **Cost-aware**: Track per-agent spend, prefer cheaper for routine tasks
- **Drift prevention**: Spec anchoring, cross-validation, consistency scoring
