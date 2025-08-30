# DAC Control System

This project provides test programs for communicating with DAC control devices over both TCP and serial connections. The system includes both Rust and Python implementations that share the same 4-byte command protocol.

## Device Information

- **Device**: csv1-ol8
- **Communication**: TCP or Serial (CDC)

## Features

- **Dual Transport Support**: Automatic detection of TCP vs serial targets
- **Cross-Platform**: Works on Windows, Linux, macOS, FreeBSD
- **Multiple Implementations**: Both Rust and Python versions
- **Same Protocol**: Identical 4-byte command structure across all transports
- **Configurable Timeouts**: Adjustable communication timeouts
- **Robust Error Handling**: Graceful handling of connection issues
- **DAC Control**: Direct DAC channel manipulation
- **GPIO Control**: GPIO pin state management
- **Table Operations**: Waveform table programming and playback

## Protocol

The device uses a 4-byte command protocol with automatic padding:

| First byte  | Second byte  | Bytes 2-3     | Description |
|-------------|--------------|---------------|-------------|
| 0-7         | 0x00         | value         | Direct DAC write: DAC(n) = value |
| 0-7         | 16-19        | 0x0000        | Attach table to DAC: DAC(n) = Table(i) |
| 16-19       | 0-255        | value         | Write table entry: Table(i)[n] = value |
| 0xFF        | 0-255        | 0x0000        | Use table offset |
| 0xFE        | 0-7          | 0x0000/0x0001 | Control GPIO pin |
| 0xFD        | 0x00         | 0x0000        | Keep alive |
| 0xFC        | 0x00         | 0x0000        | LDAC - update DACs |
| 0xFB        | 0-255        | value         | Register write |

## Rust Implementation

### Building

```bash
cargo build --release
```

### Available Programs

- `unified_test`: Main test program with auto-transport detection
- `tcp_robust_test`: TCP-optimized test with advanced error handling
- `tcp_server_example`: TCP server simulator for testing

### Usage Examples

#### Unified Test (Recommended)
```bash
# Serial communication
cargo run --bin unified_test -- /dev/ttyACM0
cargo run --bin unified_test -- COM5 --rate 50 --verbose

# TCP communication
cargo run --bin unified_test -- 192.168.56.102:2012
cargo run --bin unified_test -- [::1]:8080 --read-timeout 1000
```

#### Robust TCP Test
```bash
# Basic TCP test
cargo run --bin tcp_robust_test -- 192.168.56.102:2012

# With custom timeouts and no response reading
cargo run --bin tcp_robust_test -- 192.168.56.102:2012 \
  --read-timeout 500 --no-responses --verbose

# Only expect responses from specific commands
cargo run --bin tcp_robust_test -- 192.168.56.102:2012 \
  --response-commands "0xfd,0xfe" --verbose
```

### Command Line Options

- `--rate <Hz>`: Test frequency (default: 10 Hz)
- `--verbose`: Enable detailed logging
- `--read-timeout <ms>`: Read timeout in milliseconds
- `--write-timeout <ms>`: Write timeout in milliseconds
- `--no-responses`: Skip reading responses (fire-and-forget)
- `--duration <sec>`: Test duration in seconds

## Python Implementation

### Requirements

```bash
pip install -r requirements.txt
```

### Usage Examples

```bash
# Serial communication
python CSv1-OL8-IRS422.py /dev/ttyACM0
python CSv1-OL8-IRS422.py COM5 --timeout 0.2 --verbose

# TCP communication
python CSv1-OL8-IRS422.py 192.168.56.102:2012
python CSv1-OL8-IRS422.py 192.168.56.102:2012 --timeout 0.5 --verbose

# IPv6 TCP
python CSv1-OL8-IRS422.py [::1]:8080 --timeout 0.3
```

### Python Options

- `--timeout <sec>`: Communication timeout (default: 0.1s serial, 0.25s TCP)
- `--verbose`: Enable verbose output

## Transport Detection

The system automatically detects the appropriate transport based on the target format:

| Format | Transport | Example |
|--------|-----------|---------|
| `/dev/ttyXXX` | Serial | `/dev/ttyACM0`, `/dev/ttyUSB0` |
| `COMX` | Serial | `COM1`, `COM5` |
| `IP:port` | TCP IPv4 | `192.168.1.100:8080` |
| `[IPv6]:port` | TCP IPv6 | `[::1]:8080`, `[2001:db8::1]:1234` |

## Test Sequence

Both implementations execute the same test sequence:

1. **GPIO Setup**: Enable GPIO pins 0 and 1
2. **DAC Initialization**: Configure initial DAC values
3. **Table Binding**: Attach DAC channels to lookup tables
4. **Table Programming**: Fill tables with test waveforms
5. **Table Playback**: Cycle through table offsets
6. **Keep Alive**: Continuous keep-alive transmission

## Communication Settings

### Serial (USB CDC)
- **Baud Rate**: any
- **Default Timeout**: 100ms (Rust), 100ms (Python)

### Serial (UART)
- **Baud Rate**: 115,200 bps
- **Data Bits**: 8
- **Stop Bits**: 1
- **Parity**: None
- **Flow Control**: None
- **Default Timeout**: 100ms (Rust), 100ms (Python)

### TCP
- **Protocol**: Raw TCP sockets
- **Connection**: Persistent stream
- **Default Timeout**: 200ms (Rust), 250ms (Python)
- **Features**: Nodelay enabled, configurable timeouts

## Troubleshooting

### Serial Issues
- **Permission denied**: Add user to `dialout` group (Linux) or `operator` group (FreeBSD)
- **Device not found**: Check device path with `ls /dev/tty*` or Device Manager
- **Access denied**: Close other serial terminal programs

### TCP Issues
- **Connection refused**: Verify server is listening on specified port
- **Timeout errors**: Increase timeout values or use `--no-responses` mode
- **Network unreachable**: Check network connectivity and firewall

### General Issues
- **Protocol errors**: Enable verbose mode to see command/response details
- **Build failures**: Run `cargo update` or check Python requirements
- **Intermittent failures**: Try longer timeouts or retry logic

## Development and Testing

### TCP Server Simulation
Test TCP functionality without hardware:

```bash
# Start TCP server simulator
cargo run --bin tcp_server_example -- --port 8080 --verbose

# Connect with test client (in another terminal)
cargo run --bin unified_test -- 127.0.0.1:8080 --verbose
```

### Verbose Debugging
Enable verbose mode to see all communication:

```bash
# Rust
cargo run --bin unified_test -- <target> --verbose

# Python
python CSv1-OL8-IRS422.py <target> --verbose
```

## Files

- `src/bin/unified_test.rs`: Main Rust test program
- `src/bin/tcp_robust_test.rs`: TCP-optimized Rust test
- `src/bin/tcp_server_example.rs`: TCP server simulator
- `python/CSv1-OL8-IRS422.py`: Python implementation
- `UNIFIED_TEST.md`: Detailed Rust usage documentation
- `IMPROVEMENTS.md`: Technical implementation details

## License

This project is provided as-is for testing and development purposes.
