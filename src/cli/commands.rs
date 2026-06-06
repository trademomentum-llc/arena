use clap::{Parser, Subcommand};
use crate::session::*;
use crate::adapters::*;
use crate::adapters::openai::OpenAIAdapter;
use crate::adapters::anthropic::AnthropicAdapter;
use crate::adapters::mock::MockAdapter;
use crate::arena::*;
use crate::metrics::ArenaMetrics;
use std::sync::Arc;

/// Exact output markers that external tools (e.g. arenax Go wrapper) depend on.
/// These are synchronized with the M2 synthesis for stable CLI contract (NFR-9).
pub const SESSION_CREATED_PREFIX: &str = "Session created: ";
pub const SESSION_FINALIZED_PREFIX: &str = "Session finalized: ";
pub const NO_DRIFT_MESSAGE: &str = "No drift detected. Implementations match specs.";
pub const DRIFT_FINDINGS_PREFIX: &str = "Drift findings (";

/// Pure extraction of session ID from arena output (C3-style determinism ported back).
/// Mirrors arenax ExtractUUID for contract fidelity.
pub fn extract_session_id(output: &str) -> Option<String> {
    for line in output.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix(SESSION_CREATED_PREFIX) {
            let cand = rest.trim();
            if is_rfc4122_uuid(cand) {
                return Some(cand.to_string());
            }
        }
        if let Some(rest) = trimmed.strip_prefix(SESSION_FINALIZED_PREFIX) {
            let cand = rest.trim();
            if is_rfc4122_uuid(cand) {
                return Some(cand.to_string());
            }
        }
    }
    None
}

fn is_rfc4122_uuid(s: &str) -> bool {
    let parts: Vec<&str> = s.split('-').collect();
    if parts.len() != 5 { return false; }
    let lens = [8usize, 4, 4, 4, 12];
    for (i, p) in parts.iter().enumerate() {
        if p.len() != lens[i] { return false; }
        if !p.chars().all(|c| c.is_ascii_hexdigit()) { return false; }
    }
    true
}

#[derive(Parser)]
#[command(name = "arena")]
#[command(about = "Multi-agent arena system for checks and balances in development")]
pub struct Cli {
    /// Path to session storage directory
    #[arg(short, long, default_value = "./arena-sessions")]
    pub store_path: String,

    /// Path to config file
    #[arg(short, long)]
    pub config: Option<String>,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Create a new arena session
    Create {
         /// Session type: code-review, implementation, validation, architecture, spec-drift
         #[arg(long)]
         session_type: String,

        /// Orchestration mode: human-in-loop, council
        #[arg(short, long, default_value = "human-in-loop")]
        mode: String,

        /// Worker agents (comma-separated agent IDs)
        #[arg(short, long)]
        workers: String,

         /// Council agents (comma-separated agent IDs, council mode only)
         #[arg(long)]
         council: Option<String>,

        /// Task description
        #[arg(short, long)]
        task: String,

         /// Optional context
         #[arg(short = 'x', long)]
         context: Option<String>,
    },

    /// Run an arena session (dispatch task, collect responses)
    Run {
        /// Session ID
        #[arg(short, long)]
        session_id: String,
    },

    /// List active sessions
    List {
        /// Show all sessions (including finalized)
        #[arg(short, long)]
        all: bool,
    },

    /// View session details
    View {
        /// Session ID
        #[arg(short, long)]
        session_id: String,
    },

    /// Finalize a session with a human decision
    Finalize {
        /// Session ID
        #[arg(short, long)]
        session_id: String,

        /// Decision: approve, approve-mods, reject, iterate
        #[arg(short, long)]
        decision: String,

        /// Reasoning
        #[arg(short, long)]
        reasoning: String,

        /// Selected agent ID (if approving a specific agent's response)
        #[arg(short, long)]
        agent: Option<String>,
    },

    /// Cancel a session
    Cancel {
        /// Session ID
        #[arg(short, long)]
        session_id: String,
    },

