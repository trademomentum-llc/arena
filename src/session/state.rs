use crate::session::types::*;
use std::error::Error;
use std::fmt;

#[derive(Debug)]
pub enum StateError {
    InvalidTransition { from: SessionPhase, to: SessionPhase, reason: String },
    SessionFinalized,
    SessionCancelled,
}

impl fmt::Display for StateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StateError::InvalidTransition { from, to, reason } => {
                write!(f, "Invalid transition from {:?} to {:?}: {}", from, to, reason)
            }
            StateError::SessionFinalized => write!(f, "Session is already finalized"),
            StateError::SessionCancelled => write!(f, "Session is cancelled"),
        }
    }
}

impl Error for StateError {}

/// Valid state transitions for arena sessions
pub fn valid_transitions(from: &SessionPhase) -> Vec<SessionPhase> {
    match from {
        SessionPhase::Created => vec![SessionPhase::Dispatched, SessionPhase::Cancelled],
        SessionPhase::Dispatched => vec![SessionPhase::ResponsesComplete, SessionPhase::Cancelled],
        SessionPhase::ResponsesComplete => vec![
            SessionPhase::CouncilEvaluating,
            SessionPhase::AwaitingHuman,
            SessionPhase::Finalized,
            SessionPhase::Cancelled,
        ],
        SessionPhase::CouncilEvaluating => vec![SessionPhase::CouncilComplete],
        SessionPhase::CouncilComplete => vec![SessionPhase::AwaitingHuman, SessionPhase::Finalized],
        SessionPhase::AwaitingHuman => vec![SessionPhase::Finalized, SessionPhase::Cancelled],
        SessionPhase::Finalized => vec![],
        SessionPhase::Cancelled => vec![],
    }
}

pub fn can_transition(from: &SessionPhase, to: &SessionPhase) -> bool {
    valid_transitions(from).contains(to)
}

pub fn validate_transition(session: &ArenaSession, new_phase: SessionPhase) -> Result<(), StateError> {
    if session.phase == SessionPhase::Finalized {
        return Err(StateError::SessionFinalized);
    }
    if session.phase == SessionPhase::Cancelled {
        return Err(StateError::SessionCancelled);
    }
    if !can_transition(&session.phase, &new_phase) {
        return Err(StateError::InvalidTransition {
            from: session.phase.clone(),
            to: new_phase,
            reason: "Transition not in valid transitions table".to_string(),
        });
    }
    Ok(())
}

/// Compute consistency score from agent responses
/// Returns 0.0-1.0 based on semantic agreement across responses
pub fn compute_consistency(responses: &[AgentResponse]) -> f64 {
    if responses.len() <= 1 {
        return 1.0;
    }

    // Simple heuristic: compare structured outputs if available,
    // otherwise use content length similarity as proxy
    let mut agreement_count = 0;
    let mut total_comparisons = 0;

    for i in 0..responses.len() {
        for j in (i + 1)..responses.len() {
            total_comparisons += 1;

            let similarity = if let (Some(a), Some(b)) = (
                &responses[i].structured,
                &responses[j].structured,
            ) {
                structured_similarity(a, b)
            } else {
                text_similarity(&responses[i].content, &responses[j].content)
            };

            if similarity > 0.7 {
                agreement_count += 1;
            }
        }
    }

    if total_comparisons == 0 {
        1.0
    } else {
        agreement_count as f64 / total_comparisons as f64
    }
}

fn structured_similarity(a: &serde_json::Value, b: &serde_json::Value) -> f64 {
    // Compare JSON structures by checking key overlap and value similarity
    match (a.as_object(), b.as_object()) {
        (Some(a_obj), Some(b_obj)) => {
            let all_keys: std::collections::HashSet<_> = a_obj.keys()
                .chain(b_obj.keys())
                .cloned()
                .collect();
            let shared_keys: std::collections::HashSet<_> = a_obj.keys()
                .filter(|k| b_obj.contains_key(*k))
                .cloned()
                .collect();
            
            if all_keys.is_empty() {
                return 1.0;
            }

            let key_overlap = shared_keys.len() as f64 / all_keys.len() as f64;
            
            let mut value_similarity = 0.0;
            let mut compared = 0;
            for key in &shared_keys {
                if let (Some(a_val), Some(b_val)) = (a_obj.get(key), b_obj.get(key)) {
                    if a_val == b_val {
                        value_similarity += 1.0;
                    } else if let (Some(a_str), Some(b_str)) = (a_val.as_str(), b_val.as_str()) {
                        value_similarity += text_similarity(a_str, b_str);
                    } else if let (Some(a_num), Some(b_num)) = (a_val.as_f64(), b_val.as_f64()) {
                        let max = a_num.abs().max(b_num.abs());
                        if max > 0.0 {
                            value_similarity += 1.0 - (a_num - b_num).abs() / max;
                        } else {
                            value_similarity += 1.0;
                        }
                    }
                    compared += 1;
                }
            }

            if compared > 0 {
                value_similarity /= compared as f64;
            }

            key_overlap * 0.5 + value_similarity * 0.5
        }
        _ => {
            if a == b { 1.0 } else { 0.0 }
        }
    }
}

