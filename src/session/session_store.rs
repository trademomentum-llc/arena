use crate::session::types::*;
use crate::session::state::*;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum StoreError {
    #[error("Session not found: {0}")]
    NotFound(SessionId),
    #[error("Storage error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("State transition error: {0}")]
    State(#[from] StateError),
}

/// Session storage backend
pub trait SessionStore: Send + Sync {
    fn create_session(&self, session: ArenaSession) -> Result<(), StoreError>;
    fn get_session(&self, id: &SessionId) -> Result<ArenaSession, StoreError>;
    fn update_session(&self, session: &ArenaSession) -> Result<(), StoreError>;
    fn list_sessions(&self) -> Result<Vec<ArenaSession>, StoreError>;
    fn list_active_sessions(&self) -> Result<Vec<ArenaSession>, StoreError>;
}

/// File-based session store (persists to disk as JSONL)
pub struct FileSessionStore {
    store_path: PathBuf,
    cache: Mutex<HashMap<SessionId, ArenaSession>>,
}

impl FileSessionStore {
    pub fn new<P: AsRef<Path>>(store_path: P) -> Result<Self, StoreError> {
        let path = store_path.as_ref().to_path_buf();
        
        // Create directory if it doesn't exist
        if !path.exists() {
            fs::create_dir_all(&path)?;
        }

        let mut store = FileSessionStore {
            store_path: path,
            cache: Mutex::new(HashMap::new()),
        };

        // Load existing sessions
        store.load_from_disk()?;

        Ok(store)
    }

    fn load_from_disk(&mut self) -> Result<(), StoreError> {
        let jsonl_path = self.store_path.join("sessions.jsonl");
        if !jsonl_path.exists() {
            return Ok(());
        }

        let content = fs::read_to_string(&jsonl_path)?;
        let mut cache = self.cache.lock().unwrap();
        
        for line in content.lines() {
            if line.trim().is_empty() {
                continue;
            }
            let session: ArenaSession = serde_json::from_str(line)?;
            cache.insert(session.id, session);
        }

        Ok(())
    }

    fn persist_to_disk(&self, cache: &MutexGuard<'_, HashMap<SessionId, ArenaSession>>) -> Result<(), StoreError> {
        let jsonl_path = self.store_path.join("sessions.jsonl");
        
        let mut content = String::new();
        for session in cache.values() {
            content.push_str(&serde_json::to_string(session)?);
            content.push('\n');
        }
        
        fs::write(&jsonl_path, content)?;
        Ok(())
    }
}

impl SessionStore for FileSessionStore {
    fn create_session(&self, session: ArenaSession) -> Result<(), StoreError> {
        let mut cache = self.cache.lock().unwrap();
        if cache.contains_key(&session.id) {
            return Err(StoreError::Io(std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                format!("Session {} already exists", session.id),
            )));
        }
        cache.insert(session.id, session);
        self.persist_to_disk(&cache)?;
        Ok(())
    }

    fn get_session(&self, id: &SessionId) -> Result<ArenaSession, StoreError> {
        let cache = self.cache.lock().unwrap();
        cache.get(id).cloned().ok_or(StoreError::NotFound(*id))
    }

    fn update_session(&self, session: &ArenaSession) -> Result<(), StoreError> {
        let mut cache = self.cache.lock().unwrap();
        if !cache.contains_key(&session.id) {
            return Err(StoreError::NotFound(session.id));
        }
        cache.insert(session.id, session.clone());
        self.persist_to_disk(&cache)?;
        Ok(())
    }

    fn list_sessions(&self) -> Result<Vec<ArenaSession>, StoreError> {
        let cache = self.cache.lock().unwrap();
        Ok(cache.values().cloned().collect())
    }

    fn list_active_sessions(&self) -> Result<Vec<ArenaSession>, StoreError> {
        let cache = self.cache.lock().unwrap();
        Ok(cache
            .values()
            .filter(|s| {
                !matches!(
                    s.phase,
                    SessionPhase::Finalized | SessionPhase::Cancelled
                )
            })
            .cloned()
            .collect())
    }
}

/// In-memory session store (for testing)
pub struct MemSessionStore {
    store: Mutex<HashMap<SessionId, ArenaSession>>,
}