    /// Run spec drift check
    DriftCheck {
        /// Spec files (comma-separated paths)
        #[arg(short, long)]
        specs: String,

        /// Implementation files (comma-separated paths)
        #[arg(short, long)]
        impls: String,

        /// Agent to use for analysis
        #[arg(short, long, default_value = "gpt-4-turbo")]
        agent: String,
    },

    /// Export metrics in Prometheus format
    Metrics,

    /// Run council evaluation on an existing session
    CouncilEvaluate {
        /// Session ID
        #[arg(short, long)]
        session_id: String,
    },

    /// Launch basic TUI (ratatui skeleton) for session list/view
    Tui,
}

pub async fn execute(cli: Cli) -> Result<(), Box<dyn std::error::Error>> {
    // Initialize storage
    let store = FileSessionStore::new(&cli.store_path)?;
    let session_manager = SessionManager::new(store);

    // Initialize metrics
    let _metrics = ArenaMetrics::init();

    // Initialize registry (in production, load from config)
    let mut registry = AgentRegistry::new();

    // Register default agents (API keys from env vars)
    if let Ok(key) = std::env::var("OPENAI_API_KEY") {
        registry.register(
            "gpt-4-turbo",
            Box::new(OpenAIAdapter::new(key.clone(), "gpt-4-turbo".to_string())),
        );
        registry.register(
            "gpt-4o",
            Box::new(OpenAIAdapter::new(key, "gpt-4o".to_string())),
        );
    }

    if let Ok(key) = std::env::var("ANTHROPIC_API_KEY") {
        registry.register(
            "claude-3-opus",
            Box::new(AnthropicAdapter::new(
                key.clone(),
                "claude-3-opus-20240229".to_string(),
            )),
        );
        registry.register(
            "claude-3-sonnet",
            Box::new(AnthropicAdapter::new(key, "claude-3-sonnet-20240229".to_string())),
        );
    }

    if let Ok(key) = std::env::var("XAI_API_KEY") {
        registry.register(
            "grok-3",
            Box::new(OpenAIAdapter::with_config(
                key.clone(),
                "grok-3".to_string(),
                Some("https://api.x.ai/v1".to_string()),
                30_000,
                3,
            )),
        );
        registry.register(
            "grok-2",
            Box::new(OpenAIAdapter::with_config(
                key,
                "grok-2".to_string(),
                Some("https://api.x.ai/v1".to_string()),
                30_000,
                3,
            )),
        );
    }

    // Register mock agents for demonstration
    registry.register(
        "mock-reviewer-1",
        Box::new(MockAdapter::new("mock-reviewer-1".to_string(), None)),
    );
    registry.register(
        "mock-reviewer-2",
        Box::new(MockAdapter::new("mock-reviewer-2".to_string(), None)),
    );
    registry.register(
        "mock-implementer-1",
        Box::new(MockAdapter::new("mock-implementer-1".to_string(), None)),
    );
    registry.register(
        "mock-validator-1",
        Box::new(MockAdapter::new("mock-validator-1".to_string(), None)),
    );

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

     let orchestrator = ArenaOrchestrator::new(session_manager, Arc::new(registry));

    match cli.command {
        Commands::Create {
            session_type,
            mode,
            workers,
            council,
            task,
            context,
        } => {
            let context = resolve_context(context)?;   // resolve @file or stdin (FR-11)
            let parsed_type = parse_session_type(&session_type, &task, &context)?;
            let parsed_mode = parse_mode(&mode)?;

            let worker_configs: Vec<AgentConfig> = workers
                .split(',')
                .map(|id| AgentConfig {
                    id: id.trim().to_string(),
                    name: id.trim().to_string(),
                    backend: detect_backend(id.trim()),
                    model: id.trim().to_string(),
                    tier: AgentTier::Worker,
                })
                .collect();

            let council_configs: Vec<AgentConfig> = council
                .map(|c| {
                    c.split(',')
                        .map(|id| AgentConfig {
                            id: id.trim().to_string(),
                            name: id.trim().to_string(),
                            backend: detect_backend(id.trim()),
                            model: id.trim().to_string(),
                            tier: AgentTier::Council,
                        })
                        .collect()
                })
                .unwrap_or_default();

            let session = ArenaSession {
                id: uuid::Uuid::new_v4(),
                session_type: parsed_type,
                mode: parsed_mode,
                phase: SessionPhase::Created,
                worker_agents: worker_configs,
                council_agents: council_configs,
                task: Task {
                    id: format!("task-{}", uuid::Uuid::new_v4()),
                    description: task,
                    context,
                    constraints: vec![],
                    created_at: chrono::Utc::now(),
                },
                responses: vec![],
                evaluations: vec![],
                human_decision: None,
                consistency_score: None,
                drift_findings: None,
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
                finalized_at: None,
            };

            let id = session.id;
            orchestrator.session_manager().create_session(session)?;
            println!("{}{}", SESSION_CREATED_PREFIX, id);
            println!("Use 'arena run --session-id {}' to dispatch tasks", id);
        }

        Commands::Run { session_id } => {
            let id = parse_uuid(&session_id)?;
            let session = orchestrator.session_manager().get_session(&id)?;

            println!("Running session: {}", id);
            println!("Mode: {:?}", session.mode);
            println!("Workers: {}", session.worker_agents.len());

            let agent_request = AgentRequest {
                system: build_system_prompt(&session.session_type),
                prompt: session.task.description.clone(),
                context: session.task.context.clone().map(|c| {
                    vec![Message {
                        role: MessageRole::User,
                        content: c,
                    }]
                }),
                output_format: OutputFormat::Text,
                temperature: 0.7,
                max_tokens: 4000,
            };

            println!("Dispatching task to {} agents...", session.worker_agents.len());
            println!("About to call dispatch_and_collect");
            
            let session = orchestrator
                .dispatch_and_collect(&id, agent_request)
                .await?;
            println!("Returned from dispatch_and_collect");

            println!("\nResponses collected:");
            for response in &session.responses {
                println!("\n--- {} ---", response.agent_name);
                println!("{}", response.content);
                println!(
                    "  [latency: {}ms, cost: ${:.4}]",
                    response.latency_ms,
                    response.cost_cents / 100.0
                );
            }

            if let Some(score) = session.consistency_score {
                println!("\nConsistency score: {:.2}", score);
            }

            match session.phase {
                SessionPhase::CouncilEvaluating | SessionPhase::CouncilComplete => {
                    println!("\nCouncil evaluation in progress...");
                    let responses = session.responses.clone();
                    let session = orchestrator
                        .dispatch_council_evaluation(&id, &responses)
                        .await?;

                    for eval in &session.evaluations {
                        println!("\n--- Council: {} ---", eval.evaluator_name);
                        println!("Recommendation: {:?}", eval.consensus_recommendation);
                        println!("{}", eval.reasoning);
                    }
                }
                SessionPhase::AwaitingHuman => {
                    println!("\nAwaiting human decision...");
                    println!("Use 'arena finalize --session-id {} --decision approve --reasoning \"...\"'", id);
                }
                _ => {}
            }
        }

        Commands::List { all } => {
            let sessions = if all {
                orchestrator.session_manager().list_all()?
            } else {
                orchestrator.session_manager().list_active()?
            };

            if sessions.is_empty() {
                println!("No sessions found.");
            } else {
                println!("{:<36} {:<15} {:<10} {:<20}", "ID", "Type", "Mode", "Phase");
                println!("{}", "-".repeat(85));
                for s in &sessions {
                    let type_str = match &s.session_type {
                        SessionType::CodeReview { .. } => "code-review",
                        SessionType::Implementation { .. } => "implementation",
                        SessionType::Validation { .. } => "validation",
                        SessionType::Architecture { .. } => "architecture",
                        SessionType::SpecDriftCheck { .. } => "spec-drift",
                    };
                    let mode_str = match s.mode {
                        OrchestrationMode::HumanInLoop => "human",
                        OrchestrationMode::Council => "council",
                    };
                    let phase_str = format!("{:?}", s.phase);
                    println!("{:<36} {:<15} {:<10} {:<20}", s.id, type_str, mode_str, phase_str);
                }
            }
        }

        Commands::View { session_id } => {
            let id = parse_uuid(&session_id)?;
            let session = orchestrator.session_manager().get_session(&id)?;

            println!("Session: {}", session.id);
            println!("Type: {:?}", session.session_type);
            println!("Mode: {:?}", session.mode);
            println!("Phase: {:?}", session.phase);
            println!("\nTask:");
            println!("  Description: {}", session.task.description);
            if let Some(ctx) = &session.task.context {
                println!("  Context: {}", ctx);
            }
            println!("\nWorker Agents:");
            for agent in &session.worker_agents {
                println!("  - {} ({}:{})", agent.name, agent.backend, agent.model);
            }
            if !session.council_agents.is_empty() {
                println!("\nCouncil Agents:");
                for agent in &session.council_agents {
                    println!("  - {} ({}:{})", agent.name, agent.backend, agent.model);
                }
            }
            println!("\nResponses: {}", session.responses.len());
            for response in &session.responses {
                println!("  - {}: {}ms, ${:.4}", response.agent_name, response.latency_ms, response.cost_cents / 100.0);
            }
            if let Some(score) = session.consistency_score {
                println!("\nConsistency Score: {:.2}", score);
            }
            if let Some(decision) = &session.human_decision {
                println!("\nHuman Decision: {:?}", decision.decision);
                println!("  Reasoning: {}", decision.reasoning);
            }
            println!("\nCreated: {}", session.created_at);
            if let Some(finalized) = session.finalized_at {
                println!("Finalized: {}", finalized);
            }
        }

        Commands::Finalize {
            session_id,
            decision,
            reasoning,
            agent,
        } => {
            let id = parse_uuid(&session_id)?;
            let parsed_decision = parse_decision(&decision)?;

            let human_decision = HumanDecision {
                session_id: id,
                decision: parsed_decision,
                reasoning,
                selected_agent: agent,
                merged_content: None,
                decided_at: chrono::Utc::now(),
            };

            let session = orchestrator.finalize(&id, human_decision)?;
            println!("{}{}", SESSION_FINALIZED_PREFIX, id);
            println!("Decision: {:?}", session.human_decision.as_ref().unwrap().decision);
        }

        Commands::Cancel { session_id } => {
            let id = parse_uuid(&session_id)?;
             let _session = orchestrator.cancel(&id)?;
            println!("Session cancelled: {}", id);
        }

        Commands::DriftCheck {
            specs,
            impls,
            agent,
        } => {
            let spec_paths: Vec<String> = specs.split(',').map(|s| s.trim().to_string()).collect();
            let impl_paths: Vec<String> = impls.split(',').map(|s| s.trim().to_string()).collect();

            // Create drift detector with specified agent
            let adapter = create_adapter_for_agent(&agent)?;
            let detector = crate::drift::DriftDetector::new(adapter);

            println!("Running drift check...");
            println!("Specs: {:?}", spec_paths);
            println!("Implementations: {:?}", impl_paths);

            let findings = detector.detect_drift(&spec_paths, &impl_paths).await?;

            if findings.is_empty() {
                println!("\n{}", NO_DRIFT_MESSAGE);
            } else {
                println!("\nDrift findings ({}):", findings.len());
                for finding in &findings {
                    println!("  [{:?}] {}", finding.severity, finding.description);
                }
            }
        }

        Commands::Metrics => {
            let metrics = ArenaMetrics::global();
            println!("{}", metrics.export());
        }

        Commands::CouncilEvaluate { session_id } => {
            let id = parse_uuid(&session_id)?;
            let session = orchestrator.session_manager().get_session(&id)?;

            if session.responses.is_empty() {
                println!("No responses to evaluate. Run the session first.");
                return Ok(());
            }

            println!("Dispatching council evaluation...");
            let responses = session.responses.clone();
            let session = orchestrator
                .dispatch_council_evaluation(&id, &responses)
                .await?;

            for eval in &session.evaluations {
                println!("\n--- Council: {} ---", eval.evaluator_name);
                println!("Recommendation: {:?}", eval.consensus_recommendation);
                for agent_score in &eval.evaluations {
                    println!("  {} ({}) score: {:.2}", eval.evaluator_name, agent_score.agent_id, agent_score.score);
                }
                println!("{}", eval.reasoning);
            }
        }

        Commands::Tui => {
            run_tui_skeleton()?;
        }
    }

    Ok(())
}

