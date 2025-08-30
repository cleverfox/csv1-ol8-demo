use anyhow::{anyhow, Context, Result};
use clap::Parser;
use serialport::SerialPort;
use std::io::{Read, Write};
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

/// TCP server that bridges serial communication to TCP for csv1-ol8 devices
#[derive(Parser, Debug)]
#[command(name = "tcp_server")]
#[command(about = "Serial-to-TCP bridge server for csv1-ol8 devices")]
struct Args {
    /// Serial device path (e.g., /dev/ttyACM0, /dev/cu.usbmodemcsv1_00011, COM5)
    serial_device: String,

    /// TCP port to listen on
    #[arg(short, long, default_value = "2012")]
    port: u16,

    /// Bind address (default: listen on all interfaces)
    #[arg(short, long)]
    bind: Option<String>,

    /// Enable verbose output for debugging
    #[arg(short, long)]
    verbose: bool,
}

/// Response format detector and handler
#[derive(Debug, Clone, Copy)]
enum ResponseType {
    /// Stadard format: 2 bytes [0x00, status_code]
    Stadard,
    /// Extended format: [0x01, payload_length, ...payload]
    Extended { payload_length: u8 },
}

impl ResponseType {
    /// Determine expected response length from first byte
    fn expected_length(&self) -> usize {
        match self {
            ResponseType::Stadard => 2,
            ResponseType::Extended { payload_length } => 2 + (*payload_length as usize),
        }
    }
}

/// Parse response header to determine format and expected length
fn parse_response_header(first_byte: u8, second_byte: Option<u8>) -> Result<ResponseType> {
    match first_byte {
        0x00 => {
            // Stadard format: [0x00, status_code]
            Ok(ResponseType::Stadard)
        }
        0x01 => {
            // Extended format: [0x01, payload_length, ...payload]
            match second_byte {
                Some(length) => Ok(ResponseType::Extended {
                    payload_length: length,
                }),
                None => Err(anyhow!(
                    "Extended response format requires payload length byte"
                )),
            }
        }
        _ => Err(anyhow!(
            "Unknown response format: first byte 0x{:02X}",
            first_byte
        )),
    }
}

/// Handle a single TCP client connection
fn handle_client(
    mut tcp_stream: TcpStream,
    serial_device: String,
    verbose: bool,
    shutdown_flag: Arc<AtomicBool>,
) -> Result<()> {
    let client_addr = tcp_stream.peer_addr()?;

    if verbose {
        println!("Client connected: {}", client_addr);
    }

    // Set TCP stream timeouts
    tcp_stream.set_read_timeout(Some(Duration::from_millis(100)))?;
    tcp_stream.set_write_timeout(Some(Duration::from_millis(1000)))?;

    // Open serial port
    let mut serial_port = serialport::new(&serial_device, 115_200)
        .timeout(Duration::from_millis(200))
        .data_bits(serialport::DataBits::Eight)
        .stop_bits(serialport::StopBits::One)
        .parity(serialport::Parity::None)
        .flow_control(serialport::FlowControl::None)
        .open()
        .with_context(|| format!("Failed to open serial port: {}", serial_device))?;

    if verbose {
        println!("Opened serial port: {} at 115200 8N1", serial_device);
    }

    let mut tcp_buffer = [0u8; 1024];
    let mut serial_buffer = [0u8; 1024];

    while !shutdown_flag.load(Ordering::Relaxed) {
        // Read from TCP client
        match tcp_stream.read(&mut tcp_buffer) {
            Ok(0) => {
                // Client disconnected
                if verbose {
                    println!("Client {} disconnected", client_addr);
                }
                break;
            }
            Ok(bytes_read) => {
                let request_data = &tcp_buffer[..bytes_read];

                if verbose {
                    println!("TCP → Serial: {} bytes: {:02X?}", bytes_read, request_data);
                }

                // Forward request to serial device (with padding to 4-byte boundary)
                let mut padded_data = request_data.to_vec();
                while padded_data.len() % 4 != 0 {
                    padded_data.push(0);
                }

                match serial_port.write_all(&padded_data) {
                    Ok(_) => {
                        if verbose && padded_data.len() != bytes_read {
                            println!(
                                "Serial write: {} bytes (padded from {}): {:02X?}",
                                padded_data.len(),
                                bytes_read,
                                padded_data
                            );
                        }
                    }
                    Err(e) => {
                        eprintln!("Serial write error: {}", e);
                        continue;
                    }
                }

                // Read response from serial device
                match read_serial_response(&mut *serial_port, &mut serial_buffer, verbose) {
                    Ok(response_data) => {
                        if !response_data.is_empty() {
                            if verbose {
                                println!(
                                    "Serial → TCP: {} bytes: {:02X?}",
                                    response_data.len(),
                                    response_data
                                );
                            }

                            // Forward response to TCP client
                            if let Err(e) = tcp_stream.write_all(&response_data) {
                                eprintln!("TCP write error to {}: {}", client_addr, e);
                                break;
                            }
                        }
                    }
                    Err(e) => {
                        if verbose {
                            eprintln!("Serial read error: {}", e);
                        }
                        // Continue operation even on read errors
                    }
                }
            }
            Err(e)
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut =>
            {
                // Timeout, continue listening
                continue;
            }
            Err(e) => {
                eprintln!("TCP read error from {}: {}", client_addr, e);
                break;
            }
        }
    }

    if verbose {
        println!("Connection to {} closed", client_addr);
    }

    Ok(())
}