impl MemSessionStore {
    pub fn new() -> Self {
        MemSessionStore {
            store: Mutex::new(HashMap::new()),
        }
    }
}

impl SessionStore for MemSessionStore {
    fn create_session(&self, session: ArenaSession) -> Result<(), StoreError> {
        let mut store = self.store.lock().unwrap();
        if store.contains_key(&session.id) {
            return Err(StoreError::Io(std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                format!("Session {} already exists", session.id),
            )));
        }
        store.insert(session.id, session);
        Ok(())
    }

    fn get_session(&self, id: &SessionId) -> Result<ArenaSession, StoreError> {
        let store = self.store.lock().unwrap();
        store.get(id).cloned().ok_or(StoreError::NotFound(*id))
    }

    fn update_session(&self, session: &ArenaSession) -> Result<(), StoreError> {
        let mut store = self.store.lock().unwrap();
        if !store.contains_key(&session.id) {
            return Err(StoreError::NotFound(session.id));
        }
        store.insert(session.id, session.clone());
        Ok(())
    }

    fn list_sessions(&self) -> Result<Vec<ArenaSession>, StoreError> {
        let store = self.store.lock().unwrap();
        Ok(store.values().cloned().collect())
    }

    fn list_active_sessions(&self) -> Result<Vec<ArenaSession>, StoreError> {
        let store = self.store.lock().unwrap();
        Ok(store
            .values()
            .filter(|s| {
                !matches!(
                    s.phase,
                    SessionPhase::Finalized | SessionPhase::Cancelled
                )
            })
            .cloned()
            .collect())
    }
}

/// Session manager: orchestrates session lifecycle with state validation
pub struct SessionManager<S: SessionStore> {
    store: S,
}

impl<S: SessionStore> SessionManager<S> {
    pub fn new(store: S) -> Self {
        SessionManager { store }
    }

    pub fn create_session(&self, session: ArenaSession) -> Result<(), StoreError> {
        self.store.create_session(session)
    }

    pub fn get_session(&self, id: &SessionId) -> Result<ArenaSession, StoreError> {
        self.store.get_session(id)
    }

    pub fn dispatch_task(&self, id: &SessionId) -> Result<ArenaSession, StoreError> {
        let mut session = self.store.get_session(id)?;
        validate_transition(&session, SessionPhase::Dispatched)?;
        session.phase = SessionPhase::Dispatched;
        session.updated_at = chrono::Utc::now();
        self.store.update_session(&session)?;
        Ok(session)
    }