fn parse_session_type(
    type_str: &str,
    task: &str,
    context: &Option<String>,
) -> Result<SessionType, Box<dyn std::error::Error>> {
    match type_str {
        "code-review" => Ok(SessionType::CodeReview {
            target: task.to_string(),
            spec_path: context.clone(),
        }),
        "implementation" => Ok(SessionType::Implementation {
            task: task.to_string(),
            constraints: vec![],
        }),
        "validation" => Ok(SessionType::Validation {
            implementation: task.to_string(),
            spec: context.clone().unwrap_or_default(),
        }),
        "architecture" => Ok(SessionType::Architecture {
            question: task.to_string(),
            options: context
                .as_ref()
                .map(|c| c.split(',').map(|s| s.trim().to_string()).collect())
                .unwrap_or_default(),
        }),
        "spec-drift" => Ok(SessionType::SpecDriftCheck {
            spec_paths: context
                .as_ref()
                .map(|c| c.split(',').map(|s| s.trim().to_string()).collect())
                .unwrap_or_default(),
            implementation_paths: vec![task.to_string()],
        }),
        _ => Err(format!("Unknown session type: {}", type_str).into()),
    }
}

fn parse_mode(mode_str: &str) -> Result<OrchestrationMode, Box<dyn std::error::Error>> {
    match mode_str {
        "human-in-loop" | "human" => Ok(OrchestrationMode::HumanInLoop),
        "council" => Ok(OrchestrationMode::Council),
        _ => Err(format!("Unknown mode: {}", mode_str).into()),
    }
}

