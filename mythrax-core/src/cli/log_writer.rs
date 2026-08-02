use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::Mutex;

pub struct SizeRollingFileWriter {
    inner: Mutex<SizeRollingFileWriterInner>,
}

struct SizeRollingFileWriterInner {
    file: Option<File>,
    log_path: PathBuf,
}

impl SizeRollingFileWriter {
    pub fn new(log_path: PathBuf) -> io::Result<Self> {
        if let Some(parent) = log_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .append(true)
            .open(&log_path)?;
        Ok(Self {
            inner: Mutex::new(SizeRollingFileWriterInner {
                file: Some(file),
                log_path,
            }),
        })
    }

    fn write_all_and_roll(&self, buf: &[u8]) -> io::Result<()> {
        let mut inner = self.inner.lock().unwrap();

        // 1. Check current file size
        let mut needs_roll = false;
        if let Some(ref file) = inner.file {
            if let Ok(metadata) = file.metadata() {
                // Roll if current size + new data >= 50MB
                if metadata.len() + buf.len() as u64 >= 50 * 1024 * 1024 {
                    needs_roll = true;
                }
            }
        }

        // 2. Roll if needed
        if needs_roll {
            inner.file = None;

            let base_path = &inner.log_path;
            let path3 = base_path.with_extension("log.3");
            let path2 = base_path.with_extension("log.2");
            let path1 = base_path.with_extension("log.1");

            if path3.exists() {
                let _ = fs::remove_file(&path3);
            }
            if path2.exists() {
                let _ = fs::rename(&path2, &path3);
            }
            if path1.exists() {
                let _ = fs::rename(&path1, &path2);
            }
            if base_path.exists() {
                let _ = fs::rename(base_path, &path1);
            }

            let file = OpenOptions::new()
                .create(true)
                .write(true)
                .append(true)
                .open(base_path)?;
            inner.file = Some(file);
        }

        // 3. Write data
        if let Some(ref mut file) = inner.file {
            file.write_all(buf)?;
        }
        Ok(())
    }
}

impl Write for SizeRollingFileWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.write_all_and_roll(buf)?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        let mut inner = self.inner.lock().unwrap();
        if let Some(ref mut file) = inner.file {
            file.flush()?;
        }
        Ok(())
    }
}

impl Write for &SizeRollingFileWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.write_all_and_roll(buf)?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        let mut inner = self.inner.lock().unwrap();
        if let Some(ref mut file) = inner.file {
            file.flush()?;
        }
        Ok(())
    }
}
