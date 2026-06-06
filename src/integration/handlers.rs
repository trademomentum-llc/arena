use crate::session::*;
use crate::arena::*;
use crate::adapters::*;
use crate::metrics::ArenaMetrics;
use crate::integration::config::*;
use serde::{Deserialize, Serialize};
use tracing::{info, error};

/// Forgejo PR webhook payload (simplified)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForgejoPREvent {
    pub action: String,
    pub repository: ForgejoRepository,
    pub pull_request: ForgejoPullRequest,
    pub sender: ForgejoUser,
}

/// Forgejo push webhook payload (simplified)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForgejoPushEvent {
    #[serde(rename = "ref")]
    pub ref_name: String,
    pub repository: ForgejoRepository,
    pub commits: Vec<ForgejoCommit>,
    pub sender: ForgejoUser,
}

/// Forgejo issue webhook payload (simplified)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForgejoIssueEvent {
    pub action: String,
    pub repository: ForgejoRepository,
    pub issue: ForgejoIssue,
    pub sender: ForgejoUser,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForgejoRepository {
    pub id: i64,
    pub full_name: String,
    pub html_url: String,
    pub clone_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForgejoPullRequest {
    pub id: i64,
    pub number: i64,
    pub title: String,
    pub body: String,
    pub state: String,
    pub head: ForgejoPRRef,
    pub base: ForgejoPRRef,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForgejoPRRef {
    pub sha: String,
    #[serde(rename = "ref")]
    pub ref_name: String,
    pub repo: ForgejoRepository,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForgejoCommit {
    pub id: String,
    pub message: String,
    pub url: String,
    pub author: ForgejoUser,
    pub added: Vec<String>,
    pub removed: Vec<String>,
    pub modified: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForgejoIssue {
    pub id: i64,
    pub number: i64,
    pub title: String,
    pub body: String,
    pub state: String,
    pub labels: Vec<ForgejoLabel>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForgejoLabel {
    pub name: String,
    pub color: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForgejoUser {
    pub login: String,
    pub id: i64,
}

/// Result of handling a Forgejo event through arena
#[derive(Debug, Clone)]
pub struct ArenaHandlerResult {
    pub session_id: SessionId,
    pub event_type: ForgejoEvent,
    pub success: bool,
    pub agent_responses: usize,
    pub consistency_score: Option<f64>,
    pub council_recommendation: Option<String>,
    pub error: Option<String>,
}

/// Handle a PR opened event - create code review arena session
pub async fn handle_pr_opened(
    event: &ForgejoPREvent,
    config: &ArenaConfig,
    orchestrator: &ArenaOrchestrator<impl SessionStore>,
    _registry: &AgentRegistry,
) -> ArenaHandlerResult {
    info!(
        pr = event.pull_request.number,
        repo = event.repository.full_name,
        "PR opened event received"
    );

    let metrics = ArenaMetrics::global();
    metrics.record_session_created();

     // Create code review session
     let session = ArenaSession {
         id: uuid::Uuid::new_v4(),
         session_type: SessionType::CodeReview {
             target: format!("PR#{} in {}", event.pull_request.number, event.repository.full_name),
             spec_path: Some("./design_specs/coding_standards.md".to_string()),
         },
        mode: if config.council_mode {
            OrchestrationMode::Council
        } else {
            OrchestrationMode::HumanInLoop
        },
        phase: SessionPhase::Created,
        worker_agents: build_agent_configs(&config.event_agents.pr_review, config, AgentTier::Worker),
        council_agents: if config.council_mode {
            build_agent_configs(&config.council_agents, config, AgentTier::Council)
        } else {
            vec![]
        },
        task: Task {
            id: format!("task-{}", uuid::Uuid::new_v4()),
            description: format!(
                "Review pull request #{} in {}.\n\nTitle: {}\nDescription: {}\n\n\
                Focus on:\n\
                - Correctness and edge cases\n\
                - Security vulnerabilities\n\
                - Performance implications\n\
                - Code quality and maintainability\n\
                - Alignment with project conventions",
                event.pull_request.number,
                event.repository.full_name,
                event.pull_request.title,
                event.pull_request.body,
            ),
            context: Some(format!(
                "PR URL: {}/pull/{}",
                event.repository.html_url,
                event.pull_request.number
            )),
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

    let session_id = session.id;

    if let Err(e) = orchestrator.session_manager().create_session(session) {
        error!(error = %e, "Failed to create session");
        return ArenaHandlerResult {
            session_id,
            event_type: ForgejoEvent::PullRequestOpened,
            success: false,
            agent_responses: 0,
            consistency_score: None,
            council_recommendation: None,
            error: Some(e.to_string()),
        };
    }

    // Dispatch task and collect responses
    let agent_request = AgentRequest {
        system: "You are a code reviewer in a multi-agent arena. Review the provided code diff \
                thoroughly, focusing on correctness, security, performance, and code quality. \
                Provide specific, actionable feedback. Be precise and cite specific code locations."
            .to_string(),
        prompt: format!(
            "Review PR #{} in {}. Title: {}. Description: {}",
            event.pull_request.number,
            event.repository.full_name,
            event.pull_request.title,
            event.pull_request.body,
        ),
        context: None,
        output_format: OutputFormat::Text,
        temperature: 0.7,
        max_tokens: 4000,
    };

    match orchestrator.dispatch_and_collect(&session_id, agent_request).await {
        Ok(session) => {
            info!(
                session_id = %session_id,
                responses = session.responses.len(),
                "PR review session completed"
            );

            let consistency = session.consistency_score;
            if let Some(score) = consistency {
                metrics.record_consistency_score("code_review", score);
            }

            // In council mode, dispatch council evaluation
            let mut council_rec = None;
            let final_session = if matches!(session.phase, SessionPhase::CouncilEvaluating) {
                match orchestrator.dispatch_council_evaluation(&session_id, &session.responses).await {
                    Ok(s) => {
                        // Compute consensus
                        let consensus = crate::arena::consensus::compute_consensus(&s.evaluations);
                        council_rec = Some(format!("{:?}", consensus.recommendation));
                        if consensus.confidence >= config.auto_approve_threshold
                            && crate::arena::consensus::can_auto_approve(&consensus, config.auto_approve_threshold)
                        {
                            metrics.record_council_evaluation("auto_approve");
                        } else {
                            metrics.record_council_evaluation("escalate");
                        }
                        s
                    }
                    Err(e) => {
                        error!(error = %e, "Council evaluation failed");
                        session
                    }
                }
            } else {
                session
            };

            // Auto-post reviews if configured
            if config.auto_post_reviews {
                let _ = post_review_comments(&final_session, event, config).await;
            }

            ArenaHandlerResult {
                session_id,
                event_type: ForgejoEvent::PullRequestOpened,
                success: true,
                agent_responses: final_session.responses.len(),
                consistency_score: consistency,
                council_recommendation: council_rec,
                error: None,
            }
        }
        Err(e) => {
            error!(error = %e, "Failed to dispatch PR review session");
            ArenaHandlerResult {
                session_id,
                event_type: ForgejoEvent::PullRequestOpened,
                success: false,
                agent_responses: 0,
                consistency_score: None,
                council_recommendation: None,
                error: Some(e.to_string()),
            }
        }
    }
}

/// Handle a push event - security scan arena session
pub async fn handle_push(
    event: &ForgejoPushEvent,
    config: &ArenaConfig,
    orchestrator: &ArenaOrchestrator<impl SessionStore>,
    _registry: &AgentRegistry,
) -> ArenaHandlerResult {
    info!(
        repo = event.repository.full_name,
        commits = event.commits.len(),
        "Push event received"
    );

    let metrics = ArenaMetrics::global();
    metrics.record_session_created();

    let session = ArenaSession {
        id: uuid::Uuid::new_v4(),
        session_type: SessionType::Validation {
            implementation: format!("Push to {} at {}", event.repository.full_name, event.ref_name),
            spec: "Project security requirements and best practices".to_string(),
        },
        mode: OrchestrationMode::HumanInLoop,
        phase: SessionPhase::Created,
        worker_agents: build_agent_configs(&config.event_agents.push_scan, config, AgentTier::Worker),
        council_agents: vec![],
        task: Task {
            id: format!("task-{}", uuid::Uuid::new_v4()),
            description: format!(
                "Analyze {} commits pushed to {} at {}.\n\nCommits:\n{}\n\n\
                Files changed:\n{}\n\n\
                Focus on security vulnerabilities, unsafe patterns, and potential exploits.",
                event.commits.len(),
                event.repository.full_name,
                event.ref_name,
                event.commits.iter()
                    .map(|c| format!("- {} ({})", c.message, c.id.get(..8).unwrap_or(&c.id)))
                    .collect::<Vec<_>>()
                    .join("\n"),
                event.commits.iter()
                    .flat_map(|c| {
                        c.added.iter().map(|f| format!("+ {}", f))
                            .chain(c.modified.iter().map(|f| format!("M {}", f)))
                            .chain(c.removed.iter().map(|f| format!("- {}", f)))
                    })
                    .collect::<std::collections::HashSet<_>>()
                    .into_iter()
                    .collect::<Vec<_>>()
                    .join("\n"),
            ),
            context: Some(format!(
                "Push URL: {}/compare/{}",
                event.repository.html_url, event.ref_name
            )),
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

    let session_id = session.id;

    if let Err(e) = orchestrator.session_manager().create_session(session) {
        error!(error = %e, "Failed to create push scan session");
        return ArenaHandlerResult {
            session_id,
            event_type: ForgejoEvent::Push,
            success: false,
            agent_responses: 0,
            consistency_score: None,
            council_recommendation: None,
            error: Some(e.to_string()),
        };
    }

    // Dispatch task (simplified - in production would fetch actual diffs)
    let agent_request = AgentRequest {
        system: "You are a security auditor analyzing code changes for vulnerabilities. \
                Focus on injection, XSS, CSRF, auth bypasses, data leaks, and unsafe patterns."
            .to_string(),
        prompt: format!(
            "Analyze {} commits to {} at {} for security issues.",
            event.commits.len(),
            event.repository.full_name,
            event.ref_name,
        ),
        context: None,
        output_format: OutputFormat::Text,
        temperature: 0.3,
        max_tokens: 4000,
    };

    match orchestrator.dispatch_and_collect(&session_id, agent_request).await {
        Ok(session) => {
            info!(
                session_id = %session_id,
                responses = session.responses.len(),
                "Push scan completed"
            );

            let consistency = session.consistency_score;
            if let Some(score) = consistency {
                metrics.record_consistency_score("push_scan", score);
            }

            ArenaHandlerResult {
                session_id,
                event_type: ForgejoEvent::Push,
                success: true,
                agent_responses: session.responses.len(),
                consistency_score: consistency,
                council_recommendation: None,
                error: None,
            }
        }
        Err(e) => {
            error!(error = %e, "Failed to dispatch push scan session");
            ArenaHandlerResult {
                session_id,
                event_type: ForgejoEvent::Push,
                success: false,
                agent_responses: 0,
                consistency_score: None,
                council_recommendation: None,
                error: Some(e.to_string()),
            }
        }
    }
}

/// Handle an issue opened event - auto-triage arena session
pub async fn handle_issue_opened(
    event: &ForgejoIssueEvent,
    config: &ArenaConfig,
    orchestrator: &ArenaOrchestrator<impl SessionStore>,
    _registry: &AgentRegistry,
) -> ArenaHandlerResult {
    info!(
        issue = event.issue.number,
        repo = event.repository.full_name,
        "Issue opened event received"
    );

    let metrics = ArenaMetrics::global();
    metrics.record_session_created();

    let session = ArenaSession {
        id: uuid::Uuid::new_v4(),
        session_type: SessionType::Architecture {
            question: format!("How should we triage and address issue #{}?", event.issue.number),
            options: event.issue.labels.iter().map(|l| l.name.clone()).collect(),
        },
        mode: OrchestrationMode::HumanInLoop,
        phase: SessionPhase::Created,
        worker_agents: build_agent_configs(&config.event_agents.issue_triage, config, AgentTier::Worker),
        council_agents: vec![],
        task: Task {
            id: format!("task-{}", uuid::Uuid::new_v4()),
            description: format!(
                "Triaging issue #{} in {}.\n\nTitle: {}\nDescription: {}\n\n\
                Labels: {}\n\n\
                Provide triage recommendations: severity, priority, suggested approach.",
                event.issue.number,
                event.repository.full_name,
                event.issue.title,
                event.issue.body,
                event.issue.labels.iter().map(|l| l.name.clone()).collect::<Vec<_>>().join(", "),
            ),
            context: Some(format!(
                "Issue URL: {}/issues/{}",
                event.repository.html_url, event.issue.number
            )),
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

    let session_id = session.id;

    if let Err(e) = orchestrator.session_manager().create_session(session) {
        error!(error = %e, "Failed to create issue triage session");
        return ArenaHandlerResult {
            session_id,
            event_type: ForgejoEvent::IssueOpened,
            success: false,
            agent_responses: 0,
            consistency_score: None,
            council_recommendation: None,
            error: Some(e.to_string()),
        };
    }

    let agent_request = AgentRequest {
        system: "You are an issue triage agent. Analyze the issue description and recommend \
                severity, priority, and approach. Be concise and specific."
            .to_string(),
        prompt: format!(
            "Triaging issue #{}: {}. {}",
            event.issue.number, event.issue.title, event.issue.body,
        ),
        context: None,
        output_format: OutputFormat::Text,
        temperature: 0.3,
        max_tokens: 2000,
    };

    match orchestrator.dispatch_and_collect(&session_id, agent_request).await {
        Ok(session) => {
            info!(
                session_id = %session_id,
                responses = session.responses.len(),
                "Issue triage completed"
            );

            let consistency = session.consistency_score;
            if let Some(score) = consistency {
                metrics.record_consistency_score("issue_triage", score);
            }

            ArenaHandlerResult {
                session_id,
                event_type: ForgejoEvent::IssueOpened,
                success: true,
                agent_responses: session.responses.len(),
                consistency_score: consistency,
                council_recommendation: None,
                error: None,
            }
        }
        Err(e) => {
            error!(error = %e, "Failed to dispatch issue triage session");
            ArenaHandlerResult {
                session_id,
                event_type: ForgejoEvent::IssueOpened,
                success: false,
                agent_responses: 0,
                consistency_score: None,
                council_recommendation: None,
                error: Some(e.to_string()),
            }
        }
    }
}

fn build_agent_configs(
    agent_ids: &[String],
    config: &ArenaConfig,
    tier: AgentTier,
) -> Vec<AgentConfig> {
    agent_ids
        .iter()
        .filter_map(|id| {
            config.agents.iter().find(|a| &a.id == id).map(|agent| AgentConfig {
                id: agent.id.clone(),
                name: agent.id.clone(),
                backend: agent.backend.clone(),
                model: agent.model.clone(),
                tier: tier.clone(),
            })
        })
        .collect()
}

/// Post review comments to Forgejo PR
async fn post_review_comments(
    session: &ArenaSession,
    event: &ForgejoPREvent,
    _config: &ArenaConfig,
) -> Result<(), String> {
    info!(
        session_id = %session.id,
        pr = event.pull_request.number,
        "Posting review comments to PR"
    );

    // In production, this would make HTTP calls to Forgejo API to post review comments
    // For now, log the responses
    for response in &session.responses {
        info!(
            agent = response.agent_name,
            "Review from {}: {}",
            response.agent_name,
            response.content.get(..200).unwrap_or(&response.content),
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> ArenaConfig {
        ArenaConfig::default()
    }

    fn test_pr_event() -> ForgejoPREvent {
        ForgejoPREvent {
            action: "opened".to_string(),
            repository: ForgejoRepository {
                id: 1,
                full_name: "test-org/test-repo".to_string(),
                html_url: "https://git.example.com/test-org/test-repo".to_string(),
                clone_url: "https://git.example.com/test-org/test-repo.git".to_string(),
            },
            pull_request: ForgejoPullRequest {
                id: 42,
                number: 42,
                title: "Add authentication module".to_string(),
                body: "This PR adds OAuth2 authentication with PKCE flow.".to_string(),
                state: "open".to_string(),
                head: ForgejoPRRef {
                    sha: "abc123".to_string(),
                    ref_name: "feature/auth".to_string(),
                    repo: ForgejoRepository {
                        id: 1,
                        full_name: "test-org/test-repo".to_string(),
                        html_url: "https://git.example.com/test-org/test-repo".to_string(),
                        clone_url: "https://git.example.com/test-org/test-repo.git".to_string(),
                    },
                },
                base: ForgejoPRRef {
                    sha: "def456".to_string(),
                    ref_name: "main".to_string(),
                    repo: ForgejoRepository {
                        id: 1,
                        full_name: "test-org/test-repo".to_string(),
                        html_url: "https://git.example.com/test-org/test-repo".to_string(),
                        clone_url: "https://git.example.com/test-org/test-repo.git".to_string(),
                    },
                },
            },
            sender: ForgejoUser {
                login: "developer".to_string(),
                id: 100,
            },
        }
    }

    #[test]
    fn test_build_agent_configs() {
        let config = test_config();
        let agents = build_agent_configs(&config.event_agents.pr_review, &config, AgentTier::Worker);
        assert_eq!(agents.len(), 2);
        assert_eq!(agents[0].id, "gpt-4-turbo");
        assert_eq!(agents[1].id, "claude-3-sonnet");
    }

    #[test]
    fn test_pr_event_parsing() {
        let event = test_pr_event();
        assert_eq!(event.pull_request.number, 42);
        assert_eq!(event.action, "opened");
    }
}