fn parse_decision(decision_str: &str) -> Result<Decision, Box<dyn std::error::Error>> {
    match decision_str {
        "approve" => Ok(Decision::Approve),
        "approve-mods" | "approve-modifications" => Ok(Decision::ApproveWithModifications),
        "reject" => Ok(Decision::Reject),
        "iterate" => Ok(Decision::Iterate),
        _ => Err(format!("Unknown decision: {}", decision_str).into()),
    }
}

fn parse_uuid(s: &str) -> Result<uuid::Uuid, Box<dyn std::error::Error>> {
    uuid::Uuid::parse_str(s).map_err(|e| e.into())
}

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

fn detect_backend(agent_id: &str) -> String {
    let lower = agent_id.to_lowercase();
    if lower.starts_with("ac") {
        "anthropic".to_string()
    } else if lower.starts_with("oc") {
        "openai".to_string()
    } else if lower.starts_with("gg") {
        "google".to_string()
    } else if lower.starts_with("qq") {
        "qwen".to_string()
    } else if lower.starts_with("pp") {
        "perplexity".to_string()
    } else if lower.starts_with("xg") || lower.starts_with("xai") || lower.starts_with("grok") {
        "xai".to_string()
    } else if lower.starts_with("mk") {
        "moonshot".to_string()
    } else if lower.starts_with("ml") {
        "meta".to_string()
    } else if lower.starts_with("hd") {
        "deepseek".to_string()
    } else if lower.starts_with("mx") {
        "morphlex".to_string()
    } else {
        "unknown".to_string()
    }
}

