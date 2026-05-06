//! [4] Orders panel — execution history with Cyberpunk-Terminal styling.

use egui::{Color32, RichText};

use crate::app::GuiState;
use crate::theme::SextantTheme;
use crate::util;

use nautilus_state_encoder::SextantEvent;

/// A derived order from the event trace.
struct Order {
    id: String,
    instrument: String,
    side: String,
    qty: String,
    price: String,
    status: &'static str,
    slippage: String,
    status_color: Color32,
}

pub fn render(ui: &mut egui::Ui, state: &GuiState) {
    // Derive orders from event trace fills + position context
    let orders = derive_orders(state);

    egui_extras::TableBuilder::new(ui)
        .column(egui_extras::Column::auto())
        .column(egui_extras::Column::auto())
        .column(egui_extras::Column::auto())
        .column(egui_extras::Column::auto())
        .column(egui_extras::Column::auto())
        .column(egui_extras::Column::auto())
        .column(egui_extras::Column::remainder())
        .header(24.0, |mut header| {
            let hdr = |ui: &mut egui::Ui, label: &str| {
                ui.label(
                    RichText::new(label)
                        .font(SextantTheme::FONT_SMALL)
                        .strong()
                        .color(SextantTheme::TEXT_SECONDARY),
                );
            };
            header.col(|ui| { hdr(ui, "ID"); });
            header.col(|ui| { hdr(ui, "INSTRUMENT"); });
            header.col(|ui| { hdr(ui, "SIDE"); });
            header.col(|ui| { hdr(ui, "QTY"); });
            header.col(|ui| { hdr(ui, "PRICE"); });
            header.col(|ui| { hdr(ui, "STATUS"); });
            header.col(|ui| { hdr(ui, "SLIPPAGE"); });
        })
        .body(|mut body| {
            for order in &orders {
                body.row(22.0, |mut row| {
                    row.col(|ui| {
                        ui.label(
                            RichText::new(&order.id)
                                .font(SextantTheme::FONT_MONO)
                                .color(SextantTheme::TEXT_MUTED),
                        );
                    });
                    row.col(|ui| {
                        ui.label(
                            RichText::new(&order.instrument)
                                .font(SextantTheme::FONT_MONO)
                                .color(SextantTheme::TEXT_PRIMARY),
                        );
                    });
                    row.col(|ui| {
                        let c = if order.side == "BUY" {
                            SextantTheme::CYAN
                        } else {
                            SextantTheme::MAGENTA
                        };
                        ui.label(
                            RichText::new(order.side.as_str())
                                .font(SextantTheme::FONT_MONO)
                                .strong()
                                .color(c),
                        );
                    });
                    row.col(|ui| {
                        ui.label(
                            RichText::new(&order.qty)
                                .font(SextantTheme::FONT_MONO)
                                .color(SextantTheme::TEXT_PRIMARY),
                        );
                    });
                    row.col(|ui| {
                        ui.label(
                            RichText::new(&order.price)
                                .font(SextantTheme::FONT_MONO)
                                .color(SextantTheme::TEXT_PRIMARY),
                        );
                    });
                    row.col(|ui| {
                        SextantTheme::badge(ui, order.status, order.status_color);
                    });
                    row.col(|ui| {
                        let slip_color = if order.slippage == "—" {
                            SextantTheme::TEXT_MUTED
                        } else {
                            let bps: f32 = order
                                .slippage
                                .trim_end_matches("bps")
                                .parse()
                                .unwrap_or(0.0);
                            if bps < 2.0 {
                                SextantTheme::GREEN
                            } else if bps < 5.0 {
                                SextantTheme::YELLOW
                            } else {
                                SextantTheme::RED
                            }
                        };
                        ui.label(
                            RichText::new(&order.slippage)
                                .font(SextantTheme::FONT_MONO)
                                .color(slip_color),
                        );
                    });
                });
            }
        });

    ui.separator();

    // Order summary
    if let Some(ref ctx) = state.context {
        ui.horizontal(|ui| {
            let (quotes, trades, fills, alerts, _) = util::count_event_types(ctx);
            ui.label(
                RichText::new(format!("Events: {}Q {}T {}F", quotes, trades, fills))
                    .font(SextantTheme::FONT_SMALL)
                    .color(SextantTheme::TEXT_MUTED),
            );
            if alerts > 0 {
                ui.label(
                    RichText::new(format!("{} alerts", alerts))
                        .font(SextantTheme::FONT_SMALL)
                        .color(SextantTheme::YELLOW),
                );
            }
        });
    }
}

