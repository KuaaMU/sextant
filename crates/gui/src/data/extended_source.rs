//! Reads SextantEvents from the extended event mmap file.

use nautilus_state_encoder::{ExtendedEventReader, SextantEvent};

pub struct ExtendedSource {
    reader: ExtendedEventReader,
}

impl ExtendedSource {
    pub fn open(path: &str) -> std::io::Result<Self> {
        let reader = ExtendedEventReader::open(path)?;
        Ok(Self { reader })
    }

    pub fn drain_events(&mut self) -> Vec<SextantEvent> {
        self.reader.drain_new()
    }
}