fn build_system_prompt(session_type: &SessionType) -> String {
    match session_type {
        SessionType::CodeReview { .. } => {
            "You are a code reviewer in a multi-agent arena. Review the provided code diff \
            thoroughly, focusing on correctness, security, performance, and code quality. \
            Provide specific, actionable feedback. Be precise and cite specific code locations."
                .to_string()
        }
        SessionType::Implementation { .. } => {
            "You are an implementation agent in a multi-agent arena. Your task is to propose \
            a high-quality implementation of the given specification. Focus on correctness, \
            security, performance, and adherence to project conventions."
                .to_string()
        }
        SessionType::Validation { .. } => {
            "You are a validation agent in a multi-agent arena. Your task is to review an \
            implementation against its specification and identify any bugs, edge cases, or \
            deviations from the spec."
                .to_string()
        }
        SessionType::Architecture { .. } => {
            "You are an architecture advisor in a multi-agent arena. Analyze the proposed \
            options and provide a well-reasoned recommendation considering trade-offs, \
            maintainability, and alignment with project goals."
                .to_string()
        }
        SessionType::SpecDriftCheck { .. } => {
            "You are a spec compliance auditor. Compare implementations against specifications \
            and identify any drift. Be thorough but precise in your analysis."
                .to_string()
        }
    }
}

