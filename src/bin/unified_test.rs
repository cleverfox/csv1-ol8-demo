use anyhow::{anyhow, Context, Result};
use clap::Parser;
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

/// Unified test program that can communicate over serial or TCP
#[derive(Parser, Debug)]
#[command(name = "unified_test")]
#[command(about = "Test program supporting both serial and TCP communication")]
struct Args {
    /// Connection target: serial device path (e.g., /dev/ttyACM0, COM5) or network address (IPv4:port, [IPv6]:port)
    target: String,

    /// Test rate in Hz
    #[arg(short, long, default_value = "10")]
    rate: u32,

    /// Enable verbose output
    #[arg(short, long)]
    verbose: bool,

    /// Read timeout in milliseconds
    #[arg(long, default_value = "500")]
    read_timeout: u64,

    /// Write timeout in milliseconds
    #[arg(long, default_value = "1000")]
    write_timeout: u64,
}

// Protocol documentation:
// 0 - DAC [0..15] or table number [16..20]
// 1 - table item index [0..255]
// 2 - word's MSB
// 3 - word's LSB
/* + -----------------------------------------------+
 * | First byte  | Second byte  | third & 4th bytes |
 * + -----------------------------------------------+
 * | n = 0..7    | 0x00         | vv                | DirectWrite DAC(n)=vv
 * | n = 0..7    | i+16 (16..19)| vv                | AttachTable DAC(n)=Table(i)
 * | i+16(16..19)| n (0..255)   | vv                | Table(i)[n]=vv
 * | 0xff        | n (0..255)   | 0x0000            | UseTable
 * | 0xfe        | n (0..7)     | 0x0000..0x0001    | control GPIOn
 * | 0xfd        | 0x00         | 0x0000            | KeepAlive (to avoid disabling GPIO0)
 * | 0xfc        | 0x00         | 0x0000            | LDAC - update DACs with loaded values
 * + -----------------------------------------------+
 */

/// Transport abstraction trait
trait Transport {
    fn write_data(&mut self, data: &[u8]) -> Result<usize>;
    fn read_data(&mut self, buffer: &mut [u8]) -> Result<usize>;
    fn transport_type(&self) -> &'static str;
}

/// Serial port transport implementation
struct SerialTransport {
    port: Box<dyn serialport::SerialPort>,
}

impl SerialTransport {
    fn new(device_path: &str) -> Result<Self> {
        let port = serialport::new(device_path, 115_200)
            .timeout(Duration::from_millis(100))
            .open()
            .with_context(|| format!("Failed to open serial port: {}", device_path))?;

        Ok(SerialTransport { port })
    }
}

impl Transport for SerialTransport {
    fn write_data(&mut self, data: &[u8]) -> Result<usize> {
        // Pad data to multiple of 4 bytes as required by protocol
        let mut padded_data = data.to_vec();
        while padded_data.len() % 4 != 0 {
            padded_data.push(0);
        }

        self.port
            .write(&padded_data)
            .with_context(|| "Serial write failed")
    }

    fn read_data(&mut self, buffer: &mut [u8]) -> Result<usize> {
        match self.port.read(buffer) {
            Ok(n) => Ok(n),
            Err(e) if e.kind() == std::io::ErrorKind::TimedOut => Ok(0),
            Err(e) => Err(anyhow!("Serial read failed: {}", e)),
        }
    }

    fn transport_type(&self) -> &'static str {
        "Serial"
    }
}

/// TCP transport implementation
struct TcpTransport {
    stream: TcpStream,
}

impl TcpTransport {
    fn new(address: &str, read_timeout_ms: u64, write_timeout_ms: u64) -> Result<Self> {
        let stream = TcpStream::connect(address)
            .with_context(|| format!("Failed to connect to TCP address: {}", address))?;

        stream.set_read_timeout(Some(Duration::from_millis(read_timeout_ms)))?;
        stream.set_write_timeout(Some(Duration::from_millis(write_timeout_ms)))?;

        Ok(TcpTransport { stream })
    }
}

