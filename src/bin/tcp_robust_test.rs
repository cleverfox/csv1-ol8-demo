use anyhow::{anyhow, Context, Result};
use clap::Parser;
use std::collections::HashSet;
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};

/// Robust TCP test program optimized for real device communication
#[derive(Parser, Debug)]
#[command(name = "tcp_robust_test")]
#[command(about = "Robust TCP test program for real DAC devices")]
struct Args {
    /// TCP address (IPv4:port or [IPv6]:port)
    address: String,

    /// Test rate in Hz
    #[arg(short, long, default_value = "10")]
    rate: u32,

    /// Enable verbose output
    #[arg(short, long)]
    verbose: bool,

    /// Connection timeout in milliseconds
    #[arg(long, default_value = "5000")]
    connect_timeout: u64,

    /// Read timeout in milliseconds
    #[arg(long, default_value = "200")]
    read_timeout: u64,

    /// Write timeout in milliseconds
    #[arg(long, default_value = "1000")]
    write_timeout: u64,

    /// Delay between commands in milliseconds
    #[arg(long, default_value = "10")]
    command_delay: u64,

    /// Skip reading responses (fire-and-forget mode)
    #[arg(long)]
    no_responses: bool,

    /// Only read responses for specific commands (comma-separated hex values, e.g., "0xfe,0xfd")
    #[arg(long)]
    response_commands: Option<String>,

    /// Maximum number of read retries per command
    #[arg(long, default_value = "3")]
    read_retries: u32,

    /// Test duration in seconds (0 = infinite)
    #[arg(short, long, default_value = "0")]
    duration: u64,
}

// Protocol documentation - same as unified test
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

struct RobustTcpClient {
    stream: TcpStream,
    response_commands: HashSet<u8>,
    stats: ConnectionStats,
    args: Args,
}

#[derive(Debug, Default)]
struct ConnectionStats {
    commands_sent: u64,
    responses_received: u64,
    timeouts: u64,
    errors: u64,
    bytes_sent: u64,
    bytes_received: u64,
}

impl RobustTcpClient {
    fn new(args: Args) -> Result<Self> {
        println!("Connecting to {}...", args.address);

        // Parse response commands if specified
        let response_commands = if args.no_responses {
            HashSet::new()
        } else if let Some(ref cmd_str) = args.response_commands {
            Self::parse_response_commands(cmd_str)?
        } else {
            // Default: expect responses from GPIO, keepalive, and LDAC commands
            [0xfe, 0xfd, 0xfc].iter().cloned().collect()
        };

        // Connect with timeout
        let addr = args
            .address
            .to_socket_addrs()
            .with_context(|| format!("Invalid address format: {}", args.address))?
            .next()
            .ok_or_else(|| anyhow!("Could not resolve address: {}", args.address))?;

        let stream = TcpStream::connect_timeout(&addr, Duration::from_millis(args.connect_timeout))
            .with_context(|| format!("Failed to connect to {}", addr))?;

        // Configure socket options
        stream.set_read_timeout(Some(Duration::from_millis(args.read_timeout)))?;
        stream.set_write_timeout(Some(Duration::from_millis(args.write_timeout)))?;
        stream.set_nodelay(true)?; // Disable Nagle's algorithm for low latency

        println!("Connected successfully to {}", addr);
        println!("Configuration:");
        println!("  Read timeout: {}ms", args.read_timeout);
        println!("  Write timeout: {}ms", args.write_timeout);
        println!("  Command delay: {}ms", args.command_delay);
        println!(
            "  Response mode: {}",
            if args.no_responses {
                "No responses"
            } else if response_commands.is_empty() {
                "All responses"
            } else {
                "Selective responses"
            }
        );

        Ok(RobustTcpClient {
            stream,
            response_commands,
            stats: ConnectionStats::default(),
            args,
        })
    }

    fn parse_response_commands(cmd_str: &str) -> Result<HashSet<u8>> {
        let mut commands = HashSet::new();
        for part in cmd_str.split(',') {
            let trimmed = part.trim();
            let value = if trimmed.starts_with("0x") || trimmed.starts_with("0X") {
                u8::from_str_radix(&trimmed[2..], 16)
            } else {
                trimmed.parse::<u8>()
            };
            commands.insert(value.with_context(|| format!("Invalid command value: {}", trimmed))?);
        }
        Ok(commands)
    }

