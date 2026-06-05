use arena::session::*;
use arena::adapters::*;
use arena::arena::*;
use std::sync::Arc;



#[tokio::main]
async fn main() {
    println!("Dispatch test starting...");
    
    // Initialize storage
    let store = MemSessionStore::new();
    let session_manager = SessionManager::new(store);
    
    // Initialize registry
    let mut registry = AgentRegistry::new();
    
    // Register mock agents with explicit responses
    registry.register(
        "mock-reviewer-1",
        Box::new(MockAdapter::new("mock-reviewer-1".to_string(), Some("Mock response 1".to_string()))),
    );
    registry.register(
        "mock-reviewer-2",
        Box::new(MockAdapter::new("mock-reviewer-2".to_string(), Some("Mock response 2".to_string()))),
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
    println!("Created session with ID: {}", session_id);
    
    // Create the session
    orchestrator.session_manager().create_session(session.clone()).expect("Failed to create session");
    
    // Now test the dispatch_and_collect method directly (this will handle dispatching internally)
    println!("Testing dispatch_and_collect...");
    let agent_request = AgentRequest {
        system: "You are a code reviewer".to_string(),
        prompt: "Review this code".to_string(),
        context: None,
        output_format: OutputFormat::Text,
        temperature: 0.7,
        max_tokens: 4000,
    };
    
    // This is where it might hang
    match orchestrator.dispatch_and_collect(&session_id, agent_request).await {
        Ok(updated_session) => {
            println!("Dispatch and collect successful!");
            println!("Responses received: {}", updated_session.responses.len());
            for (i, response) in updated_session.responses.iter().enumerate() {
                println!("  Response {}: {}", i+1, response.content);
            }
        },
        Err(e) => {
            eprintln!("Dispatch and collect failed: {}", e);
        }
    }
    
    println!("Dispatch test completed!");
}