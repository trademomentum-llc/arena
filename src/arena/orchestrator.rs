use crate::session::*;
use crate::adapters::*;
use std::sync::Arc;
use std::time::Instant;
use tokio::task::JoinSet;
use tracing::{info, error};

/// Arena orchestrator: coordinates task dispatch and response collection
pub struct ArenaOrchestrator<S: SessionStore> {
    session_manager: SessionManager<S>,
    registry: Arc<AgentRegistry>,
}

impl<S: SessionStore> ArenaOrchestrator<S> {
    pub fn new(session_manager: SessionManager<S>, registry: Arc<AgentRegistry>) -> Self {
        ArenaOrchestrator {
            session_manager,
            registry,
        }
    }

    pub fn session_manager(&self) -> &SessionManager<S> {
        &self.session_manager
    }

    /// Dispatch a task to all worker agents in a session and collect responses
    pub async fn dispatch_and_collect(
        &self,
        session_id: &SessionId,
        agent_request: AgentRequest,
    ) -> Result<ArenaSession, ArenaError> {
        // Transition to dispatched
        let session = self.session_manager.dispatch_task(session_id)?;
        info!(
            session_id = %session_id,
            agents = session.worker_agents.len(),
            "Session dispatched"
        );

        // Build agent requests
        let mut tasks = JoinSet::new();
        for agent in &session.worker_agents {
            let adapter: Box<dyn AgentAdapter> = self
                .registry
                .get(&agent.id)
                .ok_or_else(|| ArenaError::AgentNotFound(agent.id.clone()))?
                .box_clone();

            let req = agent_request.clone();
            let agent_id = agent.id.clone();
            let agent_name = agent.name.clone();

            tasks.spawn(async move {
                let _start = Instant::now();
                match adapter.request(&req).await {
                    Ok(output) => Ok(AgentResponse {
                        agent_id,
                        agent_name,
                        content: output.content,
                        structured: output.structured,
                        latency_ms: output.usage.latency_ms,
                        cost_cents: output.usage.cost_cents,
                        created_at: chrono::Utc::now(),
                    }),
                    Err(e) => Err((agent_id, e)),
                }
            });
        }

        // Collect responses
        while let Some(result) = tasks.join_next().await {
            match result {
                Ok(Ok(response)) => {
                    info!(
                        agent_id = %response.agent_id,
                        latency_ms = response.latency_ms,
                        cost = response.cost_cents,
                        "Agent response received"
                    );
                    self.session_manager
                        .add_response(session_id, response)
                        .map_err(|e| ArenaError::Store(e))?;
                }
                Ok(Err((agent_id, error))) => {
                    error!(agent_id = %agent_id, error = %error, "Agent request failed");
                    // Create a failure response so the session doesn't hang
                    self.session_manager
                        .add_response(
                            session_id,
                            AgentResponse {
                                agent_id,
                                agent_name: "unknown".to_string(),
                                content: format!("[Agent error: {}]", error),
                                structured: None,
                                latency_ms: 0,
                                cost_cents: 0.0,
                                created_at: chrono::Utc::now(),
                            },
                        )
                        .map_err(|e| ArenaError::Store(e))?;
                }
                Err(e) => {
                    error!(error = %e, "Task join failed");
                }
            }
        }

        // Return updated session
        Ok(self.session_manager.get_session(session_id)?)
    }

