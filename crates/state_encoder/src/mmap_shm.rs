//! File-backed mmap for cross-process SharedStateBuffer access.
//!
//! The engine writes ContextWindow to a file-backed SharedStateBuffer.
//! The TUI (or any other process) reads it via mmap — zero-copy, lock-free.

use std::path::Path;
use std::ptr;

use memmap2::{Mmap, MmapMut};

use crate::context_window::ContextWindow;

/// Engine-side writer: holds a mutable mmap of the SharedStateBuffer layout.
///
/// Layout in file:
/// - bytes 0..8:   seq (AtomicU64)
/// - bytes 8..N:   buffer[0] (ContextWindow)
/// - bytes N..2N:  buffer[1] (ContextWindow)
/// where N = size_of::<ContextWindow>()
pub struct MmapWriter {
    _file: std::fs::File,
    mmap: MmapMut,
}

impl MmapWriter {
    /// Create or open a file-backed shared memory buffer at `path`.
    pub fn open(path: impl AsRef<Path>) -> std::io::Result<Self> {
        let file_size = Self::file_size();
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(path.as_ref())?;
        file.set_len(file_size as u64)?;

        let mmap = unsafe { MmapMut::map_mut(&file)? };

        Ok(Self {
            _file: file,
            mmap,
        })
    }

    /// Write a ContextWindow to the shared memory file.
    pub fn write(&mut self, ctx: &ContextWindow) {
        let seq_ptr = self.mmap.as_mut_ptr() as *mut u64;
        let buf_base = unsafe { self.mmap.as_mut_ptr().add(8) };

        unsafe {
            // Read current seq
            let current = ptr::read_volatile(seq_ptr);
            // Flip active buffer, set writing bit
            let next = ((current >> 1) + 1) << 1 | 1;
            ptr::write_volatile(seq_ptr, next);

            let target = ((next >> 1) & 1) as usize;
            let dst = buf_base.add(target * std::mem::size_of::<ContextWindow>());
            ptr::copy_nonoverlapping(ctx as *const ContextWindow, dst as *mut ContextWindow, 1);

            // Clear writing bit
            ptr::write_volatile(seq_ptr, next & !1);
        }
    }

    /// Total file size needed.
    pub fn file_size() -> usize {
        8 + 2 * std::mem::size_of::<ContextWindow>()
    }
}

/// TUI-side reader: holds an immutable mmap for lock-free reading.
pub struct MmapReader {
    _file: std::fs::File,
    mmap: Mmap,
}

impl MmapReader {
    /// Open an existing shared memory file for reading.
    pub fn open(path: impl AsRef<Path>) -> std::io::Result<Self> {
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(false)
            .open(path.as_ref())?;

        let mmap = unsafe { Mmap::map(&file)? };

        Ok(Self {
            _file: file,
            mmap,
        })
    }