fn text_similarity(a: &str, b: &str) -> f64 {
    // Simple Jaccard similarity on word sets
    let words_a: std::collections::HashSet<_> = a.split_whitespace().collect();
    let words_b: std::collections::HashSet<_> = b.split_whitespace().collect();
    
    let intersection: usize = words_a.intersection(&words_b).count();
    let union = words_a.len() + words_b.len() - intersection;
    
    if union == 0 {
        1.0
    } else {
        intersection as f64 / union as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn test_valid_transitions() {
        assert!(can_transition(&SessionPhase::Created, &SessionPhase::Dispatched));
        assert!(can_transition(&SessionPhase::Created, &SessionPhase::Cancelled));
        assert!(!can_transition(&SessionPhase::Created, &SessionPhase::Finalized));
        
        assert!(can_transition(&SessionPhase::Dispatched, &SessionPhase::ResponsesComplete));
        
        assert!(can_transition(&SessionPhase::ResponsesComplete, &SessionPhase::AwaitingHuman));
        assert!(can_transition(&SessionPhase::ResponsesComplete, &SessionPhase::CouncilEvaluating));
        
        assert!(can_transition(&SessionPhase::AwaitingHuman, &SessionPhase::Finalized));
        assert!(!can_transition(&SessionPhase::Finalized, &SessionPhase::Created));
    }

    #[test]
    fn test_consistency_identical_responses() {
        let responses = vec![
            AgentResponse {
                agent_id: "a1".to_string(),
                agent_name: "Agent 1".to_string(),
                content: "same response".to_string(),
                structured: None,
                latency_ms: 100,
                cost_cents: 0.5,
                created_at: Utc::now(),
            },
            AgentResponse {
                agent_id: "a2".to_string(),
                agent_name: "Agent 2".to_string(),
                content: "same response".to_string(),
                structured: None,
                latency_ms: 150,
                cost_cents: 0.3,
                created_at: Utc::now(),
            },
        ];
        let score = compute_consistency(&responses);
        assert!(score > 0.9, "Identical responses should have high consistency");
    }

    #[test]
    fn test_consistency_different_responses() {
        let responses = vec![
            AgentResponse {
                agent_id: "a1".to_string(),
                agent_name: "Agent 1".to_string(),
                content: "use a hash map for O(1) lookups".to_string(),
                structured: None,
                latency_ms: 100,
                cost_cents: 0.5,
                created_at: Utc::now(),
            },
            AgentResponse {
                agent_id: "a2".to_string(),
                agent_name: "Agent 2".to_string(),
                content: "implement binary search tree with O(log n) access".to_string(),
                structured: None,
                latency_ms: 150,
                cost_cents: 0.3,
                created_at: Utc::now(),
            },
        ];
        let score = compute_consistency(&responses);
        assert!(score < 0.5, "Different responses should have low consistency");
    }

    #[test]
    fn test_single_response_consistency() {
        let responses = vec![AgentResponse {
            agent_id: "a1".to_string(),
            agent_name: "Agent 1".to_string(),
            content: "single".to_string(),
            structured: None,
            latency_ms: 100,
            cost_cents: 0.5,
            created_at: Utc::now(),
        }];
        assert_eq!(compute_consistency(&responses), 1.0);
    }

    #[test]
    fn test_validate_transition_finalized() {
        let session = ArenaSession {
            id: uuid::Uuid::new_v4(),
            session_type: SessionType::CodeReview { target: "PR#1".to_string(), spec_path: None },
            mode: OrchestrationMode::HumanInLoop,
            phase: SessionPhase::Finalized,
            worker_agents: vec![],
            council_agents: vec![],
            task: Task {
                id: "t1".to_string(),
                description: "test".to_string(),
                context: None,
                constraints: vec![],
                created_at: Utc::now(),
            },
            responses: vec![],
            evaluations: vec![],
            human_decision: None,
            consistency_score: None,
            drift_findings: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            finalized_at: Some(Utc::now()),
        };
        assert!(validate_transition(&session, SessionPhase::Created).is_err());
    }
}