/// Read complete response from serial device, handling both legacy and extended formats
fn read_serial_response(
    serial_port: &mut dyn SerialPort,
    buffer: &mut [u8],
    verbose: bool,
) -> Result<Vec<u8>> {
    // First, try to read at least 2 bytes for header
    let mut response_data = Vec::new();
    let mut bytes_needed = 2; // Start by reading header
    let mut total_read = 0;

    while total_read < bytes_needed && total_read < buffer.len() {
        match serial_port.read(&mut buffer[total_read..]) {
            Ok(0) => break, // No more data
            Ok(n) => {
                response_data.extend_from_slice(&buffer[total_read..total_read + n]);
                total_read += n;

                // Once we have at least 2 bytes, determine the response format
                if total_read >= 2 && bytes_needed == 2 {
                    match parse_response_header(response_data[0], Some(response_data[1])) {
                        Ok(response_type) => {
                            bytes_needed = response_type.expected_length();
                            if verbose {
                                match response_type {
                                    ResponseType::Stadard => {
                                        println!("Detected Stadard response format (2 bytes)");
                                    }
                                    ResponseType::Extended { payload_length } => {
                                        println!("Detected extended response format ({} bytes payload, {} total)",
                                               payload_length, bytes_needed);
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            if verbose {
                                eprintln!("Response format error: {}, treating as legacy", e);
                            }
                            bytes_needed = 2; // Fall back to legacy format
                        }
                    }
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::TimedOut => {
                // Timeout - return what we have if anything
                break;
            }
            Err(e) => {
                return Err(anyhow!("Serial read error: {}", e));
            }
        }
    }

    Ok(response_data)
}

/// Start TCP server on IPv4
fn start_ipv4_server(
    bind_addr: Ipv4Addr,
    port: u16,
    serial_device: String,
    verbose: bool,
    shutdown_flag: Arc<AtomicBool>,
) -> Result<()> {
    let socket_addr = SocketAddr::from((bind_addr, port));
    let listener = TcpListener::bind(socket_addr)
        .with_context(|| format!("Failed to bind to {}", socket_addr))?;

    // Set non-blocking mode to allow checking shutdown flag
    listener
        .set_nonblocking(true)
        .with_context(|| "Failed to set listener to non-blocking mode")?;

    println!("TCP server listening on {} (IPv4)", socket_addr);

    while !shutdown_flag.load(Ordering::Relaxed) {
        match listener.accept() {
            Ok((tcp_stream, _addr)) => {
                let serial_device_clone = serial_device.clone();
                let shutdown_flag_clone = shutdown_flag.clone();

                thread::spawn(move || {
                    if let Err(e) = handle_client(
                        tcp_stream,
                        serial_device_clone,
                        verbose,
                        shutdown_flag_clone,
                    ) {
                        eprintln!("Client handler error: {}", e);
                    }
                });
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                // No pending connections, sleep briefly and check shutdown flag again
                thread::sleep(Duration::from_millis(100));
                continue;
            }
            Err(e) => {
                eprintln!("Failed to accept connection: {}", e);
                thread::sleep(Duration::from_millis(100));
            }
        }
    }

    if verbose {
        println!("IPv4 server on {} shutting down", socket_addr);
    }

    Ok(())
}

/// Start TCP server on IPv6
fn start_ipv6_server(
    bind_addr: Ipv6Addr,
    port: u16,
    serial_device: String,
    verbose: bool,
    shutdown_flag: Arc<AtomicBool>,
) -> Result<()> {
    let socket_addr = SocketAddr::from((bind_addr, port));
    let listener = TcpListener::bind(socket_addr)
        .with_context(|| format!("Failed to bind to {}", socket_addr))?;

    // Set non-blocking mode to allow checking shutdown flag
    listener
        .set_nonblocking(true)
        .with_context(|| "Failed to set listener to non-blocking mode")?;

    println!("TCP server listening on {} (IPv6)", socket_addr);

    while !shutdown_flag.load(Ordering::Relaxed) {
        match listener.accept() {
            Ok((tcp_stream, _addr)) => {
                let serial_device_clone = serial_device.clone();
                let shutdown_flag_clone = shutdown_flag.clone();

                thread::spawn(move || {
                    if let Err(e) = handle_client(
                        tcp_stream,
                        serial_device_clone,
                        verbose,
                        shutdown_flag_clone,
                    ) {
                        eprintln!("Client handler error: {}", e);
                    }
                });
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                // No pending connections, sleep briefly and check shutdown flag again
                thread::sleep(Duration::from_millis(100));
                continue;
            }
            Err(e) => {
                eprintln!("Failed to accept connection: {}", e);
                thread::sleep(Duration::from_millis(100));
            }
        }
    }

    if verbose {
        println!("IPv6 server on {} shutting down", socket_addr);
    }

    Ok(())
}

fn main() -> Result<()> {
    let args = Args::parse();

    // Set up graceful shutdown handling
    let shutdown_flag = Arc::new(AtomicBool::new(false));
    let shutdown_flag_clone = shutdown_flag.clone();

    ctrlc::set_handler(move || {
        println!("\nReceived interrupt signal, shutting down...");
        shutdown_flag_clone.store(true, Ordering::Relaxed);
    })?;

    if args.verbose {
        println!(
            "Starting TCP server for serial device: {}",
            args.serial_device
        );
        println!("Server will listen on port {} (IPv4 and IPv6)", args.port);
    }

    // Determine bind addresses
    let (ipv4_addr, ipv6_addr): (Option<Ipv4Addr>, Option<Ipv6Addr>) = match &args.bind {
        Some(bind_str) => {
            // Parse specific bind address
            match bind_str.parse::<Ipv4Addr>() {
                Ok(addr) => (Some(addr), None),
                Err(_) => match bind_str.parse::<Ipv6Addr>() {
                    Ok(addr) => (None, Some(addr)),
                    Err(_) => {
                        return Err(anyhow!("Invalid bind address: {}", bind_str));
                    }
                },
            }
        }
        None => {
            // Bind to all interfaces
            (Some(Ipv4Addr::UNSPECIFIED), Some(Ipv6Addr::UNSPECIFIED))
        }
    };

    // Start servers
    let mut handles = Vec::new();

    // Start IPv4 server if requested
    if let Some(addr) = ipv4_addr {
        let serial_device = args.serial_device.clone();
        let shutdown_flag = shutdown_flag.clone();
        let handle = thread::spawn(move || {
            start_ipv4_server(addr, args.port, serial_device, args.verbose, shutdown_flag)
        });
        handles.push(handle);
    }

    // Start IPv6 server if requested
    if let Some(addr) = ipv6_addr {
        thread::sleep(Duration::from_millis(100)); //sleep a little bit to allow the IPv4 server to start
        let serial_device = args.serial_device.clone();
        let shutdown_flag = shutdown_flag.clone();
        let handle = thread::spawn(move || {
            start_ipv6_server(addr, args.port, serial_device, args.verbose, shutdown_flag)
        });
        handles.push(handle);
    }

    // Wait for all server threads
    for handle in handles {
        if let Err(e) = handle.join() {
            eprintln!("Server thread error: {:?}", e);
        }
    }

    println!("Server shutdown complete.");
    Ok(())
}
