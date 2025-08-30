# TUI Diagnostic Tool

An interactive Terminal User Interface (TUI) for real-time DAC control and GPIO manipulation. This tool provides a visual, keyboard-driven interface for testing and controlling DAC devices over both serial and TCP connections.

## Overview

The TUI diagnostic tool offers:
- **Real-time DAC Control**: 8 visual sliders with keyboard control
- **GPIO Management**: Toggle GPIO pins 0-7 with number keys
- **Table Control**: Switch table offsets 0-9 with QWERTYUIOP keys
- **Auto Keepalive**: Automatic keepalive transmission every 5 seconds
- **Dual Transport**: Works over serial ports or TCP connections
- **Visual Feedback**: Live status display and command history

## Building and Running

### Build
```bash
cargo build --bin tui_diagnostic --release
```

### Usage
```bash
cargo run --bin tui_diagnostic -- <TARGET> [OPTIONS]
```

## Command Line Arguments

| Argument | Description | Default |
|----------|-------------|---------|
| `<TARGET>` | Connection target (required) | - |
| `-s, --step <STEP>` | DAC value step size for up/down keys | 256 |
| `--read-timeout <MS>` | Read timeout in milliseconds | 200 |
| `--write-timeout <MS>` | Write timeout in milliseconds | 1000 |
| `--keepalive-interval <SEC>` | Keepalive interval in seconds | 5 |

## Connection Targets

| Format | Transport | Example |
|--------|-----------|---------|
| Serial Device | Serial/CDC | `/dev/ttyACM0`, `COM5` |
| IPv4 Address | TCP | `192.168.56.102:2012` |
| IPv6 Address | TCP | `[::1]:8080` |

## Examples

### Serial Connection
```bash
# Basic serial connection
cargo run --bin tui_diagnostic -- /dev/ttyACM0

# Serial with custom step size
cargo run --bin tui_diagnostic -- COM5 --step 512

# Serial with longer timeouts
cargo run --bin tui_diagnostic -- /dev/ttyUSB0 --read-timeout 500 --write-timeout 2000
```

### TCP Connection
```bash
# Basic TCP connection
cargo run --bin tui_diagnostic -- 192.168.56.102:2012

# TCP with fine control step
cargo run --bin tui_diagnostic -- 192.168.1.100:8080 --step 64

# IPv6 TCP connection
cargo run --bin tui_diagnostic -- [::1]:8080 --step 1024
```

## Interface Layout

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                    DAC Control Panel - TUI Diagnostic Tool                 │
└─────────────────────────────────────────────────────────────────────────────┘
┌──────┬──────┬──────┬──────┬──────┬──────┬──────┬──────────────────────────┐
│ DAC0 │ DAC1 │ DAC2 │ DAC3 │ DAC4 │ DAC5 │ DAC6 │ DAC7                     │
│ ████ │ ██   │ ████ │      │ ████ │      │ ██   │ ████                     │
│ 2048 │ 1024 │ 3072 │   0  │ 2560 │   0  │ 512  │ 4096                     │
└──────┴──────┴──────┴──────┴──────┴──────┴──────┴──────────────────────────┘
┌──────┬──────┬──────┬──────┬──────┬──────┬──────┬──────────────────────────┐
│GPIO0 │GPIO1 │GPIO2 │GPIO3 │GPIO4 │GPIO5 │GPIO6 │GPIO7                     │
│ ON   │ OFF  │ ON   │ OFF  │ OFF  │ ON   │ OFF  │ ON                       │
└──────┴──────┴──────┴──────┴──────┴──────┴──────┴──────────────────────────┘
┌─────────────────────────────────────────────────────────────────────────────┐
│ Table Offset: 3 (0-9 keys)                                                 │
└─────────────────────────────────────────────────────────────────────────────┘
┌─────────────────────────────────────────────────────────────────────────────┐
│ Last: DAC 2 = 3072 | Response: 2 bytes: [00, 00]                          │
└─────────────────────────────────────────────────────────────────────────────┘
┌─────────────────────────────────────────────────────────────────────────────┐
│ Controls:                                                                   │
│ ← → : Select DAC channel      ↑ ↓ : Adjust DAC value                      │
│ SPACE : Large step (+8192)    0-9 : Set table offset                      │
│ ZXCVBNM, : Toggle GPIO 0-7    ESC/q : Quit application                    │
└─────────────────────────────────────────────────────────────────────────────┘
```

## Controls

### DAC Control
- **← →** (Left/Right arrows): Select DAC channel (0-7)
- **↑ ↓** (Up/Down arrows): Increase/decrease selected DAC value by step size (clamped at 0 and 65535, overflow-safe)
- **SPACE** (Space bar): Large step increase (+8192) up to 65535, then wraps to 0 (only when already at 65535)
- Selected channel is highlighted in **red**
- DAC values range from 0 to 65535 (16-bit)
- Visual sliders show current values as percentages and absolute values

### GPIO Control
- **Z X C V B N M ,** (Letter keys): Toggle GPIO pins 0-7 respectively
  - Z = GPIO 0, X = GPIO 1, C = GPIO 2, V = GPIO 3
  - B = GPIO 4, N = GPIO 5, M = GPIO 6, , = GPIO 7
- **Green/Bold**: GPIO pin is ON (HIGH)
- **Gray**: GPIO pin is OFF (LOW)
- Each press toggles the state

### Table Control
- **0 1 2 3 4 5 6 7 8 9**: Set table offset 0-9 respectively
- Sends UseTable command (0xFF) with specified offset

### System Control
- **ESC** or **q**: Quit application
- **Automatic Keepalive**: Sent every 5 seconds (configurable)

### DAC Value Behavior
- **Up/Down arrows**: Increment/decrement with bounds checking (0 ≤ value ≤ 65535), overflow-safe
- **Space bar**: Large increment (+8192) up to 65535, then wraps to 0 (only from 65535 → 0)
- **Step size**: Configurable via `--step` argument (default: 256)

## Protocol Commands

The tool sends standard 4-byte DAC protocol commands:

| Command | Bytes | Description |
|---------|--------|-------------|
| DAC Write | `[ch, 0x00, hi, lo]` | Set DAC channel to 16-bit value |
| GPIO Control | `[0xFE, pin, 0x00, state]` | Set GPIO pin high/low (state: 0x00=OFF, 0x01=ON) |
| Table Offset | `[0xFF, offset, 0x00, 0x00]` | Use table at offset (0-9) |
| Keepalive | `[0xFD, 0x00, 0x00, 0x00]` | Prevent timeout |

## Status Information

### Display Elements
- **Title Bar**: Shows application name
- **DAC Sliders**: Visual representation of all 8 DAC channels
- **GPIO Status**: Shows ON/OFF state of all 8 GPIO pins
- **Table Offset**: Current table offset (0-9)
- **Status**: Shows last command sent and device response received
- **Controls**: Help text for keyboard shortcuts

### Visual Indicators
- **Red highlight**: Selected DAC channel
- **Green/Bold**: Active GPIO pins
- **Blue gauges**: DAC value visualization
- **Percentage bars**: DAC values as 0-100% of full scale
- **Response display**: Shows raw bytes received from device (e.g., "2 bytes: [00, 00]")

## Step Size Configuration

The step size determines how much DAC values change with up/down keys:

| Step Size | Precision | Use Case |
|-----------|-----------|----------|
| 1 | Maximum | Fine tuning |
| 16 | High | Precise control |
| 64 | Medium | General testing |
| 256 | Standard | Normal operation |
| 1024 | Coarse | Quick changes |
| 4096 | Very coarse | Range testing |

**Space Bar Behavior Examples:**
- From 57344: Space → 65535 (not wrapped to 0)
- From 61440: Space → 65535 (clamped at maximum)  
- From 65535: Space → 0 (only wraps when at maximum)

### Examples
```bash
# Fine control (1 LSB steps)
cargo run --bin tui_diagnostic -- /dev/ttyACM0 --step 1

