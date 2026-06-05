use crate::session::types::*;

/// Compute council consensus from multiple evaluations
pub fn compute_consensus(evaluations: &[CouncilEvaluation]) -> ConsensusResult {
    if evaluations.is_empty() {
        return ConsensusResult {
            recommendation: Recommendation::Escalate,
            confidence: 0.0,
            agent_scores: vec![],
            reasoning: "No council evaluations".to_string(),
        };
    }

    // Aggregate scores across all council evaluations
    let mut agent_score_map: std::collections::HashMap<String, Vec<f64>> =
        std::collections::HashMap::new();

    for evaluation in evaluations {
        for agent_score in &evaluation.evaluations {
            agent_score_map
                .entry(agent_score.agent_id.clone())
                .or_default()
                .push(agent_score.score);
        }
    }

    // Average scores per agent
    let mut agent_scores: Vec<AgentConsensusScore> = agent_score_map
        .into_iter()
        .map(|(agent_id, scores)| {
            let avg = scores.iter().sum::<f64>() / scores.len() as f64;
            let variance = scores.iter().map(|s| (s - avg).powi(2)).sum::<f64>() / scores.len() as f64;
            AgentConsensusScore {
                agent_id,
                average_score: avg,
                score_variance: variance,
            }
        })
        .collect();

    agent_scores.sort_by(|a, b| b.average_score.partial_cmp(&a.average_score).unwrap());

    // Determine consensus recommendation
    let recommendations: std::collections::HashMap<String, usize> =
        evaluations.iter().fold(std::collections::HashMap::new(), |mut acc, e| {
            let key = match &e.consensus_recommendation {
                Recommendation::Approve => "Approve",
                Recommendation::Escalate => "Escalate",
                Recommendation::Revise(_) => "Revise",
            };
            *acc.entry(key.to_string()).or_default() += 1;
            acc
        });

    let (recommendation, confidence) = if let Some((rec, count)) = recommendations
        .into_iter()
        .max_by_key(|(_, count)| *count)
    {
        let conf = count as f64 / evaluations.len() as f64;
        let parsed = match rec.as_str() {
            "Approve" => Recommendation::Approve,
            "Escalate" => Recommendation::Escalate,
            "Revise" => {
                // Collect all agents to revise across council evaluations
                let mut agents_to_revise = vec![];
                for e in evaluations {
                    if let Recommendation::Revise(agents) = &e.consensus_recommendation {
                        for a in agents {
                            if !agents_to_revise.contains(a) {
                                agents_to_revise.push(a.clone());
                            }
                        }
                    }
                }
                Recommendation::Revise(agents_to_revise)
            }
            _ => Recommendation::Escalate,
        };
        (parsed, conf)
    } else {
        (Recommendation::Escalate, 0.0)
    };

    // Combine reasoning from all evaluations
    let reasoning = evaluations
        .iter()
        .map(|e| format!("[{}] {}", e.evaluator_name, e.reasoning))
        .collect::<Vec<_>>()
        .join("\n\n");

    ConsensusResult {
        recommendation,
        confidence,
        agent_scores,
        reasoning,
    }
}

#[derive(Debug, Clone)]
pub struct ConsensusResult {
    pub recommendation: Recommendation,
    pub confidence: f64,
    pub agent_scores: Vec<AgentConsensusScore>,
    pub reasoning: String,
}

#[derive(Debug, Clone)]
pub struct AgentConsensusScore {
    pub agent_id: AgentId,
    pub average_score: f64,
    pub score_variance: f64,
}

/// Check if council consensus indicates auto-approval is safe
pub fn can_auto_approve(consensus: &ConsensusResult, threshold: f64) -> bool {
    matches!(consensus.recommendation, Recommendation::Approve)
        && consensus.confidence >= threshold
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn make_evaluation(
        evaluator_id: &str,
        agent_scores: Vec<(&str, f64)>,
        recommendation: Recommendation,
    ) -> CouncilEvaluation {
        CouncilEvaluation {
            evaluator_id: evaluator_id.to_string(),
            evaluator_name: evaluator_id.to_string(),
            evaluations: agent_scores
                .iter()
                .map(|(id, score)| AgentScore {
                    agent_id: id.to_string(),
                    score: *score,
                    criteria_scores: vec![],
                    critique: "Good".to_string(),
                })
                .collect(),
            consensus_recommendation: recommendation,
            reasoning: "Test evaluation".to_string(),
            created_at: Utc::now(),
        }
    }

    #[test]
    fn test_consensus_unanimous_approve() {
        let evaluations = vec![
            make_evaluation("c1", vec![("a1", 0.9), ("a2", 0.7)], Recommendation::Approve),
            make_evaluation("c2", vec![("a1", 0.85), ("a2", 0.75)], Recommendation::Approve),
            make_evaluation("c3", vec![("a1", 0.88), ("a2", 0.72)], Recommendation::Approve),
        ];

        let result = compute_consensus(&evaluations);
        assert!(matches!(result.recommendation, Recommendation::Approve));
        assert!(result.confidence >= 0.99);
        assert_eq!(result.agent_scores.len(), 2);
        // a1 should be ranked higher
        assert!(result.agent_scores[0].average_score > result.agent_scores[1].average_score);
    }

    #[test]
    fn test_consensus_split_decision() {
        let evaluations = vec![
            make_evaluation("c1", vec![("a1", 0.9)], Recommendation::Approve),
            make_evaluation("c2", vec![("a1", 0.3)], Recommendation::Escalate),
            make_evaluation("c3", vec![("a1", 0.5)], Recommendation::Escalate),
        ];

        let result = compute_consensus(&evaluations);
        assert!(matches!(result.recommendation, Recommendation::Escalate));
        assert!((result.confidence - 0.666).abs() < 0.01);
    }

    #[test]
    fn test_empty_evaluations() {
        let result = compute_consensus(&[]);
        assert!(matches!(result.recommendation, Recommendation::Escalate));
        assert!((result.confidence).abs() < 0.001);
    }

    #[test]
    fn test_can_auto_approve() {
        let consensus = ConsensusResult {
            recommendation: Recommendation::Approve,
            confidence: 1.0,
            agent_scores: vec![],
            reasoning: "Unanimous".to_string(),
        };
        assert!(can_auto_approve(&consensus, 0.8));

        let low_confidence = ConsensusResult {
            recommendation: Recommendation::Approve,
            confidence: 0.5,
            agent_scores: vec![],
            reasoning: "Low confidence".to_string(),
        };
        assert!(!can_auto_approve(&low_confidence, 0.8));
    }
}
