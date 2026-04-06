#!/bin/bash
# Continuously write test audio to shared buffer for HAL plugin testing

set -euo pipefail

echo "Writing continuous test audio to shared buffer..."
echo "Press Ctrl+C to stop"
echo ""

cd "$(dirname "$0")"

# Kill any existing emit_shared_output
pkill -f emit_shared_output 2>/dev/null || true

# Run in a loop
while true; do
    cargo run -p demo-cli --bin emit_shared_output 2>&1 | grep -E "(frames_written|shared_path)" || true
    sleep 0.1
done