# Standard control (default)
cargo run --bin tui_diagnostic -- 192.168.1.100:8080 --step 256

# Coarse control (quick range testing)
cargo run --bin tui_diagnostic -- COM5 --step 4096
```

## Transport Settings

### Serial (CDC)
- **Baud Rate**: 115,200 bps (fixed)
- **Data Format**: 8N1 (8 data bits, no parity, 1 stop bit)
- **Flow Control**: None
- **Default Timeout**: 200ms read, 1000ms write

### TCP
- **Protocol**: Raw TCP sockets
- **Features**: TCP_NODELAY enabled for low latency
- **Default Timeout**: 200ms read, 1000ms write
- **Connection**: Persistent stream

## Troubleshooting

### Connection Issues
**Serial port not found**:
- Check device path: `ls /dev/tty*` (Linux/macOS) or Device Manager (Windows)
- Verify permissions: add user to `dialout` group (Linux)
- Close other serial programs

**TCP connection refused**:
- Verify server is listening: `netstat -ln | grep :2012`
- Check network connectivity: `ping 192.168.56.102`
- Verify firewall settings

### Performance Issues
**Slow response or timeouts**:
- Increase timeouts: `--read-timeout 1000 --write-timeout 2000`
- Reduce keepalive frequency: `--keepalive-interval 10`
- Check network latency for TCP connections

**Interface lag**:
- Use smaller step sizes for smoother control
- Ensure stable connection to device
- Check terminal performance

### Display Issues
**Garbled display**:
- Resize terminal window
- Ensure terminal supports ANSI colors
- Try different terminal emulator

**Missing characters**:
- Use terminal with Unicode support
- Verify font supports box drawing characters
- Try running with `TERM=xterm-256color`

## Development and Testing

### Test with Simulator
```bash
# Terminal 1: Start TCP simulator
cargo run --bin tcp_server_example -- -p 8080 -v

# Terminal 2: Connect with TUI
cargo run --bin tui_diagnostic -- 127.0.0.1:8080
```

### Debug Mode
Enable verbose output on the server side to see all commands:
```bash
cargo run --bin tcp_server_example -- -p 8080 -v
```

This shows exactly what commands the TUI tool is sending. The TUI tool itself will display device responses in the status window, showing both the command sent and the raw response bytes received.

## Integration

The TUI diagnostic tool complements other testing tools:
- Use `unified_test` for automated scripted testing
- Use `tcp_robust_test` for connection reliability testing  
- Use `tui_diagnostic` for interactive manual control
- Use `tcp_server_example` for development without hardware

## Tips and Best Practices

1. **Start with default step size (256)** for general testing
2. **Use fine steps (1-16)** for precise calibration
3. **Monitor status messages** for communication errors  
4. **Keep terminals wide enough** for proper display (≥80 columns)
5. **Use keepalive** to maintain connection during idle periods
6. **Test GPIO states** before relying on them for control
7. **Verify table offsets** correspond to programmed table data
8. **Monitor responses** in status window to verify device communication

## Keyboard Quick Reference

| Key | Action | Key | Action |
|-----|--------|-----|--------|
| ← → | Select DAC | 0-4 | Table offset 0-4 |
| ↑ ↓ | Adjust DAC | 5-9 | Table offset 5-9 |
| SPACE | Large step (+8192) | Z X C V | GPIO 0-3 |
| ESC/q | Quit | B N M , | GPIO 4-7 |

---

**Note**: The TUI tool provides real-time control and is ideal for interactive testing, calibration, and debugging of DAC control systems.