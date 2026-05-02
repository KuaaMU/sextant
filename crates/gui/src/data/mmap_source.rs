//! Mmap data source — reads ContextWindow from engine's file-backed mmap.

use nautilus_state_encoder::{ContextWindow, MmapReader};

use super::data_source::DataSource;

pub struct MmapSource {
    reader: MmapReader,
    last_version: u64,
}

impl MmapSource {
    pub fn open(path: &str) -> std::io::Result<Self> {
        let reader = MmapReader::open(path)?;
        Ok(Self {
            reader,
            last_version: 0,
        })
    }
}

impl DataSource for MmapSource {
    fn try_read(&mut self) -> Option<ContextWindow> {
        let ctx = self.reader.read()?;
        if ctx.version > self.last_version {
            self.last_version = ctx.version;
            Some(ctx)
        } else {
            None
        }
    }
}
