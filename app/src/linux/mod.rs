// Z-SHOT — Copyright (C) 2026 R. Trunk
// SPDX-License-Identifier: GPL-3.0-or-later

//! Linux desktop integration, built for Wayland (works on X11 desktops too).
//!
//! Wayland lets no application read the screen, grab global keys or hide and
//! re-show its own window at will. Z-SHOT therefore runs as two kinds of process:
//!
//! - the **daemon** (`z-shot --background`, also started by `z-shot` or the
//!   autostart entry): no window; tray icon (StatusNotifierItem), global shortcuts
//!   (GlobalShortcuts portal), clipboard owner and a local socket for commands;
//! - a **UI process** (`z-shot --ui <region|full|show|settings>`) per use, started by
//!   the daemon. For a capture it takes the screenshot (Screenshot portal) *before*
//!   its window exists, shows the region overlay and the editor, and exits when the
//!   window is closed. The album lives on disk, so nothing is lost in between.
//!
//! `z-shot --capture region|full` sends a capture to the daemon. Desktops without
//! the GlobalShortcuts portal can bind that command to a key in their settings.

pub mod daemon;
pub mod desktop;
pub mod ipc;
pub mod portal;

use std::sync::Mutex;

/// Application id: name of the desktop entry, icon and Wayland app id. Portals use
/// it to remember permissions (screenshots, shortcuts).
pub const APP_ID: &str = "io.github.raytrunk.ZShot";

/// Runs `f` to completion on a fresh single-threaded tokio runtime (zbus, which the
/// portals use, needs one).
pub fn block_on<F: std::future::Future>(f: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime")
        .block_on(f)
}

/// A capture asked for from inside the editor: the UI process closes its window and,
/// once it is gone, has the daemon start a fresh capture (see [`finish_ui_process`]).
static RELAUNCH: Mutex<Option<ipc::Command>> = Mutex::new(None);

pub fn request_relaunch(cmd: ipc::Command) {
    if let Ok(mut r) = RELAUNCH.lock() {
        *r = Some(cmd);
    }
}

/// Called by `main` after the UI window has closed.
pub fn finish_ui_process() {
    ipc::remove_socket(ipc::Socket::Ui);
    let relaunch = RELAUNCH.lock().ok().and_then(|mut r| r.take());
    if let Some(cmd) = relaunch {
        // Give the compositor a moment to take the closed window off the screen,
        // so it is not in the next screenshot.
        // Started from here, not via the daemon: to the daemon this process may
        // still look like a window that is starting up.
        std::thread::sleep(std::time::Duration::from_millis(250));
        daemon::spawn_ui(&cmd);
    }
}
