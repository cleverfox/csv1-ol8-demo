# Unified Test Program

The `unified_test` program provides a single interface for testing DAC control devices over both serial and TCP connections, using the same protocol and framing.

## Features

- **Dual Transport Support**: Automatically detects and connects via serial or TCP based on target format
- **Protocol Compatibility**: Uses the same 4-byte command protocol for both transports
- **Configurable Rate**: Adjustable test frequency (Hz)
- **Verbose Output**: Optional detailed logging of all communications
- **Graceful Shutdown**: Handles Ctrl+C interruption cleanly

## Usage

```bash
cargo run --bin unified_test -- <TARGET> [OPTIONS]
```

### Command Line Arguments

- `<TARGET>`: Connection target (required)
  - **Serial device**: `/dev/ttyACM0`, `/dev/ttyUSB0`, `COM5`, etc.
  - **IPv4 TCP**: `192.168.1.100:1234`
  - **IPv6 TCP**: `[::1]:1234`, `[2001:db8::1]:1234`

- `-r, --rate <RATE>`: Test rate in Hz (default: 10)
- `-v, --verbose`: Enable verbose output showing all data transfers
- `-h, --help`: Show help information

## Examples

### Serial Connection
```bash
# Connect to USB CDC device on Linux
cargo run --bin unified_test -- /dev/ttyACM0

# Connect to USB serial adapter on Linux
cargo run --bin unified_test -- /dev/ttyUSB0

# Connect to COM port on Windows
cargo run --bin unified_test -- COM5

# High-speed serial test with verbose output
cargo run --bin unified_test -- /dev/ttyACM0 -r 100 -v
```

### TCP Connection
```bash
# Connect to IPv4 address
cargo run --bin unified_test -- 192.168.1.100:8080

# Connect to localhost
cargo run --bin unified_test -- 127.0.0.1:1234

# Connect to IPv6 address
cargo run --bin unified_test -- [::1]:8080

# Connect with custom rate
cargo run --bin unified_test -- 192.168.1.100:1234 -r 50
```

## Protocol Overview

The program communicates using 4-byte commands with automatic padding to 4-byte boundaries:

| Byte 0 | Byte 1 | Bytes 2-3 | Description |
|--------|--------|-----------|-------------|
| 0-7    | 0x00   | value     | Direct DAC write |
| 0-7    | 16-19  | 0x0000    | Attach table to DAC |
| 16-19  | 0-255  | value     | Write table entry |
| 0xFF   | offset | 0x0000    | Use table offset |
| 0xFE   | 0-7    | 0/1       | GPIO control |
| 0xFD   | 0x00   | 0x0000    | Keep alive |
| 0xFC   | 0x00   | 0x0000    | LDAC update |

## Test Sequence

The program executes the following test sequence:

1. **GPIO Setup**: Enables GPIO pins 0 and 1
2. **Init1**: Configures initial DAC table bindings
3. **Init2**: Sets up secondary table bindings  
4. **Init3**: Initializes all 8 DAC channels (sent as individual 4-byte chunks)
5. **Keep Alive**: Sends 3 keep-alive commands with delays
6. **Main Loop**: Continuously cycles through DAC channels with increasing values

## Transport Details

### Serial Transport
- **Baud Rate**: 115,200 bps
- **Timeout**: 100ms for read/write operations
- **Flow Control**: None
- **Data Bits**: 8, Stop Bits: 1, Parity: None

### TCP Transport
- **Timeout**: 100ms for read/write operations
- **Connection**: Persistent TCP stream
- **Protocol**: Raw binary data over TCP

## Error Handling

- **Connection Failures**: Program exits with error message
- **Communication Timeouts**: Logged but operation continues
- **Data Errors**: Detailed error reporting with context
- **Ctrl+C**: Graceful shutdown with cleanup

## Building

Ensure you have the required dependencies:

```bash
# Build the unified test
cargo build --bin unified_test

# Run with release optimizations
cargo build --release --bin unified_test
cargo run --release --bin unified_test -- <target>
```

## Dependencies

- `serialport`: Serial communication
- `clap`: Command line argument parsing
- `anyhow`: Error handling
- `ctrlc`: Signal handling
- `tokio`: Async runtime (for future TCP enhancements)

## Troubleshooting

### Serial Issues
- **Permission denied**: Add user to `dialout` group on Linux
- **Device not found**: Check device path with `ls /dev/tty*`
- **Access denied on Windows**: Close any other serial terminal programs

### TCP Issues
- **Connection refused**: Verify target device is listening on specified port
- **Address in use**: Check if port is already bound by another process
- **Network unreachable**: Verify network connectivity and firewall settings

### General Issues
- **Build failures**: Run `cargo update` to refresh dependencies
- **Runtime panics**: Try with `-v` flag for detailed debugging information