    /// Read the latest ContextWindow (lock-free, seqlock protocol).
    pub fn read(&self) -> Option<ContextWindow> {
        let seq_ptr = self.mmap.as_ptr() as *const u64;
        let buf_base = unsafe { self.mmap.as_ptr().add(8) };

        for _ in 0..100 {
            unsafe {
                let s = ptr::read_volatile(seq_ptr);

                // Odd = writing in progress
                if s & 1 != 0 {
                    std::hint::spin_loop();
                    continue;
                }

                let active = ((s >> 1) & 1) as usize;
                let src = buf_base.add(active * std::mem::size_of::<ContextWindow>());
                let mut ctx = ContextWindow::zeroed();
                ptr::copy_nonoverlapping(src as *const ContextWindow, &mut ctx, 1);

                let s2 = ptr::read_volatile(seq_ptr);
                if s == s2 {
                    return Some(ctx);
                }
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn test_mmap_write_read() {
        let tmp = NamedTempFile::new().unwrap();
        let path = tmp.path();

        let mut writer = MmapWriter::open(path).unwrap();
        let reader = MmapReader::open(path).unwrap();

        let mut ctx = ContextWindow::zeroed();
        ctx.version = 100;
        ctx.set_instrument_id("BTC-USDC");
        ctx.position_size = 3.14;
        ctx.set_market_state("bid:50000 | ask:50001");

        writer.write(&ctx);

        let read = reader.read().unwrap();
        assert_eq!(read.version, 100);
        assert_eq!(read.instrument_id_str(), "BTC-USDC");
        assert!((read.position_size - 3.14).abs() < 1e-10);
        assert_eq!(read.market_state_str(), "bid:50000 | ask:50001");
    }

    #[test]
    fn test_mmap_overwrite() {
        let tmp = NamedTempFile::new().unwrap();
        let path = tmp.path();

        let mut writer = MmapWriter::open(path).unwrap();
        let reader = MmapReader::open(path).unwrap();

        for i in 0..50u64 {
            let mut ctx = ContextWindow::zeroed();
            ctx.version = i;
            writer.write(&ctx);
        }

        let read = reader.read().unwrap();
        assert_eq!(read.version, 49);
    }

    #[test]
    fn test_mmap_file_size() {
        let size = MmapWriter::file_size();
        assert_eq!(size, 8 + 2 * std::mem::size_of::<ContextWindow>());
        // Should be 8 + 2 * ~4233 = ~8474 bytes
        assert!(size > 8000);
        assert!(size < 10000);
    }

    #[test]
    fn test_cross_thread_concurrent_read_write() {
        // Simulates cross-process behavior: one writer thread, one reader thread.
        // On Windows, file-backed mmap visibility is not instant — `read()` returning
        // `None` is expected (seq mismatch from stale page cache, not data corruption).
        // The critical invariant: every `Some(ctx)` has consistent, non-garbled data
        // and versions are monotonically non-decreasing.
        let tmp = NamedTempFile::new().unwrap();
        let path = tmp.path().to_owned();

        // Create the file and writer first so the file exists with correct size.
        let mut writer = MmapWriter::open(&path).unwrap();
        // Write initial data so reader can open a valid file.
        let mut ctx0 = ContextWindow::zeroed();
        ctx0.set_instrument_id("SOL-USDC");
        writer.write(&ctx0);

        let write_count = 5000u64;

        // Now both threads share the same file handle — move writer to its thread.
        let writer_handle = std::thread::spawn(move || {
            for i in 0..write_count {
                let mut ctx = ContextWindow::zeroed();
                ctx.version = i;
                ctx.position_size = i as f64 * 0.01;
                ctx.set_instrument_id("SOL-USDC");
                writer.write(&ctx);
            }
        });

        // Reader thread: reads until it sees the final version.
        // Small delay to ensure reader opens after file is created.
        std::thread::sleep(std::time::Duration::from_millis(10));
        let reader_handle = std::thread::spawn(move || {
            let reader = MmapReader::open(&path).unwrap();
            let mut successful_reads = 0u64;
            let mut last_version = 0u64;
            let mut consistency_violations = 0u64;

            loop {
                if let Some(ctx) = reader.read() {
                    successful_reads += 1;
                    // Data integrity: instrument_id must be preserved
                    if ctx.instrument_id_str() != "SOL-USDC" {
                        consistency_violations += 1;
                    }
                    // Version should be monotonically non-decreasing
                    if ctx.version < last_version {
                        consistency_violations += 1;
                    }
                    // Position must match version
                    let expected_pos = ctx.version as f64 * 0.01;
                    if (ctx.position_size - expected_pos).abs() > 1e-10 {
                        consistency_violations += 1;
                    }
                    last_version = ctx.version;
                    if last_version >= write_count - 1 {
                        break;
                    }
                }
                // `None` is expected on file-backed mmap — page cache visibility delay
            }

            (successful_reads, consistency_violations)
        });

        writer_handle.join().unwrap();
        let (successful, violations) = reader_handle.join().unwrap();

        assert!(successful > 0, "Reader should have read at least one snapshot");
        assert_eq!(violations, 0, "Data consistency violations: {}", violations);
    }
}
