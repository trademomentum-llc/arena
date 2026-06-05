#!/bin/bash

# Simple demo script for Arena System that we know works

echo "=== Simple Arena System Demo ==="

# Create storage directory
STORE_DIR="/tmp/arena_simple_demo"
rm -rf "$STORE_DIR"
mkdir -p "$STORE_DIR"

# Create a code review session using mock agents via direct API call (like our tests)
echo "Creating a code review session with mock agents..."
SESSION_ID=$(./target/debug/arena --store-path "$STORE_DIR" create \
  --session-type code-review \
  --mode human-in-loop \
  --workers "mock-reviewer-1,mock-reviewer-2" \
  --task "Review the authentication module implementation" \
  -x "./design_specs/coding_standards.md" 2>&1 | grep "Session created:" | cut -d' ' -f3)

if [ -z "$SESSION_ID" ]; then
  echo "Failed to create session:"
  ./target/debug/arena --store-path "$STORE_DIR" create \
    --session-type code-review \
    --mode human-in-loop \
    --workers "mock-reviewer-1,mock-reviewer-2" \
    --task "Review the authentication module implementation" \
    -x "./design_specs/coding_standards.md"
  exit 1
fi

echo "Created session: $SESSION_ID"

# List sessions
echo "Listing active sessions..."
./target/debug/arena --store-path "$STORE_DIR" list

# Run the session (this will use mock agents to generate responses)
echo "Running session $SESSION_ID..."
./target/debug/arena --store-path "$STORE_DIR" run --session-id $SESSION_ID

# View session details
echo "Viewing session details..."
./target/debug/arena --store-path "$STORE_DIR" view --session-id $SESSION_ID

echo "Demo complete!"