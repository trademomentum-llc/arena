#!/bin/bash

# Demo script for Arena System

echo "=== Arena System Demo ==="

# Create a code review session using mock agents
echo "Creating a code review session with mock agents..."
SESSION_OUTPUT=$(cargo run --bin arena create \
  --session-type code-review \
  --mode human-in-loop \
  --workers "mock-reviewer-1,mock-reviewer-2" \
  --task "Review the authentication module implementation" \
  -x "./design_specs/coding_standards.md" 2>&1)

# Check if the command succeeded
if echo "$SESSION_OUTPUT" | grep -q "Session created:"; then
  SESSION_ID=$(echo "$SESSION_OUTPUT" | grep "Session created:" | cut -d' ' -f3)
  echo "Created session: $SESSION_ID"
else
  echo "Failed to create session:"
  echo "$SESSION_OUTPUT"
  exit 1
fi

# List sessions
echo "Listing active sessions..."
cargo run --bin arena list

# Run the session (this will use mock agents to generate responses)
echo "Running session $SESSION_ID..."
RUN_OUTPUT=$(cargo run --bin arena run --session-id $SESSION_ID 2>&1)
echo "$RUN_OUTPUT"

# View session details
echo "Viewing session details..."
cargo run --bin arena view --session-id $SESSION_ID

echo "Demo complete!"