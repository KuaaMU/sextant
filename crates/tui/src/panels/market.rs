//! [1] Market panel — real-time price + order book.

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
    let border_style = if app.active == crate::app::ActivePanel::Market {
        Style::default().fg(Theme::PANEL_BORDER_ACTIVE)
    } else {
        Style::default().fg(Theme::PANEL_BORDER)
    };

    let block = Block::default()
        .title(" [1] Market ")
        .borders(Borders::ALL)
        .border_style(border_style);

    // Build header from live data
    let header = if let Some(ref ctx) = app.context {
        let instrument = ctx.instrument_id_str().to_string();
        let price = app.current_price;
        let change_pct = ((price - 150.0) / 150.0 * 100.0 * 100.0).round() / 100.0;
        let color = if change_pct >= 0.0 { Theme::OK } else { Theme::ALERT };
        let arrow = if change_pct >= 0.0 { "+" } else { "" };

        Line::from(vec![
            Span::styled(instrument, Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            Span::raw("  @  "),
            Span::styled(format!("{:.2}", price), Style::default().fg(color).add_modifier(Modifier::BOLD)),
            Span::raw("  "),
            Span::styled(format!("{}{:.2}%", arrow, change_pct), Style::default().fg(color)),
        ])
    } else {
        Line::from(vec![
            Span::styled("SOL-USDC", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            Span::raw("  @  "),
            Span::styled("---", Style::default().fg(Theme::TEXT_DIM)),
        ])
    };

    // Sparkline from price history
    let sparkline = render_sparkline(&app.price_history.prices, area.width.saturating_sub(4) as usize);

    // Position info from live data
    let position_line = if let Some(ref ctx) = app.context {
        if ctx.position_size != 0.0 {
            let pnl_color = if ctx.unrealized_pnl >= 0.0 { Theme::OK } else { Theme::ALERT };
            Line::from(vec![
                Span::styled(" Position: ", Style::default().fg(Theme::TEXT_DIM)),
                Span::styled(format!("{:.1}", ctx.position_size), Style::default().fg(Theme::TEXT)),
                Span::raw("  PnL: "),
                Span::styled(format!("{:+.2}", ctx.unrealized_pnl), Style::default().fg(pnl_color)),
                Span::raw("  Entry: "),
                Span::styled(format!("{:.2}", ctx.entry_price), Style::default().fg(Theme::TEXT_DIM)),
            ])
        } else {
            Line::from(vec![
                Span::styled(" Position: ", Style::default().fg(Theme::TEXT_DIM)),
                Span::styled("flat", Style::default().fg(Theme::TEXT_DIM)),
            ])
        }
    } else {
        Line::raw("")
    };

    // Market state from live data
    let market_state_line = if let Some(ref ctx) = app.context {
        let state = ctx.market_state_str();
        Line::from(vec![
            Span::styled(" ", Style::default()),
            Span::styled(state, Style::default().fg(Theme::TEXT_DIM)),
        ])
    } else {
        Line::raw("")
    };

    let mut lines = vec![
        header,
        Line::raw(""),
    ];

    // Add sparkline (price chart)
    for line in sparkline {
        lines.push(line);
    }

    lines.push(Line::raw(""));
    lines.push(market_state_line);
    lines.push(position_line);
    lines.push(Line::raw(""));
    let status = if app.is_simulator() {
        format!(" Version: {}  [SIM]", app.context.as_ref().map_or(0, |c| c.version))
    } else {
        format!(" Version: {}  [LIVE]", app.context.as_ref().map_or(0, |c| c.version))
    };
    lines.push(Line::from(vec![
        Span::styled(&status, Style::default().fg(Theme::INFO)),
        Span::raw("  "),
        Span::styled("j/k:instrument  +/-:zoom  Space:pause", Style::default().fg(Theme::TEXT_DIM)),
    ]));

    let paragraph = Paragraph::new(lines).block(block);
    f.render_widget(paragraph, area);
}

/// Render a simple ASCII sparkline from price data.
fn render_sparkline(prices: &[f64], width: usize) -> Vec<Line<'static>> {
    if prices.len() < 2 || width < 10 {
        return vec![Line::raw("  (waiting for data...)")];
    }

    let min = prices.iter().cloned().fold(f64::INFINITY, f64::min);
    let max = prices.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let range = max - min;
    if range < 0.001 {
        return vec![Line::raw("  (flat)")];
    }

    // Use braille characters for sub-character resolution
    let chart_height: usize = 8;
    let mut grid = vec![vec![' '; width]; chart_height];

    // Sample prices to fit width
    let step = if prices.len() > width {
        prices.len() / width
    } else {
        1
    };

    let mut prev_y = 0usize;
    for (i, chunk) in prices.chunks(step).enumerate().take(width) {
        let price = chunk[chunk.len() - 1];
        let normalized = (price - min) / range;
        let y = chart_height - 1 - (normalized * (chart_height - 1) as f64) as usize;
        let y = y.min(chart_height - 1);

        if i > 0 {
            // Draw connecting line
            let y_from = prev_y.min(y);
            let y_to = prev_y.max(y);
            for yy in y_from..=y_to {
                grid[yy][i] = '│';
            }
        }
        grid[y][i] = '●';
        prev_y = y;
    }

    // Convert grid to lines with axis labels
    let mut result = Vec::new();
    for (row_idx, row) in grid.iter().enumerate() {
        let label = if row_idx == 0 {
            format!("{:>8.2} ┤", max)
        } else if row_idx == chart_height - 1 {
            format!("{:>8.2} ┤", min)
        } else if row_idx == chart_height / 2 {
            let mid = (max + min) / 2.0;
            format!("{:>8.2} ┤", mid)
        } else {
            "          │".to_string()
        };

        let line_str: String = row.iter().collect();
        result.push(Line::from(vec![
            Span::styled(label, Style::default().fg(Theme::TEXT_DIM)),
            Span::styled(line_str, Style::default().fg(Theme::INFO)),
        ]));
    }

    result
}