    fn write_command(&mut self, data: &[u8]) -> Result<()> {
        // Pad to 4-byte boundary
        let mut padded_data = data.to_vec();
        while padded_data.len() % 4 != 0 {
            padded_data.push(0);
        }

        if self.args.verbose {
            println!("→ Sending {} bytes: {:02x?}", padded_data.len(), data);
        }

        match self.stream.write_all(&padded_data) {
            Ok(()) => {
                self.stats.commands_sent += 1;
                self.stats.bytes_sent += padded_data.len() as u64;
                Ok(())
            }
            Err(e) => {
                self.stats.errors += 1;
                Err(anyhow!("Write failed: {}", e))
            }
        }
    }

    fn read_response(&mut self, command_type: u8) -> Result<Vec<u8>> {
        // Check if we should read response for this command
        if self.args.no_responses
            || (!self.response_commands.is_empty()
                && !self.response_commands.contains(&command_type))
        {
            if self.args.verbose {
                println!("← Skipping response for command 0x{:02x}", command_type);
            }
            return Ok(Vec::new());
        }

        let mut buffer = vec![0u8; 1024];
        let mut total_bytes = 0;
        let start_time = Instant::now();

        for retry in 0..=self.args.read_retries {
            match self.stream.read(&mut buffer[total_bytes..]) {
                Ok(0) => {
                    if self.args.verbose {
                        println!("← Connection closed by remote");
                    }
                    break;
                }
                Ok(n) => {
                    total_bytes += n;
                    self.stats.responses_received += 1;
                    self.stats.bytes_received += n as u64;

                    if self.args.verbose {
                        println!(
                            "← Received {} bytes (retry {}): {:02x?}",
                            n,
                            retry,
                            &buffer[total_bytes - n..total_bytes]
                        );
                    }
                    break;
                }
                Err(e)
                    if e.kind() == std::io::ErrorKind::TimedOut ||
                         e.kind() == std::io::ErrorKind::WouldBlock ||
                         e.raw_os_error() == Some(35) ||  // EAGAIN on macOS/BSD
                         e.raw_os_error() == Some(11) =>
                {
                    // EAGAIN on Linux

                    if retry < self.args.read_retries {
                        if self.args.verbose {
                            println!(
                                "← Read timeout (retry {}/{})",
                                retry + 1,
                                self.args.read_retries
                            );
                        }
                        std::thread::sleep(Duration::from_millis(50));
                        continue;
                    } else {
                        self.stats.timeouts += 1;
                        if self.args.verbose {
                            println!(
                                "← No response after {} retries ({:.1}ms)",
                                self.args.read_retries,
                                start_time.elapsed().as_millis()
                            );
                        }
                        return Ok(Vec::new());
                    }
                }
                Err(e) => {
                    self.stats.errors += 1;
                    if self.args.verbose {
                        println!("← Read error: {} (continuing)", e);
                    }
                    return Ok(Vec::new()); // Continue operation
                }
            }
        }

        buffer.truncate(total_bytes);
        Ok(buffer)
    }

    fn send_command_with_response(&mut self, data: &[u8]) -> Result<Vec<u8>> {
        let command_type = data.get(0).copied().unwrap_or(0);

        // Send command
        self.write_command(data)?;

        // Add delay between command and response
        if self.args.command_delay > 0 {
            std::thread::sleep(Duration::from_millis(self.args.command_delay));
        }

        // Read response
        self.read_response(command_type)
    }

    fn print_stats(&self) {
        println!("\n=== Connection Statistics ===");
        println!("Commands sent:     {}", self.stats.commands_sent);
        println!("Responses received: {}", self.stats.responses_received);
        println!("Timeouts:          {}", self.stats.timeouts);
        println!("Errors:            {}", self.stats.errors);
        println!("Bytes sent:        {}", self.stats.bytes_sent);
        println!("Bytes received:    {}", self.stats.bytes_received);

        let success_rate = if self.stats.commands_sent > 0 {
            (self.stats.responses_received as f64 / self.stats.commands_sent as f64) * 100.0
        } else {
            0.0
        };
        println!("Response rate:     {:.1}%", success_rate);
    }

