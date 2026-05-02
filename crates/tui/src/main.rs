//! Sextant TUI — terminal dashboard for the trading engine.

mod app;
mod data;
mod input;
mod panels;
mod theme;

use std::io::{self, Write};

use color_eyre::Result;
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame, Terminal,
};

use app::{ActivePanel, App};

/// Guard that restores terminal state on drop (panic, early return, or normal exit).
struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture);
        let _ = io::stdout().flush();
    }
}

fn main() -> Result<()> {
    color_eyre::install()?;

    // Install panic hook that restores terminal before printing the panic message.
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture);
        original_hook(info);
    }));

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Guard ensures terminal is restored even if run() returns early or panics.
    let _guard = TerminalGuard;

    // Parse CLI: `sextant-tui --mmap <path>` to read from engine, otherwise use simulator.
    let args: Vec<String> = std::env::args().collect();
    let mut app = if let Some(pos) = args.iter().position(|a| a == "--mmap") {
        if let Some(path) = args.get(pos + 1) {
            App::with_mmap(path)?
        } else {
            eprintln!("Usage: sextant-tui [--mmap <path>]");
            return Ok(());
        }
    } else {
        App::new()
    };

    let result = run(&mut terminal, &mut app);

    drop(_guard);
    terminal.show_cursor()?;

    result
}

fn run(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, app: &mut App) -> Result<()> {
    loop {
        // Handle input
        if let Some(key) = input::poll_key() {
            input::handle_key(app, key);
        }

        if !app.running {
            break;
        }

        // Tick simulator (generates new market data)
        app.tick();

        // Draw
        terminal.draw(|f| draw(f, app))?;
        app.frame_count += 1;

        // ~60fps
        std::thread::sleep(std::time::Duration::from_millis(16));
    }
    Ok(())
}

fn draw(f: &mut Frame, app: &App) {
    let size = f.area();

    // Main layout: tab bar + content + status bar
    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),  // Tab bar
            Constraint::Min(0),    // Content
            Constraint::Length(1),  // Status bar
        ])
        .split(size);

    // Tab bar
    render_tab_bar(f, main_chunks[0], app);

    // Content area — 3x2 grid for panels
    if app.zen_mode {
        // Zen mode: single panel fills content
        render_panel(f, main_chunks[1], app.active, app);
    } else {
        // Normal mode: 3 rows x 2 cols
        let content = main_chunks[1];
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Percentage(33),
                Constraint::Percentage(34),
                Constraint::Percentage(33),
            ])
            .split(content);

        let top_cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
            .split(rows[0]);

        let mid_cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
            .split(rows[1]);

        let bot_cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
            .split(rows[2]);

        // Row 1: Market (60%) + AgentLog (40%)
        render_panel(f, top_cols[0], ActivePanel::Market, app);
        render_panel(f, top_cols[1], ActivePanel::AgentLog, app);

        // Row 2: Risk (60%) + Orders (40%)
        render_panel(f, mid_cols[0], ActivePanel::Risk, app);
        render_panel(f, mid_cols[1], ActivePanel::Orders, app);

        // Row 3: Research (60%) + Memory (40%)
        render_panel(f, bot_cols[0], ActivePanel::Research, app);
        render_panel(f, bot_cols[1], ActivePanel::Memory, app);
    }

    // Status bar
    render_status_bar(f, main_chunks[2], app);

    // Help overlay
    if app.show_help {
        render_help(f, size);
    }
}

fn render_tab_bar(f: &mut Frame, area: Rect, app: &App) {
    let mut spans = vec![Span::styled(
        " Sextant TUI ",
        Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
    )];

    for panel in ActivePanel::ALL {
        let style = if app.active == panel {
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        spans.push(Span::raw(" "));
        spans.push(Span::styled(panel.label(), style));
    }

    let line = Line::from(spans);
    let paragraph = Paragraph::new(line).style(Style::default().bg(Color::Rgb(20, 20, 30)));
    f.render_widget(paragraph, area);
}

fn render_status_bar(f: &mut Frame, area: Rect, app: &App) {
    let paused = if app.paused {
        Span::styled(" PAUSED ", Style::default().fg(Color::Black).bg(Color::Yellow))
    } else {
        Span::styled(" ▶ Running ", Style::default().fg(Color::Black).bg(Color::Green))
    };

    let mode = if app.is_simulator() {
        Span::styled(" [SIM] ", Style::default().fg(Color::DarkGray))
    } else {
        Span::styled(" [LIVE] ", Style::default().fg(Color::Green))
    };

    let fps = Span::styled(
        format!(" {}fps ", 60),
        Style::default().fg(Color::DarkGray),
    );

    let help_hint = Span::styled(
        " Space:pause  ?:help  q:quit  z:zen ",
        Style::default().fg(Color::DarkGray),
    );

    let line = Line::from(vec![paused, mode, fps, help_hint]);
    let paragraph = Paragraph::new(line).style(Style::default().bg(Color::Rgb(20, 20, 30)));
    f.render_widget(paragraph, area);
}

fn render_panel(f: &mut Frame, area: Rect, panel: ActivePanel, app: &App) {
    match panel {
        ActivePanel::Market => panels::market::render(f, area, app),
        ActivePanel::AgentLog => panels::agent_log::render(f, area, app),
        ActivePanel::Risk => panels::risk::render(f, area, app),
        ActivePanel::Orders => panels::orders::render(f, area, app),
        ActivePanel::Research => panels::research::render(f, area, app),
        ActivePanel::Memory => panels::memory::render(f, area, app),
    }
}

fn render_help(f: &mut Frame, area: Rect) {
    // Centered help overlay
    let width = 44.min(area.width);
    let height = 16.min(area.height);
    let x = (area.width - width) / 2;
    let y = (area.height - height) / 2;
    let help_area = Rect::new(x, y, width, height);

    let block = Block::default()
        .title(" Help ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .style(Style::default().bg(Color::Rgb(20, 20, 30)));

    let lines = vec![
        Line::styled(" Global Keybindings", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
        Line::raw(""),
        Line::raw("  1-6       Focus panel by number"),
        Line::raw("  Tab       Next panel"),
        Line::raw("  S-Tab     Previous panel"),
        Line::raw("  Space     Pause/resume"),
        Line::raw("  z         Zen mode (fullscreen)"),
        Line::raw("  ?         Toggle this help"),
        Line::raw("  q         Quit"),
        Line::raw("  Ctrl+C    Force quit"),
        Line::raw(""),
        Line::styled(" Panel-specific keys", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
        Line::raw("  j/k       Navigate up/down"),
        Line::raw("  Enter     Expand/drill-down"),
        Line::raw("  /         Search"),
        Line::raw("  f         Filter/follow"),
        Line::raw(""),
        Line::styled(" Press any key to close", Style::default().fg(Color::DarkGray)),
    ];

    let paragraph = Paragraph::new(lines).block(block);
    f.render_widget(paragraph, help_area);
}
