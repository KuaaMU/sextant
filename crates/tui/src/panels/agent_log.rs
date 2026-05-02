//! [2] Agent Log panel — decision flow with block folding.

use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};
use crate::app::{AgentType, App};
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

    let mut lines = Vec::new();

    if app.log_entries.is_empty() {
        lines.push(Line::from(vec![
            Span::styled("  Waiting for data...", Style::default().fg(Theme::TEXT_DIM)),
        ]));
    } else {
        // Show last N entries that fit in the area
        let max_entries = (area.height as usize).saturating_sub(3); // leave room for header/filter
        let start = app.log_entries.len().saturating_sub(max_entries);

        for entry in &app.log_entries[start..] {
            let (type_color, type_label) = match entry.agent_type {
                AgentType::Perception => (Color::Magenta, "PERCEPTION"),
                AgentType::Strategy => (Color::Yellow, "STRATEGY"),
                AgentType::Risk => (Theme::ALERT, "RISK"),
                AgentType::Execution => (Theme::INFO, "EXECUTION"),
            };

            lines.push(Line::from(vec![
                Span::styled(
                    format!("{} ", entry.timestamp),
                    Style::default().fg(Theme::TEXT_DIM),
                ),
                Span::styled(
                    format!("{:<12}", type_label),
                    Style::default().fg(type_color).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!(" {:>3}ms ", entry.latency_ms),
                    Style::default().fg(Theme::TEXT_DIM),
                ),
                Span::styled(
                    entry.summary.clone(),
                    Style::default().fg(Theme::TEXT),
                ),
            ]));
        }
    }

    // Filter bar at bottom
    lines.push(Line::raw(""));
    lines.push(Line::from(vec![
        Span::styled(
            " Filter: [ALL] PERCEPTION STRATEGY RISK EXEC  /:search  f:follow",
            Style::default().fg(Theme::TEXT_DIM),
        ),
    ]));

    let paragraph = Paragraph::new(lines).block(block);
    f.render_widget(paragraph, area);
}
