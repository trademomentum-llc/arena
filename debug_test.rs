use arena::session::*;
use arena::adapters::*;
use arena::arena::*;
use std::sync::Arc;

fn main() {
    println!("Debug test starting...");
    
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
    
    let orchestrator = ArenaOrchestrator::new(session_manager, Arc::new(registry));
    
    // Create a test session manually
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
    println!("Created session with ID: {}", session_id);
    
    // Try to create the session
    println!("Attempting to create session in storage...");
    match orchestrator.session_manager().create_session(session.clone()) {
        Ok(_) => println!("Session created successfully in storage"),
        Err(e) => {
            eprintln!("Failed to create session in storage: {}", e);
            return;
        }
    }
    
    // Try to get the session
    println!("Attempting to retrieve session from storage...");
    match orchestrator.session_manager().get_session(&session_id) {
        Ok(retrieved) => {
            println!("Successfully retrieved session: {}", retrieved.id);
            println!("Session phase: {:?}", retrieved.phase);
        },
        Err(e) => {
            eprintln!("Failed to retrieve session: {}", e);
            return;
        }
    }
    
    // Try to dispatch task (this might be where it hangs)
    println!("Attempting to dispatch task...");
    match orchestrator.session_manager().dispatch_task(&session_id) {
        Ok(dispatched) => {
            println!("Task dispatched successfully");
            println!("Dispatched session phase: {:?}", dispatched.phase);
        },
        Err(e) => {
            eprintln!("Failed to dispatch task: {}", e);
            return;
        }
    }
    
    println!("Debug test completed successfully!");
}