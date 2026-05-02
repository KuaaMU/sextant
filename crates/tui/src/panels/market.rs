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

    let lines = vec![
        Line::from(vec![
            Span::styled("SOL-USDC", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            Span::raw("  @  "),
            Span::styled("150.23", Style::default().fg(Theme::OK).add_modifier(Modifier::BOLD)),
            Span::raw("  "),
            Span::styled("+2.3%", Style::default().fg(Theme::OK)),
        ]),
        Line::raw(""),
        Line::raw("Order Book                    Recent Trades"),
        Line::raw("───────────────────────────── ─────────────────────"),
        Line::raw(" Price    Size     Total       Time    Price   Size  Side"),
        Line::raw(" 150.50   12.5    ████████    11:14   150.23  0.5   BUY"),
        Line::raw(" 150.40   8.3     █████░░░    11:14   150.20  1.2   SELL"),
        Line::raw(" 150.30   45.2    █████████   11:13   150.25  0.3   BUY"),
        Line::raw("──────── best ask ──────────  11:13   150.30  2.0   SELL"),
        Line::raw(" 150.20   23.1    █████████   11:13   150.22  0.8   BUY"),
        Line::raw(" 150.10   67.4    ██████████  11:12   150.15  1.5   BUY"),
        Line::raw(" 150.00   120.0   ██████████"),
        Line::raw("──────── best bid ──────────"),
        Line::raw(""),
        Line::from(vec![
            Span::styled(" [1m] [5m] [15m] [1h]", Style::default().fg(Theme::TEXT_DIM)),
            Span::raw("  "),
            Span::styled("h/l:pan  +/-:zoom  j/k:instrument", Style::default().fg(Theme::TEXT_DIM)),
        ]),
    ];

    let paragraph = Paragraph::new(lines).block(block);
    f.render_widget(paragraph, area);
}