    pub fn add_response(
        &self,
        id: &SessionId,
        response: AgentResponse,
    ) -> Result<ArenaSession, StoreError> {
        let mut session = self.store.get_session(id)?;
        if session.phase != SessionPhase::Dispatched {
            return Err(StoreError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Can only add responses when in Dispatched phase",
            )));
        }

        session.responses.push(response);

        // Check if all worker agents have responded
        let expected = session.worker_agents.len();
        if session.responses.len() >= expected && expected > 0 {
            session.consistency_score = Some(crate::session::state::compute_consistency(
                &session.responses,
            ));

            if session.mode == OrchestrationMode::Council
                && !session.council_agents.is_empty()
            {
                session.phase = SessionPhase::CouncilEvaluating;
            } else {
                session.phase = SessionPhase::AwaitingHuman;
            }
        }

        session.updated_at = chrono::Utc::now();
        self.store.update_session(&session)?;
        self.store.get_session(id)
    }

    pub fn add_evaluation(
        &self,
        id: &SessionId,
        evaluation: CouncilEvaluation,
    ) -> Result<ArenaSession, StoreError> {
        let mut session = self.store.get_session(id)?;
        if session.phase != SessionPhase::CouncilEvaluating {
            return Err(StoreError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Can only add evaluations when in CouncilEvaluating phase",
            )));
        }

        session.evaluations.push(evaluation);

        // Check if all council agents have evaluated
        let expected = session.council_agents.len();
        if session.evaluations.len() >= expected && expected > 0 {
            session.phase = SessionPhase::CouncilComplete;
        }

        session.updated_at = chrono::Utc::now();
        self.store.update_session(&session)?;
        self.store.get_session(id)
    }

    pub fn finalize_with_human_decision(
        &self,
        id: &SessionId,
        decision: HumanDecision,
    ) -> Result<ArenaSession, StoreError> {
        let mut session = self.store.get_session(id)?;
        validate_transition(&session, SessionPhase::Finalized)?;
        session.human_decision = Some(decision);
        session.phase = SessionPhase::Finalized;
        session.updated_at = chrono::Utc::now();
        session.finalized_at = Some(chrono::Utc::now());
        self.store.update_session(&session)?;
        self.store.get_session(id)
    }

    pub fn cancel_session(&self, id: &SessionId) -> Result<ArenaSession, StoreError> {
        let mut session = self.store.get_session(id)?;
        validate_transition(&session, SessionPhase::Cancelled)?;
        session.phase = SessionPhase::Cancelled;
        session.updated_at = chrono::Utc::now();
        session.finalized_at = Some(chrono::Utc::now());
        self.store.update_session(&session)?;
        self.store.get_session(id)
    }

    pub fn list_active(&self) -> Result<Vec<ArenaSession>, StoreError> {
        self.store.list_active_sessions()
    }

    pub fn list_all(&self) -> Result<Vec<ArenaSession>, StoreError> {
        self.store.list_sessions()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn test_session() -> ArenaSession {
        ArenaSession {
            id: uuid::Uuid::new_v4(),
            session_type: SessionType::CodeReview {
                target: "PR#1".to_string(),
                spec_path: None,
            },
            mode: OrchestrationMode::HumanInLoop,
            phase: SessionPhase::Created,
            worker_agents: vec![
                AgentConfig {
                    id: "a1".to_string(),
                    name: "GPT-4".to_string(),
                    backend: "openai".to_string(),
                    model: "gpt-4-turbo".to_string(),
                    tier: AgentTier::Worker,
                },
                AgentConfig {
                    id: "a2".to_string(),
                    name: "Claude".to_string(),
                    backend: "anthropic".to_string(),
                    model: "claude-3-opus".to_string(),
                    tier: AgentTier::Worker,
                },
            ],
            council_agents: vec![],
            task: Task {
                id: "t1".to_string(),
                description: "Review PR #1".to_string(),
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
            finalized_at: None,
        }
    }

    #[test]
    fn test_create_and_get_session() {
        let store = MemSessionStore::new();
        let manager = SessionManager::new(store);
        let session = test_session();
        let id = session.id;
        manager.create_session(session).unwrap();
        let retrieved = manager.get_session(&id).unwrap();
        assert_eq!(retrieved.id, id);
    }

    #[test]
    fn test_dispatch_task() {
        let store = MemSessionStore::new();
        let manager = SessionManager::new(store);
        let session = test_session();
        let id = session.id;
        manager.create_session(session).unwrap();
        let dispatched = manager.dispatch_task(&id).unwrap();
        assert_eq!(dispatched.phase, SessionPhase::Dispatched);
    }

    #[test]
    fn test_add_response_triggers_awaiting_human() {
        let store = MemSessionStore::new();
        let manager = SessionManager::new(store);
        let session = test_session();
        let id = session.id;
        manager.create_session(session).unwrap();
        manager.dispatch_task(&id).unwrap();

        fn make_response(agent_id: &str, name: &str) -> AgentResponse {
            AgentResponse {
                agent_id: agent_id.to_string(),
                agent_name: name.to_string(),
                content: "review findings".to_string(),
                structured: None,
                latency_ms: 100,
                cost_cents: 0.5,
                created_at: Utc::now(),
            }
        }

        manager.add_response(&id, make_response("a1", "GPT-4")).unwrap();
        let session = manager.get_session(&id).unwrap();
        assert_eq!(session.phase, SessionPhase::Dispatched); // Still waiting

        manager.add_response(&id, make_response("a2", "Claude")).unwrap();
        let session = manager.get_session(&id).unwrap();
        assert_eq!(session.phase, SessionPhase::AwaitingHuman);
        assert!(session.consistency_score.is_some());
    }

    #[test]
    fn test_council_mode_transitions() {
        let store = MemSessionStore::new();
        let manager = SessionManager::new(store);
        let mut session = test_session();
        session.mode = OrchestrationMode::Council;
        session.council_agents = vec![
            AgentConfig {
                id: "c1".to_string(),
                name: "GPT-4o".to_string(),
                backend: "openai".to_string(),
                model: "gpt-4o".to_string(),
                tier: AgentTier::Council,
            },
        ];
        let id = session.id;
        manager.create_session(session).unwrap();
        manager.dispatch_task(&id).unwrap();

        fn make_response(agent_id: &str, name: &str) -> AgentResponse {
            AgentResponse {
                agent_id: agent_id.to_string(),
                agent_name: name.to_string(),
                content: "implementation".to_string(),
                structured: None,
                latency_ms: 100,
                cost_cents: 0.5,
                created_at: Utc::now(),
            }
        }

        manager.add_response(&id, make_response("a1", "GPT-4")).unwrap();
        manager.add_response(&id, make_response("a2", "Claude")).unwrap();
        let session = manager.get_session(&id).unwrap();
        assert_eq!(session.phase, SessionPhase::CouncilEvaluating);

        let evaluation = CouncilEvaluation {
            evaluator_id: "c1".to_string(),
            evaluator_name: "GPT-4o".to_string(),
            evaluations: vec![],
            consensus_recommendation: Recommendation::Approve,
            reasoning: "Good".to_string(),
            created_at: Utc::now(),
        };
        manager.add_evaluation(&id, evaluation).unwrap();
        let session = manager.get_session(&id).unwrap();
        assert_eq!(session.phase, SessionPhase::CouncilComplete);
    }

    #[test]
    fn test_finalize_with_human_decision() {
        let store = MemSessionStore::new();
        let manager = SessionManager::new(store);
        let session = test_session();
        let id = session.id;
        manager.create_session(session).unwrap();
        manager.dispatch_task(&id).unwrap();

        fn make_response(agent_id: &str, name: &str) -> AgentResponse {
            AgentResponse {
                agent_id: agent_id.to_string(),
                agent_name: name.to_string(),
                content: "review".to_string(),
                structured: None,
                latency_ms: 100,
                cost_cents: 0.5,
                created_at: Utc::now(),
            }
        }

        manager.add_response(&id, make_response("a1", "GPT-4")).unwrap();
        manager.add_response(&id, make_response("a2", "Claude")).unwrap();

        let decision = HumanDecision {
            session_id: id,
            decision: Decision::Approve,
            reasoning: "Looks good".to_string(),
            selected_agent: Some("a1".to_string()),
            merged_content: None,
            decided_at: Utc::now(),
        };

        let finalized = manager.finalize_with_human_decision(&id, decision).unwrap();
        assert_eq!(finalized.phase, SessionPhase::Finalized);
        assert!(finalized.finalized_at.is_some());
        assert!(finalized.human_decision.is_some());
    }

    #[test]
    fn test_cancel_session() {
        let store = MemSessionStore::new();
        let manager = SessionManager::new(store);
        let session = test_session();
        let id = session.id;
        manager.create_session(session).unwrap();
        manager.dispatch_task(&id).unwrap();

        let cancelled = manager.cancel_session(&id).unwrap();
        assert_eq!(cancelled.phase, SessionPhase::Cancelled);
        assert!(cancelled.finalized_at.is_some());
    }

    #[test]
    fn test_invalid_transition_after_cancel() {
        let store = MemSessionStore::new();
        let manager = SessionManager::new(store);
        let session = test_session();
        let id = session.id;
        manager.create_session(session).unwrap();
        manager.cancel_session(&id).unwrap();

        assert!(manager.dispatch_task(&id).is_err());
    }

    #[test]
    fn test_list_active_sessions() {
        let store = MemSessionStore::new();
        let manager = SessionManager::new(store);

        let mut s1 = test_session();
        s1.id = uuid::Uuid::new_v4();
        let id1 = s1.id;
        manager.create_session(s1).unwrap();
        manager.dispatch_task(&id1).unwrap();

        let s2 = test_session();
        let id2 = s2.id;
        manager.create_session(s2).unwrap();
        manager.cancel_session(&id2).unwrap();

        let active = manager.list_active().unwrap();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].id, id1);
    }
}
