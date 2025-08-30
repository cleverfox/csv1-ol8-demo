use anyhow::{anyhow, Context, Result};
use clap::Parser;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Gauge, List, ListItem, Paragraph},
    Frame, Terminal,
};
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

/// TUI diagnostic tool for DAC control
#[derive(Parser, Debug)]
#[command(name = "tui_diagnostic")]
#[command(about = "Interactive TUI diagnostic tool for DAC control")]
struct Args {
    /// Connection target: serial device path or network address (IPv4:port, [IPv6]:port)
    target: String,

    /// DAC value step size for up/down keys
    #[arg(short, long, default_value = "256")]
    step: u16,

    /// Read timeout in milliseconds
    #[arg(long, default_value = "200")]
    read_timeout: u64,

    /// Write timeout in milliseconds
    #[arg(long, default_value = "1000")]
    write_timeout: u64,

    /// Keepalive interval in seconds
    #[arg(long, default_value = "5")]
    keepalive_interval: u64,
}

// Transport abstraction
trait Transport: Send {
    fn write_data(&mut self, data: &[u8]) -> Result<usize>;
    fn read_data(&mut self, buffer: &mut [u8]) -> Result<usize>;
    fn transport_type(&self) -> &'static str;
}

struct SerialTransport {
    port: Box<dyn serialport::SerialPort>,
}

impl SerialTransport {
    fn new(device_path: &str, read_timeout_ms: u64) -> Result<Self> {
        let port = serialport::new(device_path, 115_200)
            .timeout(Duration::from_millis(read_timeout_ms))
            .open()
            .with_context(|| format!("Failed to open serial port: {}", device_path))?;

        Ok(SerialTransport { port })
    }
}

impl Transport for SerialTransport {
    fn write_data(&mut self, data: &[u8]) -> Result<usize> {
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
            Err(_e) => Ok(0), // Continue on read errors
        }
    }

    fn transport_type(&self) -> &'static str {
        "Serial"
    }
}

struct TcpTransport {
    stream: TcpStream,
}

impl TcpTransport {
    fn new(address: &str, read_timeout_ms: u64, write_timeout_ms: u64) -> Result<Self> {
        let stream = TcpStream::connect(address)
            .with_context(|| format!("Failed to connect to TCP address: {}", address))?;

        stream.set_read_timeout(Some(Duration::from_millis(read_timeout_ms)))?;
        stream.set_write_timeout(Some(Duration::from_millis(write_timeout_ms)))?;
        stream.set_nodelay(true)?;

        Ok(TcpTransport { stream })
    }
}

impl Transport for TcpTransport {
    fn write_data(&mut self, data: &[u8]) -> Result<usize> {
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
            Err(_) => Ok(0),                                 // Continue on other read errors
        }
    }

    fn transport_type(&self) -> &'static str {
        "TCP"
    }
}

fn create_transport(target: &str, args: &Args) -> Result<Box<dyn Transport>> {
    if target.contains(':') {
        let _addr = target
            .to_socket_addrs()
            .with_context(|| format!("Invalid address format: {}", target))?
            .next()
            .ok_or_else(|| anyhow!("Could not resolve address: {}", target))?;

        Ok(Box::new(TcpTransport::new(
            target,
            args.read_timeout,
            args.write_timeout,
        )?))
    } else {
        Ok(Box::new(SerialTransport::new(target, args.read_timeout)?))
    }
}

#[derive(Debug, Clone)]
enum AppEvent {
    Input(KeyCode),
    Keepalive,
    TransportError(String),
    Response(Vec<u8>),
}

#[derive(Debug)]
struct AppState {
    dac_values: [u16; 8],
    gpio_states: [bool; 8],
    selected_channel: usize,
    step: u16,
    table_offset: u8,
    last_command: String,
    last_response: String,
    status_message: String,
    keepalive_count: u64,
}

impl AppState {
    fn new(step: u16) -> Self {
        Self {
            dac_values: [0; 8],
            gpio_states: [false; 8],
            selected_channel: 0,
            step,
            table_offset: 0,
            last_command: "Ready".to_string(),
            last_response: "No response yet".to_string(),
            status_message: "Connected".to_string(),
            keepalive_count: 0,
        }
    }
}

struct App {
    state: AppState,
    should_quit: bool,
}

impl App {
    fn new(step: u16) -> Self {
        Self {
            state: AppState::new(step),
            should_quit: false,
        }
    }