/// Very basic ratatui TUI skeleton (as requested).
/// Launch with `arena tui`.
/// Navigation: arrows, r=refresh, v/enter=print details, q/esc=quit.
/// Uses default store path (overridable via -s or ARENA_STORE env).
fn run_tui_skeleton() -> Result<(), Box<dyn std::error::Error>> {
    // Minimal implementation to avoid pulling full ratatui complexity into every build path
    // while still providing a working skeleton. In real use, `cargo run -- tui` will exercise it.
    // For a richer version, expand the List + Paragraph layout below.

    use crossterm::{
        event::{self, Event, KeyCode, KeyEventKind},
        execute,
        terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    };
    use ratatui::{
        backend::CrosstermBackend,
        layout::{Constraint, Direction, Layout},
        style::{Modifier, Style},
        text::Span,
        widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
        Terminal,
    };

    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let store_path = std::env::var("ARENA_STORE").unwrap_or_else(|_| "./arena-sessions".to_string());
    let mut sessions: Vec<ArenaSession> = vec![];
    let mut list_state = ListState::default();
    list_state.select(Some(0));
    let mut should_quit = false;

    while !should_quit {
        // best-effort reload
        if let Ok(store) = FileSessionStore::new(&store_path) {
            if let Ok(list) = store.list_sessions() {
                sessions = list;
            }
        }

        terminal.draw(|f| {
            let chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
                .split(f.area());

            let items: Vec<ListItem> = sessions.iter().map(|s| {
                let label = format!("{} | {:?} | w={}", s.id, s.phase, s.worker_agents.len());
                ListItem::new(Span::raw(label))
            }).collect();

            let list = List::new(items)
                .block(Block::default().title("arena tui (arrows, r refresh, v view, q quit)").borders(Borders::ALL))
                .highlight_style(Style::default().add_modifier(Modifier::REVERSED));

            f.render_stateful_widget(list, chunks[0], &mut list_state);

            let detail = if let Some(i) = list_state.selected() {
                if let Some(s) = sessions.get(i) {
                    format!("ID: {}\nPhase: {:?}\nMode: {:?}\nTask: {}\nResponses: {}  Consistency: {:?}",
                        s.id, s.phase, s.mode, s.task.description, s.responses.len(), s.consistency_score)
                } else { "select a session".to_string() }
            } else { "no selection".to_string() };

            let p = Paragraph::new(detail).block(Block::default().title("Details").borders(Borders::ALL));
            f.render_widget(p, chunks[1]);
        })?;

        if event::poll(std::time::Duration::from_millis(150))? {
            if let Event::Key(k) = event::read()? {
                if k.kind == KeyEventKind::Press {
                    match k.code {
                        KeyCode::Char('q') | KeyCode::Esc => should_quit = true,
                        KeyCode::Char('r') => {},
                        KeyCode::Up => if let Some(i) = list_state.selected() { if i > 0 { list_state.select(Some(i-1)); } },
                        KeyCode::Down => {
                            let i = list_state.selected().unwrap_or(0);
                            if i + 1 < sessions.len() { list_state.select(Some(i+1)); }
                        }
                        KeyCode::Char('v') | KeyCode::Enter => {
                            if let Some(i) = list_state.selected() {
                                if let Some(s) = sessions.get(i) {
                                    println!("\n[TUI] Selected:\n{:#?}\n", s);
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

#[cfg(test)]
mod contract_tests {
    use super::*;

    #[test]
    fn test_extract_session_id_exact_markers() {
        let out = "Session created: 123e4567-e89b-12d3-a456-426614174000\nUse 'arena run ...'";
        assert_eq!(extract_session_id(out), Some("123e4567-e89b-12d3-a456-426614174000".to_string()));

        let fin = "Session finalized: 123e4567-e89b-12d3-a456-426614174000\nDecision: Approve";
        assert_eq!(extract_session_id(fin), Some("123e4567-e89b-12d3-a456-426614174000".to_string()));
    }

    #[test]
    fn test_extract_session_id_no_secret_leak_in_markers() {
        // Mirrors C3's TestBuildArgv_noSecret spirit: markers themselves never contain secret material
        let secretish = "Session created: sk-1234567890abcdef-ghp_xxx";
        // The parser should still only accept valid UUIDs after the prefix
        assert_eq!(extract_session_id(secretish), None);
    }

    #[test]
    fn test_rfc4122_uuid() {
        assert!(is_rfc4122_uuid("123e4567-e89b-12d3-a456-426614174000"));
        assert!(!is_rfc4122_uuid("not-a-uuid"));
        assert!(!is_rfc4122_uuid("123e4567-e89b-12d3-a456-42661417400")); // short
    }
}

fn create_adapter_for_agent(
    agent_id: &str,
) -> Result<Box<dyn AgentAdapter>, Box<dyn std::error::Error>> {
    // Create new adapter based on agent ID
    let backend = detect_backend(agent_id);
    match backend.as_str() {
        "openai" => {
            let key = std::env::var("OPENAI_API_KEY")
                .map_err(|_| "OPENAI_API_KEY not set")?;
            Ok(Box::new(OpenAIAdapter::new(key, agent_id.to_string())))
        }
        "anthropic" => {
            let key = std::env::var("ANTHROPIC_API_KEY")
                .map_err(|_| "ANTHROPIC_API_KEY not set")?;
            Ok(Box::new(AnthropicAdapter::new(key, agent_id.to_string())))
        }
        "morphlex" => {
            Err("Morphlex adapter requires model path configuration".into())
        }
        "xai" => {
            let key = std::env::var("XAI_API_KEY")
                .map_err(|_| "XAI_API_KEY not set")?;
            // Use dedicated thin wrapper for clarity (polish from synthesis).
            Ok(Box::new(crate::adapters::new_xai_adapter(key, agent_id.to_string())))
        }
        _ => Err(format!("Unknown backend for agent: {}", agent_id).into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_xai_backend() {
        assert_eq!(detect_backend("XG"), "xai");
        assert_eq!(detect_backend("XG-grok-3"), "xai");
        assert_eq!(detect_backend("xai"), "xai");
        assert_eq!(detect_backend("grok-3"), "xai");
        assert_eq!(detect_backend("grok-2-latest"), "xai");
        assert_eq!(detect_backend("AC-foo"), "anthropic");
        assert_eq!(detect_backend("MX"), "morphlex");
    }
}

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
