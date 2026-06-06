use crate::integration::config::*;
use crate::integration::handlers::*;
use crate::session::*;
use crate::arena::*;
use crate::adapters::*;
use std::sync::Arc;
use tracing::{info, warn};

/// Arena integration runner: processes Forgejo webhook events
pub struct ArenaRunner {
    config: ArenaConfig,
    orchestrator: ArenaOrchestrator<FileSessionStore>,
    registry: Arc<AgentRegistry>,
}

impl ArenaRunner {
    pub fn new(config: ArenaConfig) -> Result<Self, Box<dyn std::error::Error>> {
        let store = FileSessionStore::new(&config.store_path)?;
        let session_manager = SessionManager::new(store);

        let mut registry = AgentRegistry::new();

        // Register agents from config
        for agent_def in &config.agents {
            match agent_def.backend.as_str() {
                "openai" => {
                    if let Ok(key) = std::env::var("OPENAI_API_KEY") {
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
                    } else {
                        warn!(agent = %agent_def.id, "OPENAI_API_KEY not set, skipping agent registration");
                    }
                }
                "anthropic" => {
                    if let Ok(key) = std::env::var("ANTHROPIC_API_KEY") {
                        if let Some(url) = &agent_def.base_url {
                            if let Err(e) = crate::adapters::endpoint::validate_local_endpoint(url, false) {
                                warn!(agent = %agent_def.id, error = %e, "Skipping agent: invalid local endpoint");
                                continue;
                            }
                        }
                        registry.register(
                            &agent_def.id,
                            Box::new(crate::adapters::anthropic::AnthropicAdapter::with_config(
                                key,
                                agent_def.model.clone(),
                                agent_def.base_url.clone(),
                                30_000,
                                3,
                            )),
                        );
                        info!(agent = %agent_def.id, "Registered Anthropic agent");
                    } else {
                        warn!(agent = %agent_def.id, "ANTHROPIC_API_KEY not set, skipping agent registration");
                    }
                }
                "morphlex" => {
                    warn!(agent = %agent_def.id, "Morphlex agent requires FFI library, skipping for now");
                }
                "xai" => {
                    if let Ok(key) = std::env::var("XAI_API_KEY") {
                        registry.register(
                            &agent_def.id,
                            Box::new(crate::adapters::new_xai_adapter(key, agent_def.model.clone())),
                        );
                        info!(agent = %agent_def.id, "Registered xAI agent");
                    } else {
                        warn!(agent = %agent_def.id, "XAI_API_KEY not set, skipping agent registration");
                    }
                }
                other => {
                    warn!(backend = %other, "Unknown agent backend, skipping");
                }
            }
        }

          let registry = Arc::new(registry);
          let orchestrator = ArenaOrchestrator::new(session_manager, registry.clone());

        Ok(ArenaRunner {
            config,
            orchestrator,
            registry,
        })
    }

    /// Process a webhook event from Forgejo
    pub async fn process_webhook(
        &self,
        event_type: &str,
        action: &str,
        payload: serde_json::Value,
    ) -> Result<ArenaHandlerResult, Box<dyn std::error::Error>> {
        let forgejo_event = match ForgejoEvent::from_webhook_type(event_type, action) {
            Some(e) => e,
            None => {
                info!(event_type, action, "Ignoring unsupported webhook event");
                return Ok(ArenaHandlerResult {
                    session_id: uuid::Uuid::nil(),
                    event_type: ForgejoEvent::Release, // placeholder
                    success: false,
                    agent_responses: 0,
                    consistency_score: None,
                    council_recommendation: None,
                    error: Some(format!("Unsupported event type: {}", event_type)),
                });
            }
        };

        info!(event = forgejo_event.as_str(), "Processing webhook event");

         match forgejo_event {
             ForgejoEvent::PullRequestOpened
             | ForgejoEvent::PullRequestUpdated
             | ForgejoEvent::PullRequestReviewRequested => {
                 let pr_event: ForgejoPREvent = serde_json::from_value(payload)?;
                 let result = handle_pr_opened(&pr_event, &self.config, &self.orchestrator, &self.registry).await;
                 Ok(result)
             }
             ForgejoEvent::Push => {
                 let push_event: ForgejoPushEvent = serde_json::from_value(payload)?;
                 let result = handle_push(&push_event, &self.config, &self.orchestrator, &self.registry).await;
                 Ok(result)
             }
             ForgejoEvent::IssueOpened => {
                 let issue_event: ForgejoIssueEvent = serde_json::from_value(payload)?;
                 let result = handle_issue_opened(&issue_event, &self.config, &self.orchestrator, &self.registry).await;
                 Ok(result)
             }
             _ => {
                 info!(event = forgejo_event.as_str(), "Event type not yet handled");
                 Ok(ArenaHandlerResult {
                     session_id: uuid::Uuid::nil(),
                     event_type: forgejo_event.clone(),
                     success: false,
                     agent_responses: 0,
                     consistency_score: None,
                     council_recommendation: None,
                     error: Some(format!("Event type not yet implemented: {:?}", forgejo_event)),
                 })
             }
         }
    }

    /// Start the webhook server (HTTP endpoint for Forgejo webhooks)
    pub async fn run_webhook_server(&self, port: u16) -> Result<(), Box<dyn std::error::Error>> {
        info!(port, "Starting arena webhook server");

        // In production, this would start an HTTP server (e.g., axum, actix-web)
        // that listens for Forgejo webhook events at /arena/webhook
        // For now, just log and return

        info!("Webhook server placeholder - use process_webhook() directly for now");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_runner_creation_with_default_config() {
        // This will fail if API keys aren't set, but should not panic
        let config = ArenaConfig::default();
        let result = ArenaRunner::new(config);
        // In test environment without API keys, this should still succeed
        // (just with no registered agents)
        assert!(result.is_ok());
    }
}