    fn handle_key(&mut self, key: KeyCode) -> Option<Vec<u8>> {
        match key {
            KeyCode::Char('q') | KeyCode::Esc => {
                self.should_quit = true;
                None
            }
            KeyCode::Left => {
                self.state.selected_channel = if self.state.selected_channel == 0 {
                    7
                } else {
                    self.state.selected_channel - 1
                };
                None
            }
            KeyCode::Right => {
                self.state.selected_channel = (self.state.selected_channel + 1) % 8;
                None
            }
            KeyCode::Up => {
                let ch = self.state.selected_channel;
                let new_value = self.state.dac_values[ch]
                    .saturating_add(self.state.step)
                    .min(65535);
                self.state.dac_values[ch] = new_value;
                self.state.last_command = format!("DAC {} = {}", ch, new_value);
                Some(self.build_dac_command(ch as u8, new_value))
            }
            KeyCode::Down => {
                let ch = self.state.selected_channel;
                let new_value = if self.state.dac_values[ch] >= self.state.step {
                    self.state.dac_values[ch] - self.state.step
                } else {
                    0
                };
                self.state.dac_values[ch] = new_value;
                self.state.last_command = format!("DAC {} = {}", ch, new_value);
                Some(self.build_dac_command(ch as u8, new_value))
            }
            KeyCode::Char('=') => {
                let ch = self.state.selected_channel;
                let new_value = self.state.dac_values[ch].saturating_add(16).min(65535);
                self.state.dac_values[ch] = new_value;
                self.state.last_command = format!("DAC {} = {}", ch, new_value);
                Some(self.build_dac_command(ch as u8, new_value))
            }
            KeyCode::Char('-') => {
                let ch = self.state.selected_channel;
                let new_value = if self.state.dac_values[ch] >= 16 {
                    self.state.dac_values[ch] - 16
                } else {
                    0
                };
                self.state.dac_values[ch] = new_value;
                self.state.last_command = format!("DAC {} = {}", ch, new_value);
                Some(self.build_dac_command(ch as u8, new_value))
            }
            KeyCode::Char(c) => match c {
                '0'..='9' => {
                    self.state.table_offset = c as u8 - b'0';
                    self.state.last_command = format!("Table offset = {}", self.state.table_offset);
                    Some(self.build_table_offset_command(self.state.table_offset))
                }
                'z' | 'Z' => {
                    self.state.gpio_states[0] = !self.state.gpio_states[0];
                    let state = self.state.gpio_states[0];
                    self.state.last_command =
                        format!("GPIO 0 = {}", if state { "ON" } else { "OFF" });
                    Some(self.build_gpio_command(0, state))
                }
                'x' | 'X' => {
                    self.state.gpio_states[1] = !self.state.gpio_states[1];
                    let state = self.state.gpio_states[1];
                    self.state.last_command =
                        format!("GPIO 1 = {}", if state { "ON" } else { "OFF" });
                    Some(self.build_gpio_command(1, state))
                }
                'c' | 'C' => {
                    self.state.gpio_states[2] = !self.state.gpio_states[2];
                    let state = self.state.gpio_states[2];
                    self.state.last_command =
                        format!("GPIO 2 = {}", if state { "ON" } else { "OFF" });
                    Some(self.build_gpio_command(2, state))
                }
                'v' | 'V' => {
                    self.state.gpio_states[3] = !self.state.gpio_states[3];
                    let state = self.state.gpio_states[3];
                    self.state.last_command =
                        format!("GPIO 3 = {}", if state { "ON" } else { "OFF" });
                    Some(self.build_gpio_command(3, state))
                }
                'b' | 'B' => {
                    self.state.gpio_states[4] = !self.state.gpio_states[4];
                    let state = self.state.gpio_states[4];
                    self.state.last_command =
                        format!("GPIO 4 = {}", if state { "ON" } else { "OFF" });
                    Some(self.build_gpio_command(4, state))
                }
                'n' | 'N' => {
                    self.state.gpio_states[5] = !self.state.gpio_states[5];
                    let state = self.state.gpio_states[5];
                    self.state.last_command =
                        format!("GPIO 5 = {}", if state { "ON" } else { "OFF" });
                    Some(self.build_gpio_command(5, state))
                }
                'm' | 'M' => {
                    self.state.gpio_states[6] = !self.state.gpio_states[6];
                    let state = self.state.gpio_states[6];
                    self.state.last_command =
                        format!("GPIO 6 = {}", if state { "ON" } else { "OFF" });
                    Some(self.build_gpio_command(6, state))
                }
                ',' => {
                    self.state.gpio_states[7] = !self.state.gpio_states[7];
                    let state = self.state.gpio_states[7];
                    self.state.last_command =
                        format!("GPIO 7 = {}", if state { "ON" } else { "OFF" });
                    Some(self.build_gpio_command(7, state))
                }
                ' ' => {
                    let ch = self.state.selected_channel;
                    let new_value = if self.state.dac_values[ch] == 65535 {
                        0 // Wrap to 0 only when already at maximum
                    } else {
                        self.state.dac_values[ch].saturating_add(8192).min(65535)
                    };
                    self.state.dac_values[ch] = new_value;
                    self.state.last_command = format!("DAC {} = {} (large step)", ch, new_value);
                    Some(self.build_dac_command(ch as u8, new_value))
                }
                _ => None,
            },
            _ => None,
        }
    }