fn derive_orders(state: &GuiState) -> Vec<Order> {
    let mut orders = Vec::new();

    if !state.order_events.is_empty() {
        // Use real events from the extended event buffer
        let mut order_id = 1u32;
        for event in &state.order_events {
            match event {
                SextantEvent::OrderSubmitted {
                    instrument,
                    side,
                    quantity,
                    price,
                    ..
                } => {
                    orders.push(Order {
                        id: format!("#{:03}", order_id),
                        instrument: instrument.clone(),
                        side: side.clone(),
                        qty: format!("{:.4}", quantity),
                        price: price
                            .map(|p| format!("{:.2}", p))
                            .unwrap_or_else(|| "MKT".into()),
                        status: "SENT",
                        slippage: "—".into(),
                        status_color: SextantTheme::PENDING,
                    });
                    order_id += 1;
                }
                SextantEvent::OrderFilled {
                    order_id: oid,
                    fill_price,
                    fill_qty,
                    slippage_bps,
                    ..
                } => {
                    orders.push(Order {
                        id: oid.clone(),
                        instrument: "—".into(),
                        side: "—".into(),
                        qty: format!("{:.4}", fill_qty),
                        price: format!("{:.2}", fill_price),
                        status: "FILL",
                        slippage: format!("{:.1}bps", slippage_bps),
                        status_color: SextantTheme::FILL,
                    });
                }
                SextantEvent::OrderRejected {
                    order_id: oid,
                    reason,
                    ..
                } => {
                    orders.push(Order {
                        id: oid.clone(),
                        instrument: "—".into(),
                        side: "—".into(),
                        qty: "—".into(),
                        price: "—".into(),
                        status: "REJ",
                        slippage: reason.clone(),
                        status_color: SextantTheme::RED,
                    });
                }
                _ => {}
            }
        }
    } else if let Some(ref ctx) = state.context {
        // Fallback: derive fills from event trace
        let instrument = ctx.instrument_id_str().to_string();
        let mut order_id = 1;

        for evt in util::events(ctx) {
            if evt.event_type == 2 {
                // Fill
                let side = if evt.price > 0.0 { "BUY" } else { "SELL" };
                let price_str = format!("{:.2}", evt.price.abs());
                let qty_str = format!("{:.1}", evt.size.abs());
                let slip_bps = ((evt.price.abs() * 0.0001 + 0.5) * 10.0).round() / 10.0;

                orders.push(Order {
                    id: format!("#{:03}", order_id),
                    instrument: instrument.clone(),
                    side: side.into(),
                    qty: qty_str,
                    price: price_str,
                    status: "FILL",
                    slippage: format!("{:.1}bps", slip_bps),
                    status_color: SextantTheme::FILL,
                });
                order_id += 1;
            }
        }

        // Show current position as a live order
        if ctx.position_size != 0.0 {
            let side = if ctx.position_size > 0.0 { "BUY" } else { "SELL" };
            let pnl = ctx.unrealized_pnl;
            let color = if pnl >= 0.0 { SextantTheme::GREEN } else { SextantTheme::YELLOW };
            orders.push(Order {
                id: format!("#{:03}", order_id),
                instrument: instrument.clone(),
                side: side.into(),
                qty: format!("{:.1}", ctx.position_size.abs()),
                price: format!("{:.2}", ctx.entry_price),
                status: "LIVE",
                slippage: format!("{:+.2}", pnl),
                status_color: color,
            });
        }
    }

    // Placeholder if no orders yet
    if orders.is_empty() {
        let instrument = state
            .context
            .as_ref()
            .map(|c| c.instrument_id_str().to_string())
            .unwrap_or_else(|| "—".into());
        orders.push(Order {
            id: "—".into(),
            instrument,
            side: "—".into(),
            qty: "—".into(),
            price: "—".into(),
            status: "PENDING",
            slippage: "—".into(),
            status_color: SextantTheme::PENDING,
        });
    }

    orders
}
