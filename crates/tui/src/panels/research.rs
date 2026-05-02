//! [5] Research panel — Autoresearch Ratchet scatter + hypothesis list.

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
    let border_style = if app.active == crate::app::ActivePanel::Research {
        Style::default().fg(Theme::PANEL_BORDER_ACTIVE)
    } else {
        Style::default().fg(Theme::PANEL_BORDER)
    };

    let block = Block::default()
        .title(" [5] Research ")
        .borders(Borders::ALL)
        .border_style(border_style);

    let w = area.width as usize;
    let chart_w = w.saturating_sub(14).max(10);
    let axis = "─".repeat(chart_w);
    let hyp_sep_prefix = "── Latest Hypotheses ";
    let hyp_sep = "─".repeat(w.saturating_sub(hyp_sep_prefix.len() + 2));

    // Scale IR chart to available width
    let chart_body: Vec<Line<'static>> = {
        let scale = |s: &str| -> String {
            // Scale the chart body proportionally
            let base_w = 44usize;
            if w < 24 {
                s.chars().take(w.saturating_sub(2)).collect()
            } else if chart_w > base_w {
                // Pad with trailing spaces
                let mut out = s.to_string();
                out.extend(std::iter::repeat(' ').take(chart_w.saturating_sub(base_w)));
                out
            } else {
                s.chars().take(chart_w).collect()
            }
        };
        vec![
            Line::raw(scale(" 1.4 ┤                                          ╭── accepted")),
            Line::raw(scale(" 1.3 ┤                                     ╭───╯")),
            Line::raw(scale(" 1.2 ┤                ╭──────────────╮╭──╯")),
            Line::raw(scale(" 1.1 ┤           ╭──╯              ╰╯")),
            Line::raw(scale(" 1.0 ┤──────────╯")),
        ]
    };

    let mut lines = vec![
        Line::from(vec![
            Span::styled("Ratchet Status:", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            Span::raw(" Baseline IR = "),
            Span::styled("1.23", Style::default().fg(Theme::OK)),
            Span::raw("  │  Accepted: "),
            Span::styled("12", Style::default().fg(Theme::OK)),
            Span::raw("  Rejected: "),
            Span::styled("38", Style::default().fg(Theme::ALERT)),
        ]),
        Line::raw(""),
        Line::styled("IR Evolution", Style::default().fg(Theme::TEXT_DIM)),
    ];
    lines.extend(chart_body);
    lines.push(Line::from(vec![
        Span::styled("     └", Style::default().fg(Theme::TEXT_DIM)),
        Span::styled(axis, Style::default().fg(Theme::TEXT_DIM)),
    ]));
    lines.push(Line::styled("      round 0    25    50    75   100   125", Style::default().fg(Theme::TEXT_DIM)));
    lines.push(Line::raw(""));
    lines.push(Line::from(vec![
        Span::styled(hyp_sep_prefix, Style::default().fg(Theme::PANEL_BORDER_ACTIVE)),
        Span::styled(hyp_sep, Style::default().fg(Theme::PANEL_BORDER)),
    ]));
    lines.push(Line::from(vec![
        Span::styled("▼ #H-047", Style::default().fg(Theme::TEXT_DIM)),
        Span::raw("  \"Increase momentum window to 20 bars\"  "),
        Span::styled("● ACCEPTED", Style::default().fg(Theme::OK).add_modifier(Modifier::BOLD)),
    ]));
    lines.push(Line::from(vec![
        Span::raw("  IR: 1.32 → "),
        Span::styled("1.38 (+4.6%)", Style::default().fg(Theme::OK)),
        Span::raw("  │  Parent: #H-031"),
    ]));
    lines.push(Line::raw(""));
    lines.push(Line::from(vec![
        Span::styled("▶ #H-048", Style::default().fg(Theme::TEXT_DIM)),
        Span::raw("  \"Add volume filter\"  "),
        Span::styled("✗ REJECTED", Style::default().fg(Theme::ALERT).add_modifier(Modifier::BOLD)),
    ]));
    lines.push(Line::from(vec![
        Span::raw("  IR: 1.38 → "),
        Span::styled("1.35 (-2.2%)", Style::default().fg(Theme::ALERT)),
        Span::raw("  │  Parent: #H-047"),
    ]));
    lines.push(Line::raw(""));
    lines.push(Line::from(vec![
        Span::styled(" j/k:navigate  Enter:expand diff  r:trigger ratchet  h:history", Style::default().fg(Theme::TEXT_DIM)),
    ]));

    let paragraph = Paragraph::new(lines).block(block);
    f.render_widget(paragraph, area);
}