impl Transport for TcpTransport {
    fn write_data(&mut self, data: &[u8]) -> Result<usize> {
        // Pad data to multiple of 4 bytes as required by protocol
        let mut padded_data = data.to_vec();
        while padded_data.len() % 4 != 0 {
            padded_data.push(0);
        }

        self.stream
            .write_all(&padded_data)
            .with_context(|| "TCP write failed")?;
        Ok(padded_data.len())
    }

    fn read_data(&mut self, buffer: &mut [u8]) -> Result<usize> {
        match self.stream.read(buffer) {
            Ok(n) => Ok(n),
            Err(e) if e.kind() == std::io::ErrorKind::TimedOut => Ok(0),
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => Ok(0),
            Err(e) if e.raw_os_error() == Some(35) => Ok(0), // EAGAIN on macOS/BSD
            Err(e) if e.raw_os_error() == Some(11) => Ok(0), // EAGAIN on Linux
            Err(e) => {
                eprintln!("TCP read error (continuing): {}", e);
                Ok(0) // Continue operation even on read errors
            }
        }
    }

    fn transport_type(&self) -> &'static str {
        "TCP"
    }
}

/// Determine transport type based on target string format
fn create_transport(target: &str, args: &Args) -> Result<Box<dyn Transport>> {
    // Check if it looks like a network address (contains : and possibly [])
    if target.contains(':') {
        // Try to parse as socket address to validate format
        let addr = if target.starts_with('[') {
            // IPv6 format [::1]:1234
            target
                .to_socket_addrs()
                .with_context(|| format!("Invalid IPv6 address format: {}", target))?
                .next()
                .ok_or_else(|| anyhow!("Could not resolve address: {}", target))?
        } else {
            // IPv4 format 127.0.0.1:1234
            target
                .to_socket_addrs()
                .with_context(|| format!("Invalid IPv4 address format: {}", target))?
                .next()
                .ok_or_else(|| anyhow!("Could not resolve address: {}", target))?
        };

        println!(
            "Connecting to {} via TCP (read_timeout={}ms, write_timeout={}ms)...",
            addr, args.read_timeout, args.write_timeout
        );
        Ok(Box::new(TcpTransport::new(
            target,
            args.read_timeout,
            args.write_timeout,
        )?))
    } else {
        // Assume it's a serial device path
        println!("Opening serial device: {}", target);
        Ok(Box::new(SerialTransport::new(target)?))
    }
}

/// Protocol helper functions
fn write_command(transport: &mut Box<dyn Transport>, data: &[u8], verbose: bool) -> Result<()> {
    let result = transport.write_data(data)?;
    if verbose {
        println!("Wrote {} bytes: {:?}", result, data);
    }
    Ok(())
}

fn read_response(transport: &mut Box<dyn Transport>, verbose: bool) -> Result<Vec<u8>> {
    let mut buffer = vec![0u8; 1000];
    let bytes_read = transport.read_data(&mut buffer)?;
    buffer.truncate(bytes_read);

    if verbose {
        if bytes_read > 0 {
            println!("Read {} bytes: {:?}", bytes_read, &buffer);
        } else {
            println!("No response data (timeout or no data available)");
        }
    }

    Ok(buffer)
}

