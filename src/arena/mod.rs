/// Arena orchestration: dispatches tasks to agents, manages council evaluation
pub mod orchestrator;
pub mod consensus;

pub use orchestrator::*;
pub use consensus::*;
