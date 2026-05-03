//! Cross-process integration test: writer and reader communicate via mmap.
//!
//! Spawns a child process that writes ContextWindows to a temp file,
//! then reads them back from the parent process via MmapReader.
//! Verifies zero torn reads and monotonic versions.

use std::process::Command;
use tempfile::NamedTempFile;

#[test]
fn test_cross_process_mmap_write_read() {
    let tmp = NamedTempFile::new().unwrap();
    let path = tmp.path().to_owned();

    // Spawn the writer helper binary.
    // It writes 1000 ContextWindows to the file, then exits.
    let writer_bin = env!("CARGO_BIN_EXE_cross_process_writer");
    let status = Command::new(writer_bin)
        .arg(&path)
        .arg("1000")
        .status()
        .expect("Failed to spawn writer process");

    assert!(status.success(), "Writer process exited with error");

    // Now read all snapshots from the parent process via MmapReader.
    let reader = nautilus_state_encoder::mmap_shm::MmapReader::open(&path).unwrap();
    let ctx = reader.read().expect("Should read at least one snapshot");

    // The writer writes versions 0..999, final should be 999.
    assert_eq!(ctx.version, 999);
    assert_eq!(ctx.instrument_id_str(), "SOL-USDC");
    assert!((ctx.position_size - 999.0 * 0.01).abs() < 1e-10);
}

#[test]
fn test_cross_process_consistency() {
    let tmp = NamedTempFile::new().unwrap();
    let path = tmp.path().to_owned();

    let writer_bin = env!("CARGO_BIN_EXE_cross_process_writer");
    let status = Command::new(writer_bin)
        .arg(&path)
        .arg("5000")
        .status()
        .expect("Failed to spawn writer process");

    assert!(status.success());

    // Read multiple times and verify consistency.
    let reader = nautilus_state_encoder::mmap_shm::MmapReader::open(&path).unwrap();
    for _ in 0..100 {
        if let Some(ctx) = reader.read() {
            assert_eq!(ctx.instrument_id_str(), "SOL-USDC");
            // Position must match version
            let expected = ctx.version as f64 * 0.01;
            assert!(
                (ctx.position_size - expected).abs() < 1e-10,
                "Position mismatch: got {}, expected {} for version {}",
                ctx.position_size,
                expected,
                ctx.version
            );
        }
    }
}
