// Z-SHOT — Copyright (C) 2026 R. Trunk
// SPDX-License-Identifier: GPL-3.0-or-later

//! Small diagnostic log at `%LOCALAPPDATA%\Z-SHOT\z-shot.log` (hotkey registration,
//! background events, window wake-ups). Rotated when it grows past `MAX_BYTES`.

use std::io::Write;
use std::sync::Mutex;

const MAX_BYTES: u64 = 512 * 1024;

static LOCK: Mutex<()> = Mutex::new(());

pub fn path() -> std::path::PathBuf {
    crate::model::album_dir().join("z-shot.log")
}

pub fn log(msg: impl AsRef<str>) {
    let _guard = LOCK.lock();
    let path = path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if std::fs::metadata(&path).map(|m| m.len() > MAX_BYTES).unwrap_or(false) {
        let _ = std::fs::rename(&path, path.with_extension("log.old"));
    }
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
        let _ = writeln!(f, "{now}  {}", msg.as_ref());
    }
}
