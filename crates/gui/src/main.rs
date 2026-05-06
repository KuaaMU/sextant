//! Sextant GUI — bridge viewport.
//!
//! Usage: sextant-gui [--mmap <path>] [--events <path>]

mod app;
mod data;
mod painter;
mod panels;
mod theme;
mod util;

use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use data::{DataSource, ExtendedSource, LogEntry, MmapSource, Simulator};

/// Data payload sent from the background thread to the UI.
pub struct DataPayload {
    pub context: nautilus_state_encoder::ContextWindow,
    pub logs: Vec<LogEntry>,
    pub events: Vec<nautilus_state_encoder::SextantEvent>,
}

fn main() {
    tracing_subscriber::fmt::init();

    // Parse CLI: --mmap <path> [--events <path>]
    let args: Vec<String> = std::env::args().collect();
    let mmap_path = args.windows(2).find_map(|w| {
        if w[0] == "--mmap" {
            Some(w[1].clone())
        } else {
            None
        }
    });
    let events_path = args.windows(2).find_map(|w| {
        if w[0] == "--events" {
            Some(w[1].clone())
        } else {
            None
        }
    });

    // Background data thread → UI via mpsc
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

        let mut ext_source: Option<ExtendedSource> = events_path.and_then(|p| {
            match ExtendedSource::open(&p) {
                Ok(s) => {
                    tracing::info!("Reading extended events from: {}", p);
                    Some(s)
                }
                Err(e) => {
                    tracing::warn!("Failed to open events '{}': {}", p, e);
                    None
                }
            }
        });

        let mut poll_interval = Duration::from_millis(8);
        let min_interval = Duration::from_millis(8);
        let max_interval = Duration::from_millis(50);

        loop {
            if let Some(ctx) = source.try_read() {
                let logs = source.drain_logs();
                let events = ext_source
                    .as_mut()
                    .map(|s| s.drain_events())
                    .unwrap_or_default();
                let _ = tx.send(DataPayload { context: ctx, logs, events });
                poll_interval = min_interval;
            } else {
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
        Box::new(move |_cc| Ok(Box::new(app::SextantApp::new(rx)))),
    )
    .expect("Failed to launch Sextant GUI");
}
