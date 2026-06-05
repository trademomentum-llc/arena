use clap::{Parser, Subcommand};
use crate::session::*;
use crate::adapters::*;
use crate::adapters::openai::OpenAIAdapter;
use crate::adapters::anthropic::AnthropicAdapter;
use crate::adapters::mock::MockAdapter;
use crate::arena::*;
use crate::metrics::ArenaMetrics;
use std::sync::Arc;

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
            println!("Session created: {}", id);
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
            println!("Session finalized: {}", id);
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
                println!("\nNo drift detected. Implementations match specs.");
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

fn detect_backend(agent_id: &str) -> String {
    match agent_id {
        id if id.starts_with("AC") => "anthropic".to_string(),
        id if id.starts_with("OC") => "openai".to_string(),
        id if id.starts_with("GG") => "google".to_string(),
        id if id.starts_with("QQ") => "qwen".to_string(),
        id if id.starts_with("PP") => "perplexity".to_string(),
        id if id.starts_with("XG") => "xai".to_string(),
        id if id.starts_with("MK") => "moonshot".to_string(),
        id if id.starts_with("ML") => "meta".to_string(),
        id if id.starts_with("HD") => "deepseek".to_string(),
        id if id.starts_with("MX") => "morphlex".to_string(),
        _ => "unknown".to_string(),
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
        _ => Err(format!("Unknown backend for agent: {}", agent_id).into()),
    }
}