    /// Dispatch council evaluation in council mode
    pub async fn dispatch_council_evaluation(
        &self,
        session_id: &SessionId,
        worker_responses: &[AgentResponse],
    ) -> Result<ArenaSession, ArenaError> {
        let session = self.session_manager.get_session(session_id)?;

        if session.mode != OrchestrationMode::Council {
            return Err(ArenaError::NotCouncilMode);
        }

        info!(
            session_id = %session_id,
            council_agents = session.council_agents.len(),
            "Council evaluation dispatched"
        );

        let council_prompt = self.build_council_prompt(worker_responses);

        let mut tasks = JoinSet::new();
        for council_agent in &session.council_agents {
            let adapter: Box<dyn AgentAdapter> = self
                .registry
                .get(&council_agent.id)
                .ok_or_else(|| ArenaError::AgentNotFound(council_agent.id.clone()))?
                .box_clone();

            let req = AgentRequest {
                system: "You are a council evaluator in a multi-agent arena. Your job is to evaluate \
                        the responses of worker agents and reach a consensus recommendation."
                    .to_string(),
                prompt: council_prompt.clone(),
                context: None,
                output_format: OutputFormat::Json,
                temperature: 0.0, // Deterministic evaluation
                max_tokens: 2000,
            };

            let evaluator_id = council_agent.id.clone();
            let evaluator_name = council_agent.name.clone();
            let worker_responses = worker_responses.to_vec();

            tasks.spawn(async move {
                match adapter.request(&req).await {
                    Ok(output) => {
                        // Parse council evaluation from response
                        let evaluation = parse_council_response(
                            &output.content,
                            &evaluator_id,
                            &evaluator_name,
                            &worker_responses,
                        );
                        Ok(evaluation)
                    }
                    Err(e) => Err((evaluator_id, e)),
                }
            });
        }

        // Collect council evaluations
        while let Some(result) = tasks.join_next().await {
            match result {
                Ok(Ok(evaluation)) => {
                    info!(
                        evaluator_id = %evaluation.evaluator_id,
                        "Council evaluation received"
                    );
                    self.session_manager
                        .add_evaluation(session_id, evaluation)
                        .map_err(|e| ArenaError::Store(e))?;
                }
                Ok(Err((evaluator_id, error))) => {
                    error!(evaluator_id = %evaluator_id, error = %error, "Council agent failed");
                }
                Err(e) => {
                    error!(error = %e, "Council task join failed");
                }
            }
        }

        self.session_manager.get_session(session_id)
            .map_err(|e| ArenaError::Store(e))
    }

    /// Finalize session with human decision
    pub fn finalize(
        &self,
        session_id: &SessionId,
        decision: HumanDecision,
    ) -> Result<ArenaSession, ArenaError> {
        self.session_manager
            .finalize_with_human_decision(session_id, decision)
            .map_err(|e| ArenaError::Store(e))
    }

    /// Cancel a session
    pub fn cancel(&self, session_id: &SessionId) -> Result<ArenaSession, ArenaError> {
        self.session_manager
            .cancel_session(session_id)
            .map_err(|e| ArenaError::Store(e))
    }

    fn build_council_prompt(&self, worker_responses: &[AgentResponse]) -> String {
        let mut prompt = String::from(
            "# Council Evaluation Request\n\n\
            Evaluate the following responses from worker agents. For each response, provide:\n\
            1. A score (0.0-1.0) for overall quality\n\
            2. Criterion scores for: correctness, completeness, clarity, security\n\
            3. A brief critique\n\
            4. A recommendation: Approve, Escalate, or Revise\n\n",
        );

        for response in worker_responses.iter() {
            prompt.push_str(&format!(
                "## Agent {} ({})\n\n{}\n\n",
                response.agent_name, response.agent_id, response.content
            ));
        }

        prompt.push_str(
            "\nReturn your evaluation as JSON with this structure:\n\
            ```json\n\
            {\n  \"evaluations\": [\n    {\n      \
            \"agent_id\": \"...\",\n      \"score\": 0.0,\n      \
            \"criteria_scores\": [\n        {\n          \"criterion\": \"correctness\",\n          \
            \"score\": 0.0\n        }\n      ],\n      \
            \"critique\": \"...\"\n    }\n  ],\n  \
            \"consensus_recommendation\": \"Approve|Escalate|Revise\",\n  \
            \"reasoning\": \"...\"\n}\n```\n",
        );

        prompt
    }
}