    fn build_dac_command(&self, channel: u8, value: u16) -> Vec<u8> {
        vec![
            channel,
            0x00,
            ((value & 0xff00) >> 8) as u8,
            (value & 0xff) as u8,
        ]
    }

    fn build_gpio_command(&self, pin: u8, state: bool) -> Vec<u8> {
        vec![0xfe, pin, 0x00, if state { 0x01 } else { 0x00 }]
    }

    fn build_table_offset_command(&self, offset: u8) -> Vec<u8> {
        vec![0xff, offset, 0x00, 0x00]
    }

    fn build_keepalive_command(&self) -> Vec<u8> {
        vec![0xfd, 0x00, 0x00, 0x00]
    }

    fn handle_keepalive(&mut self) -> Vec<u8> {
        self.state.keepalive_count += 1;
        self.state.last_command = format!("Keepalive #{}", self.state.keepalive_count);
        self.build_keepalive_command()
    }
}

fn ui(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Title
            Constraint::Min(10),   // DAC sliders
            Constraint::Length(5), // GPIO status
            Constraint::Length(3), // Table offset
            Constraint::Length(3), // Last command
            Constraint::Length(8), // Help
        ])
        .split(f.size());

    // Title
    let title = Paragraph::new("DAC Control Panel - TUI Diagnostic Tool")
        .style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::ALL));
    f.render_widget(title, chunks[0]);

    // DAC Sliders
    render_dac_sliders(f, chunks[1], app);

    // GPIO Status
    render_gpio_status(f, chunks[2], app);

    // Table Offset
    let table_info = Paragraph::new(format!(
        "Table Offset: {} (0-9 keys)",
        app.state.table_offset
    ))
    .style(Style::default().fg(Color::Yellow))
    .alignment(Alignment::Center)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title("Table Control"),
    );
    f.render_widget(table_info, chunks[3]);

    // Last Command and Response
    let status_text = format!(
        "Last: {} | Response: {}",
        app.state.last_command, app.state.last_response
    );
    let last_cmd = Paragraph::new(status_text)
        .style(Style::default().fg(Color::Green))
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::ALL).title("Status"));
    f.render_widget(last_cmd, chunks[4]);

    // Help
    render_help(f, chunks[5]);
}

fn render_dac_sliders(f: &mut Frame, area: Rect, app: &App) {
    let constraints = vec![Constraint::Percentage(12); 8];
    let slider_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(constraints)
        .split(area);

    for (i, chunk) in slider_chunks.iter().enumerate() {
        let value = app.state.dac_values[i];
        let percentage = (value as f64 / 65535.0 * 100.0) as u16;

        let style = if i == app.state.selected_channel {
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Blue)
        };

        let gauge = Gauge::default()
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(format!("DAC{}", i))
                    .border_style(style),
            )
            .gauge_style(style)
            .percent(percentage)
            .label(format!("{}", value));

        f.render_widget(gauge, *chunk);
    }
}

fn render_gpio_status(f: &mut Frame, area: Rect, app: &App) {
    let constraints = vec![Constraint::Percentage(12); 8];
    let gpio_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(constraints)
        .split(area);

    for (i, chunk) in gpio_chunks.iter().enumerate() {
        let state = app.state.gpio_states[i];
        let style = if state {
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Gray)
        };

        let gpio_widget = Paragraph::new(if state { "ON" } else { "OFF" })
            .style(style)
            .alignment(Alignment::Center)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(format!("GPIO{}", i))
                    .border_style(style),
            );

        f.render_widget(gpio_widget, *chunk);
    }
}

