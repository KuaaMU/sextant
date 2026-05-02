//! Mmap data source — reads ContextWindow from a file-backed mmap.

use nautilus_state_encoder::{ContextWindow, MmapReader};

/// Reads ContextWindow from a file-backed mmap created by an engine process.
pub struct MmapSource {
    reader: MmapReader,
    last_version: u64,
}

impl MmapSource {
    /// Open an existing mmap file for reading.
    pub fn open(path: &str) -> std::io::Result<Self> {
        let reader = MmapReader::open(path)?;
        Ok(Self {
            reader,
            last_version: 0,
        })
    }

    /// Read latest context, returning `Some` only if version changed.
    pub fn poll(&mut self) -> Option<ContextWindow> {
        let ctx = self.reader.read()?;
        if ctx.version > self.last_version {
            self.last_version = ctx.version;
            Some(ctx)
        } else {
            None
        }
    }

    /// Read latest context unconditionally.
    pub fn read_latest(&self) -> Option<ContextWindow> {
        self.reader.read()
    }
}
