use std::fs::{File, OpenOptions};
use std::io;
use std::io::Write;

pub const LOG_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/log");

pub struct Logger {
    file: File,
}

impl Logger {
    pub fn new() -> io::Result<Self> {
        let file = OpenOptions::new().create(true).write(true).open(LOG_PATH)?;

        Ok(Self { file })
    }
}

impl Write for Logger {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let written = self.file.write(buf)?;

        // you cant see logs if your program segfaults lmao
        self.flush()?;

        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}
