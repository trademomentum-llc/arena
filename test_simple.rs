use arena::session::*;
use arena::adapters::*;
use arena::arena::*;
use std::sync::Arc;

#[tokio::main]
async fn main() {
    println!("Testing basic arena functionality...");
    
    // Initialize storage
    let store = MemSessionStore::new();
    let session_manager = SessionManager::new(store);
    
    // Initialize registry
    let mut registry = AgentRegistry::new();
    
    // Register mock agents
    registry.register(
        "mock-reviewer-1",
        Box::new(MockAdapter::new("mock-reviewer-1".to_string(), Some("Mock response 1".to_string()))),
    );
    registry.register(
        "mock-reviewer-2",
        Box::new(MockAdapter::new("mock-reviewer-2".to_string(), Some("Mock response 2".to_string()))),
    );
    
    let orchestrator = ArenaOrchestrator::new(session_manager, Arc::new(registry));
    
    // Create a test session
    let session = ArenaSession {
        id: uuid::Uuid::new_v4(),
        session_type: SessionType::CodeReview {
            target: "test".to_string(),
            spec_path: None,
        },
        mode: OrchestrationMode::HumanInLoop,
        phase: SessionPhase::Created,
        worker_agents: vec![
            AgentConfig {
                id: "mock-reviewer-1".to_string(),
                name: "Mock Reviewer 1".to_string(),
                backend: "mock".to_string(),
                model: "mock".to_string(),
                tier: AgentTier::Worker,
            },
            AgentConfig {
                id: "mock-reviewer-2".to_string(),
                name: "Mock Reviewer 2".to_string(),
                backend: "mock".to_string(),
                model: "mock".to_string(),
                tier: AgentTier::Worker,
            },
        ],
        council_agents: vec![],
        task: Task {
            id: "task-1".to_string(),
            description: "Test task".to_string(),
            context: None,
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
    println!("Creating session: {}", session_id);
    
    // Create the session
    match orchestrator.session_manager().create_session(session) {
        Ok(_) => println!("Session created successfully"),
        Err(e) => {
            eprintln!("Failed to create session: {}", e);
            return;
        }
    }
    
    // List sessions
    match orchestrator.session_manager().list_active() {
        Ok(sessions) => {
            println!("Active sessions: {}", sessions.len());
            for s in sessions {
                println!("  - {} ({:?})", s.id, s.phase);
            }
        },
        Err(e) => eprintln!("Failed to list sessions: {}", e),
    }
    
    // Get the session
    match orchestrator.session_manager().get_session(&session_id) {
        Ok(session) => {
            println!("Retrieved session: {}", session.id);
            println!("  Phase: {:?}", session.phase);
            println!("  Workers: {}", session.worker_agents.len());
        },
        Err(e) => eprintln!("Failed to get session: {}", e),
    }
    
    println!("Test completed successfully!");
}