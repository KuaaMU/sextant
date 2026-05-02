//! [3] Risk panel — potential field + Greeks matrix.

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
    let border_style = if app.active == crate::app::ActivePanel::Risk {
        Style::default().fg(Theme::PANEL_BORDER_ACTIVE)
    } else {
        Style::default().fg(Theme::PANEL_BORDER)
    };

    let block = Block::default()
        .title(" [3] Risk ")
        .borders(Borders::ALL)
        .border_style(border_style);

    let lines = vec![
        Line::from(vec![
            Span::styled("SOL-USDC", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            Span::raw("  Risk Potential: "),
            Span::styled("0.52", Style::default().fg(Theme::WARN)),
            Span::raw("  │  Gradient: "),
            Span::styled("0.23 (moderate)", Style::default().fg(Theme::WARN)),
        ]),
        Line::raw(""),
        Line::from(vec![
            Span::styled("Position", Style::default().fg(Theme::TEXT_DIM)),
            Span::raw("              "),
            Span::styled("Drawdown", Style::default().fg(Theme::TEXT_DIM)),
            Span::raw("              "),
            Span::styled("Concentration", Style::default().fg(Theme::TEXT_DIM)),
        ]),
        Line::from(vec![
            Span::styled("U(p) = 0.45     ", Style::default().fg(Theme::WARN)),
            Span::styled("U(d) = 0.08     ", Style::default().fg(Theme::OK)),
            Span::styled("U(w) = 0.31     ", Style::default().fg(Theme::WARN)),
        ]),
        Line::from(vec![
            Span::styled("45% of limit    ", Style::default().fg(Theme::TEXT_DIM)),
            Span::styled("8% of limit     ", Style::default().fg(Theme::TEXT_DIM)),
            Span::styled("31% of limit    ", Style::default().fg(Theme::TEXT_DIM)),
        ]),
        Line::raw(""),
        Line::raw("Greeks Matrix"),
        Line::raw("──────────────────────────────────────────────────"),
        Line::raw("           Delta      Gamma      Theta      Vega"),
        Line::from(vec![
            Span::styled(" SOL-USDC  +0.45      0.012      -23.5      +12.3", Style::default().fg(Theme::TEXT)),
        ]),
        Line::from(vec![
            Span::styled(" ETH-USDC  -0.30      0.008      -15.2      +8.7 ", Style::default().fg(Theme::TEXT_DIM)),
        ]),
        Line::from(vec![
            Span::styled(" BTC-USDC  +0.15      0.003       -8.1      +5.2 ", Style::default().fg(Theme::TEXT_DIM)),
        ]),
        Line::raw(""),
        Line::from(vec![
            Span::styled(" j/k:instrument  g:Greeks  p:potential  !:emergency close", Style::default().fg(Theme::TEXT_DIM)),
        ]),
    ];

    let paragraph = Paragraph::new(lines).block(block);
    f.render_widget(paragraph, area);
}
