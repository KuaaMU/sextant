//! [2] Agent Log panel — decision flow with block folding.

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
    let border_style = if app.active == crate::app::ActivePanel::AgentLog {
        Style::default().fg(Theme::PANEL_BORDER_ACTIVE)
    } else {
        Style::default().fg(Theme::PANEL_BORDER)
    };

    let block = Block::default()
        .title(" [2] Agent Log ")
        .borders(Borders::ALL)
        .border_style(border_style);

    let lines = vec![
        Line::from(vec![
            Span::styled("v 11:14:23", Style::default().fg(Theme::TEXT_DIM)),
            Span::raw("  "),
            Span::styled("PERCEPTION", Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD)),
            Span::raw("  12ms  regime:trend  confidence:0.82"),
        ]),
        Line::from(vec![
            Span::raw("  ├─ input:  "),
            Span::styled("ContextWindow{SOL-USDC, bid:150.20, ask:150.30}", Style::default().fg(Theme::TEXT_DIM)),
        ]),
        Line::from(vec![
            Span::raw("  └─ output: "),
            Span::styled("regime=TrendUp, volatility=0.023", Style::default().fg(Theme::OK)),
        ]),
        Line::raw(""),
        Line::from(vec![
            Span::styled("v 11:14:23", Style::default().fg(Theme::TEXT_DIM)),
            Span::raw("  "),
            Span::styled("STRATEGY", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::raw("     8ms  intent:TrendFollow  conf:0.75"),
        ]),
        Line::from(vec![
            Span::raw("  ├─ target: "),
            Span::styled("+50 SOL @ market", Style::default().fg(Theme::BUY)),
        ]),
        Line::from(vec![
            Span::raw("  └─ risk_budget: "),
            Span::styled("max_loss=$500, max_dd=50bps", Style::default().fg(Theme::WARN)),
        ]),
        Line::raw(""),
        Line::from(vec![
            Span::styled("v 11:14:23", Style::default().fg(Theme::TEXT_DIM)),
            Span::raw("  "),
            Span::styled("RISK", Style::default().fg(Theme::ALERT).add_modifier(Modifier::BOLD)),
            Span::raw("         1ms  gradient_mag:0.23  "),
            Span::styled("✓ pass", Style::default().fg(Theme::OK)),
        ]),
        Line::raw(""),
        Line::from(vec![
            Span::styled("v 11:14:23", Style::default().fg(Theme::TEXT_DIM)),
            Span::raw("  "),
            Span::styled("EXECUTION", Style::default().fg(Theme::INFO).add_modifier(Modifier::BOLD)),
            Span::raw("    3ms  style:Twap  slices:5"),
        ]),
        Line::from(vec![
            Span::raw("  └─ order: "),
            Span::styled("BUY 10 SOL @ market", Style::default().fg(Theme::BUY)),
            Span::raw("  →  fill: 150.25  slip: 2.1bps"),
        ]),
        Line::raw(""),
        Line::from(vec![
            Span::styled(" Filter: [ALL] PERCEPTION STRATEGY RISK EXEC", Style::default().fg(Theme::TEXT_DIM)),
            Span::raw("  /:search  f:follow"),
        ]),
    ];

    let paragraph = Paragraph::new(lines).block(block);
    f.render_widget(paragraph, area);
}
