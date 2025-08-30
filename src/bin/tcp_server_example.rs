use anyhow::{Context, Result};
use clap::Parser;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;

/// TCP server example for testing unified_test TCP transport
#[derive(Parser, Debug)]
#[command(name = "tcp_server_example")]
#[command(about = "TCP server that simulates DAC device responses")]
struct Args {
    /// Port to listen on
    #[arg(short, long, default_value = "8080")]
    port: u16,

    /// Bind address
    #[arg(short, long, default_value = "127.0.0.1")]
    address: String,

    /// Enable verbose output
    #[arg(short, long)]
    verbose: bool,
}

const STATUS_OK: u16 = 0x0000;
const STATUS_ERROR: u16 = 0xFFFF;

fn handle_client(mut stream: TcpStream, verbose: bool) -> Result<()> {
    let peer_addr = stream.peer_addr()?;
    println!("Client connected: {}", peer_addr);

    let mut buffer = [0u8; 1024];

    loop {
        match stream.read(&mut buffer) {
            Ok(0) => {
                // Client disconnected
                println!("Client {} disconnected", peer_addr);
                break;
            }
            Ok(bytes_read) => {
                if verbose {
                    println!(
                        "Received {} bytes from {}: {:?}",
                        bytes_read,
                        peer_addr,
                        &buffer[..bytes_read]
                    );
                }

                // Process commands in 4-byte chunks
                let mut responses = Vec::new();
                for chunk in buffer[..bytes_read].chunks(4) {
                    if chunk.len() == 4 {
                        let response = process_command(chunk, verbose);
                        responses.extend_from_slice(&response.to_be_bytes());
                    }
                }

                // Send responses back
                if !responses.is_empty() {
                    stream.write_all(&responses)?;
                    if verbose {
                        println!(
                            "Sent {} response bytes to {}: {:?}",
                            responses.len(),
                            peer_addr,
                            responses
                        );
                    }
                }
            }
            Err(e) => {
                eprintln!("Error reading from {}: {}", peer_addr, e);
                break;
            }
        }
    }

    Ok(())
}

fn process_command(cmd: &[u8], verbose: bool) -> u16 {
    if cmd.len() != 4 {
        return STATUS_ERROR;
    }

    let cmd_type = cmd[0];
    let param = cmd[1];
    let value = ((cmd[2] as u16) << 8) | (cmd[3] as u16);

    match cmd_type {
        0..=7 => {
            // Direct DAC write
            if param == 0x00 {
                if verbose {
                    println!(
                        "  -> Direct DAC write: channel={}, value=0x{:04X}",
                        cmd_type, value
                    );
                }
            } else if param >= 16 && param <= 19 {
                let table = param - 16;
                if verbose {
                    println!("  -> Attach table: channel={}, table={}", cmd_type, table);
                }
            } else {
                if verbose {
                    println!(
                        "  -> Unknown DAC command: channel={}, param={}",
                        cmd_type, param
                    );
                }
                return STATUS_ERROR;
            }
            STATUS_OK
        }
        16..=19 => {
            // Table write
            let table = cmd_type - 16;
            if verbose {
                println!(
                    "  -> Table write: table={}, offset={}, value=0x{:04X}",
                    table, param, value
                );
            }
            STATUS_OK
        }
        0xFF => {
            // Use table with offset
            if verbose {
                println!("  -> Use table: offset={}", param);
            }
            STATUS_OK
        }
        0xFE => {
            // GPIO control
            let state = if value == 0 { "OFF" } else { "ON" };
            if verbose {
                println!("  -> GPIO control: pin={}, state={}", param, state);
            }
            STATUS_OK
        }
        0xFD => {
            // Keep alive
            if verbose {
                println!("  -> Keep alive");
            }
            STATUS_OK
        }
        0xFC => {
            // LDAC update
            if verbose {
                println!("  -> LDAC update");
            }
            STATUS_OK
        }
        0xFB => {
            // Register write
            if verbose {
                println!("  -> Register write: reg={}, value=0x{:04X}", param, value);
            }
            STATUS_OK
        }
        _ => {
            if verbose {
                println!(
                    "  -> Unknown command: 0x{:02X} 0x{:02X} 0x{:04X}",
                    cmd_type, param, value
                );
            }
            STATUS_ERROR
        }
    }
}

fn main() -> Result<()> {
    let args = Args::parse();

    let bind_addr = format!("{}:{}", args.address, args.port);
    let listener = TcpListener::bind(&bind_addr)
        .with_context(|| format!("Failed to bind to {}", bind_addr))?;

    println!("TCP DAC simulator listening on {}", bind_addr);
    println!("Use Ctrl+C to stop the server");

    if args.verbose {
        println!("Verbose mode enabled - all commands will be logged");
    }

    // Set up Ctrl+C handler
    let running = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    let r = running.clone();
    ctrlc::set_handler(move || {
        println!("\nReceived Ctrl+C, shutting down server...");
        r.store(false, std::sync::atomic::Ordering::SeqCst);
    })
    .context("Error setting Ctrl+C handler")?;

    for stream in listener.incoming() {
        if !running.load(std::sync::atomic::Ordering::SeqCst) {
            break;
        }

        match stream {
            Ok(stream) => {
                let verbose = args.verbose;
                thread::spawn(move || {
                    if let Err(e) = handle_client(stream, verbose) {
                        eprintln!("Client handler error: {}", e);
                    }
                });
            }
            Err(e) => {
                eprintln!("Error accepting connection: {}", e);
            }
        }
    }

    println!("Server shutdown complete");
    Ok(())
}
