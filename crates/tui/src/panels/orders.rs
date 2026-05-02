//! [4] Orders panel — execution history with drill-down.

use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};
use crate::app::App;
use crate::theme::Theme;

pub fn render(f: &mut Frame, area: Rect, app: &App) {
    let border_style = if app.active == crate::app::ActivePanel::Orders {
        Style::default().fg(Theme::PANEL_BORDER_ACTIVE)
    } else {
        Style::default().fg(Theme::PANEL_BORDER)
    };

    let block = Block::default()
        .title(" [4] Orders ")
        .borders(Borders::ALL)
        .border_style(border_style);

    let w = area.width as usize;
    let sep = "─".repeat(w.saturating_sub(2));
    let selected_prefix = "── Selected: #002 ";
    let detail_sep = "─".repeat(w.saturating_sub(selected_prefix.len() + 2));

    let lines = vec![
        Line::from(vec![
            Span::styled("ID         Instrument  Side  Qty    Price    Status    Slippage", Style::default().fg(Theme::TEXT_DIM)),
        ]),
        Line::raw(sep),
        Line::from(vec![
            Span::styled(" #001  SOL-USDC   BUY    50.0  market   ", Style::default().fg(Theme::TEXT)),
            Span::styled("● FILL", Style::default().fg(Theme::FILL).add_modifier(Modifier::BOLD)),
            Span::styled("     2.1bps", Style::default().fg(Theme::TEXT)),
        ]),
        Line::from(vec![
            Span::styled(" #002  SOL-USDC   BUY    30.0  market   ", Style::default().fg(Theme::TEXT)),
            Span::styled("◐ PARTIAL", Style::default().fg(Theme::PARTIAL).add_modifier(Modifier::BOLD)),
            Span::styled("  3.5bps", Style::default().fg(Theme::TEXT)),
        ]),
        Line::from(vec![
            Span::styled(" #003  ETH-USDC   SELL   5.0   3200.00  ", Style::default().fg(Theme::TEXT)),
            Span::styled("◑ ACK", Style::default().fg(Theme::ACK).add_modifier(Modifier::BOLD)),
            Span::styled("    -", Style::default().fg(Theme::TEXT)),
        ]),
        Line::from(vec![
            Span::styled(" #004  SOL-USDC   BUY    20.0  market   ", Style::default().fg(Theme::TEXT)),
            Span::styled("○ PENDING", Style::default().fg(Theme::PENDING).add_modifier(Modifier::BOLD)),
            Span::styled("  -", Style::default().fg(Theme::TEXT)),
        ]),
        Line::from(vec![
            Span::styled(" #005  BTC-USDC   BUY    0.1   market   ", Style::default().fg(Theme::TEXT)),
            Span::styled("✗ DENY", Style::default().fg(Theme::DENY).add_modifier(Modifier::BOLD)),
            Span::styled("    -", Style::default().fg(Theme::TEXT)),
        ]),
        Line::raw(""),
        Line::from(vec![
            Span::styled(selected_prefix, Style::default().fg(Theme::PANEL_BORDER_ACTIVE)),
            Span::styled(detail_sep, Style::default().fg(Theme::PANEL_BORDER)),
        ]),
        Line::from(vec![
            Span::raw(" Intent: "),
            Span::styled("TrendFollow SOL-USDC +50", Style::default().fg(Theme::TEXT)),
            Span::raw(" → Compiler: "),
            Span::styled("Twap 5 slices", Style::default().fg(Theme::INFO)),
        ]),
        Line::from(vec![
            Span::raw(" Fill: "),
            Span::styled("30/50 @ avg 150.25", Style::default().fg(Theme::OK)),
            Span::raw("  │  Slippage: "),
            Span::styled("3.5bps", Style::default().fg(Theme::WARN)),
        ]),
        Line::raw(""),
        Line::from(vec![
            Span::styled(" j/k:navigate  Enter:detail  s:sort  f:filter  /:search", Style::default().fg(Theme::TEXT_DIM)),
        ]),
    ];

    let paragraph = Paragraph::new(lines).block(block);
    f.render_widget(paragraph, area);
}
