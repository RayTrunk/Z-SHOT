// Z-SHOT — Copyright (C) 2026 R. Trunk
// SPDX-License-Identifier: GPL-3.0-or-later

//! Small diagnostic log at `%LOCALAPPDATA%\Z-SHOT\z-shot.log` (hotkey registration,
//! background events, window wake-ups).
//!
//! The file is opened once and kept open, so a log line costs one write, also on the
//! hotkey thread. It is rotated to `.log.old` at startup when it has grown past
//! `MAX_BYTES`.

use std::fs::File;
use std::io::Write;
use std::sync::Mutex;

const MAX_BYTES: u64 = 512 * 1024;

static FILE: Mutex<Option<File>> = Mutex::new(None);

pub fn path() -> std::path::PathBuf {
    crate::model::album_dir().join("z-shot.log")
}

fn open() -> Option<File> {
    let path = path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if std::fs::metadata(&path).map(|m| m.len() > MAX_BYTES).unwrap_or(false) {
        let _ = std::fs::rename(&path, path.with_extension("log.old"));
    }
    std::fs::OpenOptions::new().create(true).append(true).open(&path).ok()
}

pub fn log(msg: impl AsRef<str>) {
    let Ok(mut guard) = FILE.lock() else { return };
    if guard.is_none() {
        *guard = open();
    }
    if let Some(f) = guard.as_mut() {
        let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
        let _ = writeln!(f, "{now}  {}", msg.as_ref());
    }
}
