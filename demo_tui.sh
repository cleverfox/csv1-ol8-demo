#!/bin/bash

echo "=== TUI Diagnostic Tool Demo ==="
echo
echo "This demo shows the improved TUI diagnostic tool with new key mappings:"
echo
echo "🎛️  DAC Control:"
echo "   ← → : Select DAC channel (0-7)"
echo "   ↑ ↓ : Adjust DAC value by step (overflow-safe, clamped 0-65535)"
echo "   SPACE : Large step (+8192) up to 65535, then wraps to 0"
echo
echo "🔧 GPIO Control:"
echo "   Z X C V B N M , : Toggle GPIO pins 0-7"
echo "   (Z=GPIO0, X=GPIO1, C=GPIO2, V=GPIO3, B=GPIO4, N=GPIO5, M=GPIO6, ,=GPIO7)"
echo
echo "📊 Table Control:"
echo "   0 1 2 3 4 5 6 7 8 9 : Set table offset 0-9"
echo
echo "⚙️  System:"
echo "   ESC or q : Quit application"
echo "   Auto keepalive every 5 seconds"
echo "   Status display shows last command + device response"
echo

# Function to cleanup background processes
cleanup() {
    echo "Cleaning up..."
    pkill -f tcp_server_example 2>/dev/null || true
    exit 0
}

# Set up signal handlers
trap cleanup SIGINT SIGTERM EXIT

# Build the programs
echo "Building programs..."
if ! cargo build --bin tui_diagnostic --bin tcp_server_example >/dev/null 2>&1; then
    echo "❌ Build failed!"
    exit 1
fi
echo "✅ Build completed"
echo

# Start TCP server simulator
echo "🚀 Starting TCP server simulator on port 8080..."
cargo run --bin tcp_server_example -- -p 8080 -v &
SERVER_PID=$!

# Wait for server to start
sleep 2

echo
echo "🎮 Starting TUI Diagnostic Tool..."
echo "   Target: 127.0.0.1:8080"
echo "   Step size: 512"
echo "   Keepalive: every 3 seconds"
echo
echo "Instructions:"
echo "1. Use arrow keys ← → to select different DAC channels"
echo "2. Use arrow keys ↑ ↓ to adjust the selected DAC value (overflow-safe)"
echo "3. Press SPACE for large jumps (+8192) - goes to 65535 first, then wraps to 0!"
echo "4. Press numbers 0-9 to change table offset"
echo "5. Press Z,X,C,V,B,N,M,comma to toggle GPIO pins 0-7"
echo "6. Watch the visual sliders and GPIO indicators update in real-time"
echo "7. Monitor the status window for command/response display"
echo "8. Press ESC or 'q' to quit when done"
echo
echo "Press Enter to launch the TUI..."
read

# Launch TUI diagnostic tool
cargo run --bin tui_diagnostic -- 127.0.0.1:8080 --step 512 --keepalive-interval 3

echo
echo "🎯 Demo Features Demonstrated:"
echo "   ✓ Interactive DAC sliders with visual feedback"
echo "   ✓ Improved key mappings (0-9 for table, ZXCVBNM, for GPIO)"
echo "   ✓ Space bar large steps (goes to 65535 before wrapping to 0)"
echo "   ✓ Up/down arrow overflow-safe clamping (0 ≤ value ≤ 65535)"
echo "   ✓ Real-time GPIO status indicators"
echo "   ✓ Auto-keepalive functionality"
echo "   ✓ Command history and device response display"
echo "   ✓ Live debugging with response byte visualization"
echo
echo "🔗 Try with your real device:"
echo "   cargo run --bin tui_diagnostic -- 192.168.56.102:2012"
echo "   cargo run --bin tui_diagnostic -- /dev/ttyACM0 --step 256"
echo
echo "📚 See TUI_DIAGNOSTIC.md for complete documentation"
echo

# Cleanup will be called automatically by trap