fn main() -> Result<()> {
    let args = Args::parse();

    let mut transport = create_transport(&args.target, &args)?;

    println!(
        "Connected via {} at {}Hz (read_timeout={}ms)",
        transport.transport_type(),
        args.rate,
        args.read_timeout
    );

    // GPIO setup - same as original protocol
    println!("Setting up GPIO...");
    write_command(
        &mut transport,
        &[0xfe, 0, 0, 1, 0xfe, 1, 0, 1],
        args.verbose,
    )?;
    let _response = read_response(&mut transport, args.verbose)?;
    std::thread::sleep(Duration::from_millis(50));

    // Init1 - same as original protocol
    println!("Sending init1...");
    write_command(
        &mut transport,
        &[16, 49, 0, 0, 16, 50, 64, 0, 16, 51, 128, 0],
        args.verbose,
    )?;
    let _response = read_response(&mut transport, args.verbose)?;
    std::thread::sleep(Duration::from_millis(50));

    // Init2 - same as original protocol
    println!("Sending init2...");
    write_command(
        &mut transport,
        &[17, 49, 64, 0, 17, 50, 128, 0, 17, 51, 0, 0],
        args.verbose,
    )?;
    let _response = read_response(&mut transport, args.verbose)?;
    std::thread::sleep(Duration::from_millis(100));

    // Init3 - broken into smaller 4-byte chunks to avoid buffer overflow
    println!("Sending init3 (in chunks)...");
    let init3_commands = [
        [0, 16, 0, 0],
        [1, 17, 0, 0],
        [2, 16, 0, 0],
        [3, 17, 0, 0],
        [4, 16, 0, 0],
        [5, 17, 0, 0],
        [6, 16, 0, 0],
        [7, 17, 0, 0],
    ];

    for (i, cmd) in init3_commands.iter().enumerate() {
        write_command(&mut transport, cmd, args.verbose)?;
        let _response = read_response(&mut transport, args.verbose)?;
        if args.verbose {
            println!("Init3 chunk {} completed", i + 1);
        }
        std::thread::sleep(Duration::from_millis(20));
    }

    std::thread::sleep(Duration::from_millis(100));

    // Keepalive test
    println!("Sending keepalive commands...");
    for i in 0..3 {
        write_command(&mut transport, &[0xfd, 0, 0, 0], args.verbose)?;
        let _response = read_response(&mut transport, args.verbose)?;
        if args.verbose {
            println!("Keepalive {} completed", i + 1);
        }
        std::thread::sleep(Duration::from_secs(1));
    }

    // Main loop - same logic as original
    println!("Starting main data loop...");
    let mut msg: Vec<u8> = vec![255, 0, 0, 0];
    let mut v: u16 = 0;
    let mut c: u8 = 0;
    let mut loop_count = 0;

    // Set up Ctrl+C handler
    let running = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    let r = running.clone();
    ctrlc::set_handler(move || {
        println!("\nReceived Ctrl+C, shutting down...");
        r.store(false, std::sync::atomic::Ordering::SeqCst);
    })
    .context("Error setting Ctrl+C handler")?;

    while running.load(std::sync::atomic::Ordering::SeqCst) {
        c += 1;
        if c > 7 {
            c = 0;
        }

        if v == 65535 {
            v = 0;
        } else if v < 65535 - 511 {
            v += 511;
        } else {
            v = 65535;
        }

        msg[0] = c;
        if c < 4 {
            msg[2] = ((v & 0xff00) >> 8) as u8;
            msg[3] = (v & 0xff) as u8;
        } else {
            msg[2] = (((65535 - v) & 0xff00) >> 8) as u8;
            msg[3] = ((65535 - v) & 0xff) as u8;
        }

        match write_command(&mut transport, &msg, args.verbose) {
            Ok(()) => {
                let _response = read_response(&mut transport, args.verbose)?;
                if args.verbose || loop_count % 100 == 0 {
                    println!(
                        "Loop {}: DAC {} = {}",
                        loop_count,
                        c,
                        ((msg[2] as u16) << 8) | (msg[3] as u16)
                    );
                }
            }
            Err(e) => {
                eprintln!("Write error in main loop: {}", e);
                std::thread::sleep(Duration::from_millis(100));
            }
        }

        loop_count += 1;

        if args.rate > 0 {
            std::thread::sleep(Duration::from_millis((1000.0 / args.rate as f32) as u64));
        }
    }

    println!("Test completed successfully.");
    Ok(())
}
