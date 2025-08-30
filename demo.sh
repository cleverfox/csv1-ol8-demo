#!/bin/bash

echo "=== Unified Test Demonstration ==="
echo

# Function to cleanup background processes
cleanup() {
    echo "Cleaning up background processes..."
    pkill -f tcp_server_example 2>/dev/null || true
    exit 0
}

# Set up signal handlers
trap cleanup SIGINT SIGTERM EXIT

# Build the programs first
echo "Building programs..."
cargo build --bin unified_test --bin tcp_server_example
if [ $? -ne 0 ]; then
    echo "Build failed!"
    exit 1
fi

echo
echo "=== Starting TCP Server Example ==="
echo "Server will listen on 127.0.0.1:8080..."

# Start TCP server in background
cargo run --bin tcp_server_example -- -p 8080 -v &
SERVER_PID=$!

# Wait for server to start
sleep 2

echo
echo "=== Testing TCP Connection ==="
echo "Running unified test with TCP transport..."
echo "Command: cargo run --bin unified_test -- 127.0.0.1:8080 -r 2 -v"
echo

# Run the unified test for a limited time (simulate with a few iterations)
# We'll use a timeout-like approach by running it briefly
(
    cargo run --bin unified_test -- 127.0.0.1:8080 -r 2 -v &
    CLIENT_PID=$!
    sleep 10
    kill $CLIENT_PID 2>/dev/null || true
    wait $CLIENT_PID 2>/dev/null || true
)

echo
echo "=== Demonstration Complete ==="
echo
echo "The unified_test program supports:"
echo "  Serial devices: /dev/ttyACM0, /dev/ttyUSB0, COM5, etc."
echo "  IPv4 TCP:       192.168.1.100:1234"
echo "  IPv6 TCP:       [::1]:8080"
echo
echo "Usage examples:"
echo "  cargo run --bin unified_test -- /dev/ttyACM0"
echo "  cargo run --bin unified_test -- 192.168.1.100:1234 -r 50"
echo "  cargo run --bin unified_test -- [::1]:8080 -v"
echo

# Cleanup will be called automatically by trap