fn render_help(f: &mut Frame, area: Rect) {
    let help_items = vec![
        ListItem::new("← → : Select DAC channel      ↑ ↓ : Adjust DAC value"),
        ListItem::new("SPACE : Large step (+8192)    0-9 : Set table offset"),
        ListItem::new("- =   : step by 16 (1 lsb)"),
        ListItem::new("ZXCVBNM, : Toggle GPIO 0-7    ESC/q : Quit application"),
    ];

    let help_list = List::new(help_items)
        .block(Block::default().borders(Borders::ALL).title("Controls"))
        .style(Style::default().fg(Color::White));

    f.render_widget(help_list, area);
}

fn run_transport_thread(
    mut transport: Box<dyn Transport>,
    cmd_rx: mpsc::Receiver<Vec<u8>>,
    event_tx: mpsc::Sender<AppEvent>,
) {
    let mut buffer = [0u8; 256];

    loop {
        match cmd_rx.try_recv() {
            Ok(command) => {
                if let Err(e) = transport.write_data(&command) {
                    let _ = event_tx.send(AppEvent::TransportError(format!("Write error: {}", e)));
                    continue;
                }

                // Try to read response (non-blocking)
                match transport.read_data(&mut buffer) {
                    Ok(bytes_read) => {
                        if bytes_read > 0 {
                            let _ =
                                event_tx.send(AppEvent::Response(buffer[..bytes_read].to_vec()));
                        }
                    }
                    Err(e) => {
                        let _ =
                            event_tx.send(AppEvent::TransportError(format!("Read error: {}", e)));
                    }
                }
            }
            Err(mpsc::TryRecvError::Empty) => {
                // No command to send, just continue
                thread::sleep(Duration::from_millis(10));
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                break; // Main thread closed
            }
        }
    }
}

fn main() -> Result<()> {
    let args = Args::parse();

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create app
    let mut app = App::new(args.step);

    // Create transport
    let transport = create_transport(&args.target, &args)?;
    println!(
        "Connected via {} to {}",
        transport.transport_type(),
        args.target
    );

    // Create channels for communication
    let (cmd_tx, cmd_rx) = mpsc::channel::<Vec<u8>>();
    let (event_tx, event_rx) = mpsc::channel::<AppEvent>();

    // Start transport thread
    let event_tx_clone = event_tx.clone();
    thread::spawn(move || {
        run_transport_thread(transport, cmd_rx, event_tx_clone);
    });

    // Start event input thread
    let event_tx_clone = event_tx.clone();
    thread::spawn(move || loop {
        if let Ok(event) = event::read() {
            if let Event::Key(key) = event {
                if key.kind == KeyEventKind::Press {
                    if event_tx_clone.send(AppEvent::Input(key.code)).is_err() {
                        break;
                    }
                }
            }
        }
    });

    // Start keepalive timer thread
    let keepalive_interval = Duration::from_secs(args.keepalive_interval);
    let event_tx_clone = event_tx.clone();
    thread::spawn(move || loop {
        thread::sleep(keepalive_interval);
        if event_tx_clone.send(AppEvent::Keepalive).is_err() {
            break;
        }
    });

    // Main loop
    let mut last_tick = Instant::now();
    let tick_rate = Duration::from_millis(250);

    loop {
        terminal.draw(|f| ui(f, &app))?;

        let timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or_else(|| Duration::from_secs(0));

        if let Ok(event) = event_rx.recv_timeout(timeout) {
            match event {
                AppEvent::Input(key) => {
                    if let Some(command) = app.handle_key(key) {
                        let _ = cmd_tx.send(command);
                    }
                    if app.should_quit {
                        break;
                    }
                }
                AppEvent::Keepalive => {
                    let command = app.handle_keepalive();
                    let _ = cmd_tx.send(command);
                }
                AppEvent::TransportError(err) => {
                    app.state.status_message = format!("Error: {}", err);
                }
                AppEvent::Response(response_data) => {
                    if response_data.is_empty() {
                        app.state.last_response = "No data".to_string();
                    } else {
                        app.state.last_response =
                            format!("{} bytes: {:02x?}", response_data.len(), response_data);
                    }
                }
            }
        }

        if last_tick.elapsed() >= tick_rate {
            last_tick = Instant::now();
        }
    }

    // Cleanup
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    Ok(())
}
