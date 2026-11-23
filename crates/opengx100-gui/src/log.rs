//! A log of what the worker does with the pedal, for finding out what happened after the fact:
//! `~/Library/Application Support/OpenGX100 Editor/logs/editor.log`, started fresh at each launch
//! (the previous run's log is kept as `editor-previous.log`). With `OPENGX100_LOG` set, lines also go
//! to stderr.

use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

/// Enough for hours of normal use; the tuner and the expression pedal could otherwise fill a disk.
const MAX_BYTES: u64 = 20_000_000;

struct Log {
    file: Option<File>,
    written: u64,
}

static LOG: Mutex<Option<Log>> = Mutex::new(None);

/// Where the log is written, if there is a data folder.
pub fn path() -> Option<PathBuf> {
    opengx100_extract::data_dir().map(|d| d.join("logs").join("editor.log"))
}

fn open() -> Log {
    let file = path().and_then(|p| {
        std::fs::create_dir_all(p.parent()?).ok()?;
        if p.exists() {
            let _ = std::fs::rename(&p, p.with_file_name("editor-previous.log"));
        }
        File::create(&p).ok()
    });
    Log { file, written: 0 }
}

/// Adds a timestamped line.
pub fn line(msg: &str) {
    let text = format!("{} {msg}\n", chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f"));
    if std::env::var_os("OPENGX100_LOG").is_some() {
        eprint!("{text}");
    }
    // Tests mustn't replace the user's log.
    if cfg!(test) {
        return;
    }
    let Ok(mut guard) = LOG.lock() else { return };
    let log = guard.get_or_insert_with(open);
    let Some(file) = log.file.as_mut() else { return };
    if log.written >= MAX_BYTES {
        return;
    }
    let text = if log.written + text.len() as u64 >= MAX_BYTES { "log full, nothing more is written\n" } else { &text };
    if file.write_all(text.as_bytes()).is_ok() {
        log.written += text.len() as u64;
    }
}

/// `bytes` as hex, e.g. `08 00 00 01`.
pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(" ")
}
