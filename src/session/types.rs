use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::{DateTime, Utc};

/// Unique identifier for an arena session
pub type SessionId = Uuid;

/// Unique identifier for an agent within a session
pub type AgentId = String;

/// Defines the orchestration mode for an arena session
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum OrchestrationMode {
    /// Human poses task, reviews all agent responses, makes final decision
    HumanInLoop,
    /// Worker agents produce solutions, council agents evaluate and reach consensus
    Council,
}

/// Defines the type of arena session
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SessionType {
    /// Multiple agents review the same code diff, findings aggregated
    CodeReview {
        /// Path to diff or PR reference
        target: String,
        /// Base spec to check against
        spec_path: Option<String>,
    },
    /// Worker agents propose implementations, council scores, human picks
    Implementation {
        /// Task description
        task: String,
        /// Constraints (performance, security, etc.)
        constraints: Vec<String>,
    },
    /// One agent implements, others critique and find issues
    Validation {
        /// Implementation to validate
        implementation: String,
        /// Expected behavior spec
        spec: String,
    },
    /// Council debates, human mediates (architectural decisions)
    Architecture {
        /// Decision to be made
        question: String,
        /// Options under consideration
        options: Vec<String>,
    },
    /// All agents compare implementation vs spec doc to detect drift
    SpecDriftCheck {
        /// Spec file(s) to check against
        spec_paths: Vec<String>,
        /// Implementation paths to verify
        implementation_paths: Vec<String>,
    },
}

/// A task dispatched to agents in an arena session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub description: String,
    pub context: Option<String>,
    pub constraints: Vec<String>,
    pub created_at: DateTime<Utc>,
}

/// Response from a single agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentResponse {
    pub agent_id: AgentId,
    pub agent_name: String,
    pub content: String,
    /// Optional structured output (code, findings, scores, etc.)
    pub structured: Option<serde_json::Value>,
    pub latency_ms: u64,
    pub cost_cents: f64,
    pub created_at: DateTime<Utc>,
}

/// Council evaluation of agent responses
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CouncilEvaluation {
    pub evaluator_id: AgentId,
    pub evaluator_name: String,
    pub evaluations: Vec<AgentScore>,
    pub consensus_recommendation: Recommendation,
    pub reasoning: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentScore {
    pub agent_id: AgentId,
    pub score: f64, // 0.0 - 1.0
    pub criteria_scores: Vec<CriterionScore>,
    pub critique: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CriterionScore {
    pub criterion: String,
    pub score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Recommendation {
    /// Auto-approve (council consensus)
    Approve,
    /// Escalate to human (low agreement or high stakes)
    Escalate,
    /// Request revisions from specific agents
    Revise(Vec<AgentId>),
}

/// Human decision on an arena session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HumanDecision {
    pub session_id: SessionId,
    pub decision: Decision,
    pub reasoning: String,
    /// Which agent's response was selected (if applicable)
    pub selected_agent: Option<AgentId>,
    /// Merged content if human combined multiple responses
    pub merged_content: Option<String>,
    pub decided_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Decision {
    Approve,
    ApproveWithModifications,
    Reject,
    Iterate,
}

/// Arena session state machine
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SessionPhase {
    /// Session created, awaiting agent dispatch
    Created,
    /// Task dispatched to agents, awaiting responses
    Dispatched,
    /// All agent responses received
    ResponsesComplete,
    /// Council evaluating (if council mode)
    CouncilEvaluating,
    /// Council evaluation complete
    CouncilComplete,
    /// Awaiting human decision
    AwaitingHuman,
    /// Session finalized
    Finalized,
    /// Session cancelled
    Cancelled,
}

/// Complete arena session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArenaSession {
    pub id: SessionId,
    pub session_type: SessionType,
    pub mode: OrchestrationMode,
    pub phase: SessionPhase,
    /// Agents participating as workers
    pub worker_agents: Vec<AgentConfig>,
    /// Agents participating as council evaluators (council mode only)
    pub council_agents: Vec<AgentConfig>,
    /// Task dispatched to agents
    pub task: Task,
    /// Responses from worker agents
    pub responses: Vec<AgentResponse>,
    /// Council evaluations (council mode only)
    pub evaluations: Vec<CouncilEvaluation>,
    /// Human decision
    pub human_decision: Option<HumanDecision>,
    /// Consistency score (0.0 = total disagreement, 1.0 = unanimous)
    pub consistency_score: Option<f64>,
    /// Spec drift findings (if applicable)
    pub drift_findings: Option<Vec<DriftFinding>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub finalized_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    pub id: AgentId,
    pub name: String,
    /// Backend type (morphlex, openai, anthropic, etc.)
    pub backend: String,
    /// Model identifier
    pub model: String,
    /// Tier: worker or council
    pub tier: AgentTier,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AgentTier {
    Worker,
    Council,
}

/// Finding from spec drift check
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriftFinding {
    pub agent_id: AgentId,
    pub spec_path: String,
    pub implementation_path: String,
    pub description: String,
    pub severity: DriftSeverity,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DriftSeverity {
    Critical,
    High,
    Medium,
    Low,
    Informational,
}