fn parse_council_response(
    content: &str,
    evaluator_id: &str,
    evaluator_name: &str,
    worker_responses: &[AgentResponse],
) -> CouncilEvaluation {
    // Try to parse JSON from the response
    let json_result = extract_json(content);

    match json_result {
        Some(json) => {
            let evaluations: Vec<AgentScore> = json["evaluations"]
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(|e| {
                            Some(AgentScore {
                                agent_id: e["agent_id"].as_str()?.to_string(),
                                score: e["score"].as_f64().unwrap_or(0.0),
                                criteria_scores: e["criteria_scores"]
                                    .as_array()
                                    .map(|arr| {
                                        arr.iter()
                                            .filter_map(|c| {
                                                Some(CriterionScore {
                                                    criterion: c["criterion"]
                                                        .as_str()
                                                        .unwrap_or("")
                                                        .to_string(),
                                                    score: c["score"].as_f64().unwrap_or(0.0),
                                                })
                                            })
                                            .collect()
                                    })
                                    .unwrap_or_default(),
                                critique: e["critique"]
                                    .as_str()
                                    .unwrap_or("")
                                    .to_string(),
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();

            let recommendation = json["consensus_recommendation"]
                .as_str()
                .map(|s| match s {
                    "Approve" => Recommendation::Approve,
                    "Escalate" => Recommendation::Escalate,
                    "Revise" => {
                        // Parse agent IDs to revise
                        let agent_ids: Vec<AgentId> = json["revise_agents"]
                            .as_array()
                            .map(|arr| {
                                arr.iter()
                                    .filter_map(|a| a.as_str().map(|s| s.to_string()))
                                    .collect()
                            })
                            .unwrap_or_default();
                        Recommendation::Revise(agent_ids)
                    }
                    _ => Recommendation::Escalate,
                })
                .unwrap_or(Recommendation::Escalate);

            CouncilEvaluation {
                evaluator_id: evaluator_id.to_string(),
                evaluator_name: evaluator_name.to_string(),
                evaluations,
                consensus_recommendation: recommendation,
                reasoning: json["reasoning"]
                    .as_str()
                    .unwrap_or("")
                    .to_string(),
                created_at: chrono::Utc::now(),
            }
        }
        None => {
            // Fallback: create a basic evaluation from the text content
            CouncilEvaluation {
                evaluator_id: evaluator_id.to_string(),
                evaluator_name: evaluator_name.to_string(),
                evaluations: worker_responses
                    .iter()
                    .map(|r| AgentScore {
                        agent_id: r.agent_id.clone(),
                        score: 0.5,
                        criteria_scores: vec![],
                        critique: content.to_string(),
                    })
                    .collect(),
                consensus_recommendation: Recommendation::Escalate,
                reasoning: content.to_string(),
                created_at: chrono::Utc::now(),
            }
        }
    }
}

fn extract_json(content: &str) -> Option<serde_json::Value> {
    // Try to find JSON object in the content
    // First, try parsing the whole content as JSON
    if let Ok(json) = serde_json::from_str(content) {
        return Some(json);
    }

    // Try to find JSON between ```json and ``` markers
    if let Some(start) = content.find("```json") {
        let after_start = &content[start + 7..];
        if let Some(end) = after_start.find("```") {
            let json_str = after_start[..end].trim();
            if let Ok(json) = serde_json::from_str(json_str) {
                return Some(json);
            }
        }
    }

    // Try to find { ... } block
    if let Some(start) = content.find('{') {
        if let Some(end) = content.rfind('}') {
            let json_str = &content[start..=end];
            if let Ok(json) = serde_json::from_str(json_str) {
                return Some(json);
            }
        }
    }

    None
}

#[derive(Debug, thiserror::Error)]
pub enum ArenaError {
    #[error("Agent not found: {0}")]
    AgentNotFound(String),
    #[error("Session is not in council mode")]
    NotCouncilMode,
    #[error("Store error: {0}")]
    Store(#[from] StoreError),
}
