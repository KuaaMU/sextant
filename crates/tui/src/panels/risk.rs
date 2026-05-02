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

    let mut lines = Vec::new();

    if let Some(ref ctx) = app.context {
        let instrument = ctx.instrument_id_str();
        let risk_pot = ctx.risk_potential;
        let pos_pot = ctx.position_potential;
        let dd_pot = ctx.drawdown_potential;

        let risk_color = if risk_pot > 0.8 {
            Theme::ALERT
        } else if risk_pot > 0.5 {
            Theme::WARN
        } else {
            Theme::OK
        };

        let gradient = (pos_pot * pos_pot + dd_pot * dd_pot).sqrt();
        let grad_label = if gradient > 0.5 {
            "high"
        } else if gradient > 0.2 {
            "moderate"
        } else {
            "low"
        };

        lines.push(Line::from(vec![
            Span::styled(instrument, Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            Span::raw("  Risk Potential: "),
            Span::styled(format!("{:.2}", risk_pot), Style::default().fg(risk_color)),
            Span::raw("  │  Gradient: "),
            Span::styled(
                format!("{:.2} ({})", gradient, grad_label),
                Style::default().fg(risk_color),
            ),
        ]));
        lines.push(Line::raw(""));

        // Potential bars
        lines.push(potential_bar("Position", pos_pot, 1.0));
        lines.push(potential_bar("Drawdown", dd_pot, 1.0));
        lines.push(potential_bar("Concentration", risk_pot * 0.6, 1.0));

        lines.push(Line::raw(""));

        // Greeks
        lines.push(Line::raw("Greeks Matrix"));
        lines.push(Line::raw("──────────────────────────────────────────────────"));
        lines.push(Line::raw("           Delta      Gamma      Theta      Vega"));
        lines.push(Line::from(vec![
            Span::styled(
                format!(" {:<10} {:+.3}      {:.4}      {:+.1}      {:+.1}",
                    instrument, ctx.greeks.delta, ctx.greeks.gamma, ctx.greeks.theta, ctx.greeks.vega),
                Style::default().fg(Theme::TEXT),
            ),
        ]));
    } else {
        lines.push(Line::styled("  Waiting for data...", Style::default().fg(Theme::TEXT_DIM)));
    }

    lines.push(Line::raw(""));
    lines.push(Line::from(vec![
        Span::styled(" j/k:instrument  g:Greeks  p:potential  !:emergency close", Style::default().fg(Theme::TEXT_DIM)),
    ]));

    let paragraph = Paragraph::new(lines).block(block);
    f.render_widget(paragraph, area);
}

fn potential_bar(label: &str, value: f64, limit: f64) -> Line<'static> {
    let pct = ((value / limit) * 100.0).min(100.0) as u16;
    let color = if pct > 80 {
        Theme::ALERT
    } else if pct > 50 {
        Theme::WARN
    } else {
        Theme::OK
    };

    let filled = (pct as usize * 20) / 100;
    let empty = 20 - filled;
    let bar = format!("{}{}", "█".repeat(filled), "░".repeat(empty));

    Line::from(vec![
        Span::styled(format!(" {:<12}", label), Style::default().fg(Theme::TEXT_DIM)),
        Span::styled(format!("U = {:.2}  ", value), Style::default().fg(color)),
        Span::styled(bar, Style::default().fg(color)),
        Span::styled(format!("  {}%", pct), Style::default().fg(Theme::TEXT_DIM)),
    ])
}
