use serde::{Deserialize, Serialize};

/// Arena integration configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArenaConfig {
    /// Forgejo API base URL
    pub forgejo_url: String,
    /// Forgejo API token
    pub forgejo_token: String,
    /// Arena session storage path
    pub store_path: String,
    /// Agent configurations
    pub agents: Vec<AgentDef>,
    /// Default agents for each event type
    pub event_agents: EventAgentMapping,
    /// Auto-post review comments to Forgejo PRs
    pub auto_post_reviews: bool,
    /// Auto-post review comments to Forgejo PRs
    pub council_mode: bool,
    /// Council agents (for council mode)
    pub council_agents: Vec<String>,
    /// Auto-approve threshold (council confidence >= this => safe to auto-approve)
    pub auto_approve_threshold: f64,
}

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventAgentMapping {
    /// Agents for PR review sessions
    pub pr_review: Vec<String>,
    /// Agents for push/security scan sessions
    pub push_scan: Vec<String>,
    /// Agents for issue triage sessions
    pub issue_triage: Vec<String>,
    /// Agents for manual/triggered sessions
    pub manual: Vec<String>,
}

impl Default for ArenaConfig {
    fn default() -> Self {
        ArenaConfig {
            forgejo_url: "http://localhost:3200".to_string(),
            forgejo_token: String::new(),
            store_path: "./arena-sessions".to_string(),
            agents: vec![
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
            ],
            event_agents: EventAgentMapping {
                pr_review: vec!["gpt-4-turbo".to_string(), "claude-3-sonnet".to_string()],
                push_scan: vec!["gpt-4-turbo".to_string()],
                issue_triage: vec!["claude-3-sonnet".to_string()],
                manual: vec!["gpt-4-turbo".to_string(), "claude-3-sonnet".to_string()],
            },
            auto_post_reviews: true,
            council_mode: false,
            council_agents: vec!["claude-3-opus".to_string(), "gpt-4o".to_string()],
            auto_approve_threshold: 0.9,
        }
    }
}

/// Forgejo webhook event types that trigger arena sessions
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ForgejoEvent {
    PullRequestOpened,
    PullRequestUpdated,
    PullRequestReviewRequested,
    Push,
    IssueOpened,
    IssueComment,
    Release,
}

impl ForgejoEvent {
    pub fn from_webhook_type(type_str: &str, action: &str) -> Option<Self> {
        match (type_str, action) {
            ("pull_request", "opened") => Some(ForgejoEvent::PullRequestOpened),
            ("pull_request", "synchronize") => Some(ForgejoEvent::PullRequestUpdated),
            ("pull_request", "review_requested") => Some(ForgejoEvent::PullRequestReviewRequested),
            ("push", _) => Some(ForgejoEvent::Push),
            ("issues", "opened") => Some(ForgejoEvent::IssueOpened),
            ("issue_comment", _) => Some(ForgejoEvent::IssueComment),
            ("release", _) => Some(ForgejoEvent::Release),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            ForgejoEvent::PullRequestOpened => "pr_opened",
            ForgejoEvent::PullRequestUpdated => "pr_updated",
            ForgejoEvent::PullRequestReviewRequested => "pr_review_requested",
            ForgejoEvent::Push => "push",
            ForgejoEvent::IssueOpened => "issue_opened",
            ForgejoEvent::IssueComment => "issue_comment",
            ForgejoEvent::Release => "release",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = ArenaConfig::default();
        assert!(!config.forgejo_url.is_empty());
        assert_eq!(config.event_agents.pr_review.len(), 2);
    }

    #[test]
    fn test_event_parsing() {
        assert_eq!(
            ForgejoEvent::from_webhook_type("pull_request", "opened"),
            Some(ForgejoEvent::PullRequestOpened)
        );
        assert_eq!(
            ForgejoEvent::from_webhook_type("push", ""),
            Some(ForgejoEvent::Push)
        );
        assert_eq!(
            ForgejoEvent::from_webhook_type("unknown", ""),
            None
        );
    }

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
}
