use arena::adapters::*;
use arena::adapters::morphlex::MorphlexAdapter;

#[tokio::main]
async fn main() {
    println!("Testing Morphlex adapter integration...");
    
    // Create a Morphlex adapter pointing to our compiled library
    let adapter = MorphlexAdapter::new(
        "/Users/nnos/Projects/arena/libs/libmorphlex.so".to_string(),
        "/Users/nnos/Projects/arena/libs/libmorphlex.so".to_string(),
    );
    
    // Test health check
    match adapter.health_check().await {
        Ok(healthy) => {
            if healthy {
                println!("✓ Morphlex adapter health check passed");
            } else {
                println!("✗ Morphlex adapter health check failed (library not found or invalid)");
            }
        }
        Err(e) => {
            println!("✗ Morphlex adapter health check error: {}", e);
        }
    }
    
    // Test a simple request
    let request = AgentRequest {
        system: "You are a helpful assistant.".to_string(),
        prompt: "Say hello in one word.".to_string(),
        context: None,
        output_format: OutputFormat::Text,
        temperature: 0.0,
        max_tokens: 10,
    };
    
    match adapter.request(&request).await {
        Ok(response) => {
            println!("✓ Morphlex adapter request successful:");
            println!("  Response: {}", response.content);
            println!("  Tokens: {} prompt, {} completion", 
                     response.usage.prompt_tokens, response.usage.completion_tokens);
            println!("  Latency: {}ms", response.usage.latency_ms);
        }
        Err(e) => {
            println!("✗ Morphlex adapter request failed: {}", e);
        }
    }
    
    println!("Integration test complete.");
}