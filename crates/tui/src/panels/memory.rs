//! [6] Memory panel — shared memory performance monitor.

use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};
use crate::app::App;
use crate::theme::Theme;

pub fn render(f: &mut Frame, area: Rect, app: &App) {
    let border_style = if app.active == crate::app::ActivePanel::Memory {
        Style::default().fg(Theme::PANEL_BORDER_ACTIVE)
    } else {
        Style::default().fg(Theme::PANEL_BORDER)
    };

    let block = Block::default()
        .title(" [6] Memory ")
        .borders(Borders::ALL)
        .border_style(border_style);

    let lines = vec![
        Line::from(vec![
            Span::styled("Shared Memory Status", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            Span::raw("                      "),
            Span::styled("Seqlock Monitor", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
        ]),
        Line::raw(""),
        Line::from(vec![
            Span::styled(" Write Latency Distribution (ns)", Style::default().fg(Theme::TEXT_DIM)),
            Span::raw("          Version: "),
            Span::styled("1,247,893", Style::default().fg(Theme::INFO)),
        ]),
        Line::from(vec![
            Span::styled("  50ns  ████████████████████  85%", Style::default().fg(Theme::OK)),
            Span::raw("     Writes/s: "),
            Span::styled("12,450", Style::default().fg(Theme::INFO)),
        ]),
        Line::from(vec![
            Span::styled(" 100ns  ████████░░░░░░░░░░░░  12%", Style::default().fg(Theme::WARN)),
            Span::raw("     Reads/s:  "),
            Span::styled("8,230", Style::default().fg(Theme::INFO)),
        ]),
        Line::from(vec![
            Span::styled(" 200ns  ██░░░░░░░░░░░░░░░░░░   2%", Style::default().fg(Theme::TEXT_DIM)),
            Span::raw("     Torn Reads: "),
            Span::styled("0", Style::default().fg(Theme::OK)),
        ]),
        Line::from(vec![
            Span::styled(" 500ns  █░░░░░░░░░░░░░░░░░░░   1%", Style::default().fg(Theme::TEXT_DIM)),
            Span::raw("     Lock Contention: "),
            Span::styled("0.01%", Style::default().fg(Theme::OK)),
        ]),
        Line::raw(""),
        Line::styled("Read Latency Sparkline (last 60s)", Style::default().fg(Theme::TEXT_DIM)),
        Line::raw(" 80ns ┤ ╭╮  ╭╮"),
        Line::raw(" 60ns ┤╭╯╰──╯╰──╮  ╭╮"),
        Line::raw(" 40ns ┤╯        ╰──╯╰──────────────────── avg: 47ns"),
        Line::raw("      └──────────────────────────────────────"),
        Line::raw(""),
        Line::from(vec![
            Span::raw(" ContextWindow: "),
            Span::styled("2,248 bytes", Style::default().fg(Theme::INFO)),
            Span::raw("  │  Event Trace: "),
            Span::styled("64/64 slots", Style::default().fg(Theme::WARN)),
        ]),
        Line::from(vec![
            Span::raw(" Instrument: "),
            Span::styled("SOL-USDC", Style::default().fg(Theme::TEXT)),
            Span::raw("             │  Last Update: "),
            Span::styled("11:14:24.123", Style::default().fg(Theme::TEXT_DIM)),
        ]),
        Line::raw(""),
        Line::from(vec![
            Span::styled(" r:reset  h:histogram  s:sparkline", Style::default().fg(Theme::TEXT_DIM)),
        ]),
    ];

    let paragraph = Paragraph::new(lines).block(block);
    f.render_widget(paragraph, area);
}
