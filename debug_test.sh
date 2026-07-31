#!/bin/bash
cd ~/Development/CodeCrafters/shell-rust

# Create test directories
rm -rf /tmp/shell_test
mkdir -p /tmp/shell_test/dog/cow

# Compile
cargo build --release 2>&1 | tail -1

# Run with test directory
echo "Testing: press TAB after 'ls dog/'"
echo "\$ ls dog/" | stdbuf -oL stderr/stdout ./target/release/codecrafters-shell 2>&1 &
PID=$!
sleep 0.5
echo "TAB" > /proc/$PID/fd/0 2>/dev/null || true
wait $PID 2>/dev/null || true