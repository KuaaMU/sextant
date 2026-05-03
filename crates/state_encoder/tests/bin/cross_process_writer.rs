//! Helper binary for cross-process integration test.
//!
//! Writes N ContextWindows to a file-backed mmap, simulating the engine process.
//!
//! Usage: cross_process_writer <path> <count>

use std::env;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 3 {
        eprintln!("Usage: {} <path> <count>", args[0]);
        std::process::exit(1);
    }

    let path = &args[1];
    let count: u64 = args[2].parse().expect("count must be a u64");

    let mut writer = nautilus_state_encoder::mmap_shm::MmapWriter::open(path)
        .expect("Failed to open mmap writer");

    for i in 0..count {
        let mut ctx = nautilus_state_encoder::context_window::ContextWindow::zeroed();
        ctx.version = i;
        ctx.position_size = i as f64 * 0.01;
        ctx.set_instrument_id("SOL-USDC");
        ctx.set_market_state(&format!("bid:{:.2} | ask:{:.2}", 100.0 + i as f64, 100.1 + i as f64));
        writer.write(&ctx);
    }

    // Ensure the final write is flushed to disk.
    // On Windows, mmap pages are flushed when the mapping is dropped.
    drop(writer);
}