    fn run_test(&mut self) -> Result<()> {
        let test_start = Instant::now();
        let test_duration = if self.args.duration > 0 {
            Some(Duration::from_secs(self.args.duration))
        } else {
            None
        };

        // Set up Ctrl+C handler
        let running = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
        let r = running.clone();
        ctrlc::set_handler(move || {
            println!("\nReceived Ctrl+C, shutting down gracefully...");
            r.store(false, std::sync::atomic::Ordering::SeqCst);
        })
        .context("Error setting Ctrl+C handler")?;

        println!("\nStarting test sequence...");

        // GPIO setup
        println!("Setting up GPIO...");
        self.send_command_with_response(&[0xfe, 0, 0, 1])?;
        self.send_command_with_response(&[0xfe, 1, 0, 1])?;

        // Init1
        println!("Sending init1...");
        self.send_command_with_response(&[16, 49, 0, 0])?;
        self.send_command_with_response(&[16, 50, 64, 0])?;
        self.send_command_with_response(&[16, 51, 128, 0])?;

        // Init2
        println!("Sending init2...");
        self.send_command_with_response(&[17, 49, 64, 0])?;
        self.send_command_with_response(&[17, 50, 128, 0])?;
        self.send_command_with_response(&[17, 51, 0, 0])?;

        // Init3 - in chunks
        println!("Sending init3 (chunked)...");
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
            self.send_command_with_response(cmd)?;
            if self.args.verbose {
                println!("Init3 chunk {} completed", i + 1);
            }
        }

        // Keepalive test
        println!("Testing keepalive...");
        for i in 0..3 {
            self.send_command_with_response(&[0xfd, 0, 0, 0])?;
            if self.args.verbose {
                println!("Keepalive {} completed", i + 1);
            }
            std::thread::sleep(Duration::from_millis(500));
        }

        // Main loop
        println!("Starting main data loop (Ctrl+C to stop)...");
        let mut msg: Vec<u8> = vec![0, 0, 0, 0];
        let mut v: u16 = 0;
        let mut c: u8 = 0;
        let mut loop_count = 0;

        while running.load(std::sync::atomic::Ordering::SeqCst) {
            // Check test duration
            if let Some(duration) = test_duration {
                if test_start.elapsed() >= duration {
                    println!("Test duration reached, stopping...");
                    break;
                }
            }

            c = (c + 1) % 8;

            if v == 65535 {
                v = 0;
            } else if v < 65535 - 511 {
                v += 511;
            } else {
                v = 65535;
            }

            msg[0] = c;
            msg[1] = 0; // Direct write mode
            if c < 4 {
                msg[2] = ((v & 0xff00) >> 8) as u8;
                msg[3] = (v & 0xff) as u8;
            } else {
                msg[2] = (((65535 - v) & 0xff00) >> 8) as u8;
                msg[3] = ((65535 - v) & 0xff) as u8;
            }

            match self.send_command_with_response(&msg) {
                Ok(_) => {
                    if self.args.verbose || loop_count % 100 == 0 {
                        let value = ((msg[2] as u16) << 8) | (msg[3] as u16);
                        println!(
                            "Loop {}: DAC {} = 0x{:04x} ({})",
                            loop_count, c, value, value
                        );
                    }
                }
                Err(e) => {
                    eprintln!("Command failed: {}", e);
                    std::thread::sleep(Duration::from_millis(100));
                }
            }

            loop_count += 1;

            if self.args.rate > 0 {
                std::thread::sleep(Duration::from_millis(
                    (1000.0 / self.args.rate as f32) as u64,
                ));
            }
        }

        println!(
            "Test completed after {:.1} seconds",
            test_start.elapsed().as_secs_f64()
        );
        Ok(())
    }
}

fn main() -> Result<()> {
    let args = Args::parse();

    // Validate address format
    if !args.address.contains(':') {
        return Err(anyhow!(
            "Address must be in format IP:PORT (e.g., 192.168.1.100:8080)"
        ));
    }

    let mut client = RobustTcpClient::new(args)?;

    match client.run_test() {
        Ok(()) => {
            client.print_stats();
            println!("Test completed successfully.");
        }
        Err(e) => {
            client.print_stats();
            eprintln!("Test failed: {}", e);
            std::process::exit(1);
        }
    }

    Ok(())
}
