//! Sextant GUI — egui desktop application.
//!
//! Usage: sextant-gui [--mmap <path>]

mod app;
mod data;
mod panels;
mod theme;
mod util;

use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use data::{DataSource, LogEntry, MmapSource, Simulator};

/// Data payload sent from the background thread to the UI.
pub struct DataPayload {
    pub context: nautilus_state_encoder::ContextWindow,
    pub logs: Vec<LogEntry>,
}

fn main() {
    tracing_subscriber::fmt::init();

    // Parse CLI: --mmap <path>
    let args: Vec<String> = std::env::args().collect();
    let mmap_path = args.windows(2).find_map(|w| {
        if w[0] == "--mmap" {
            Some(w[1].clone())
        } else {
            None
        }
    });

    // Background data thread → UI via mpsc
    // Unbounded channel; UI drains all pending frames each tick (keeping latest).
    // Adaptive polling backoff when no new data.
    let (tx, rx) = mpsc::channel();

    thread::spawn(move || {
        let mut source: Box<dyn DataSource> = match mmap_path {
            Some(ref p) => match MmapSource::open(p) {
                Ok(s) => {
                    tracing::info!("Reading from mmap: {}", p);
                    Box::new(s)
                }
                Err(e) => {
                    tracing::warn!("Failed to open mmap '{}': {}, falling back to simulator", p, e);
                    Box::new(Simulator::new(150.0))
                }
            },
            None => {
                tracing::info!("No --mmap, using simulator");
                Box::new(Simulator::new(150.0))
            }
        };

        // Adaptive polling: fast when data flows, back off when idle
        let mut poll_interval = Duration::from_millis(8); // ~120Hz
        let min_interval = Duration::from_millis(8);
        let max_interval = Duration::from_millis(50); // ~20Hz minimum

        loop {
            if let Some(ctx) = source.try_read() {
                let logs = source.drain_logs();
                let _ = tx.send(DataPayload { context: ctx, logs });
                poll_interval = min_interval;
            } else {
                // No new data — exponential backoff
                poll_interval = (poll_interval * 2).min(max_interval);
            }
            thread::sleep(poll_interval);
        }
    });

    // eframe window
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1920.0, 1080.0])
            .with_title("Sextant"),
        ..Default::default()
    };

    eframe::run_native(
        "Sextant",
        options,
        Box::new(move |cc| {
            // Restore persisted dock layout, always fresh receiver
            let mut app = app::SextantApp::new(rx);
            if let Some(storage) = cc.storage {
                if let Some(saved) = eframe::get_value::<egui_dock::DockState<app::Tab>>(
                    storage,
                    "dock_state",
                ) {
                    // Only restore if it has tabs (prevent blank screen)
                    if !saved.main_surface().is_empty() {
                        app.dock_state = saved;
                    }
                }
            }
            Ok(Box::new(app))
        }),
    )
    .expect("Failed to launch Sextant GUI");
}
