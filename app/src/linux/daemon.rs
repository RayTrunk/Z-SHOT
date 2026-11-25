// Z-SHOT — Copyright (C) 2026 R. Trunk
// SPDX-License-Identifier: GPL-3.0-or-later

//! The background process: tray icon, global shortcuts, clipboard owner, update
//! check, and the one that opens a UI process for every capture (see `super`).

use super::ipc::{self, Command, ListenError, Socket};
use crate::i18n::tr;
use crate::settings::Settings;
use ksni::menu::StandardItem;
use ksni::{MenuItem, TrayMethods};
use std::path::Path;
use std::process::Child;
use tokio::sync::mpsc::{self, UnboundedSender};

/// Runs the daemon until "Quit". If one is running already, `initial` is handed
/// to it instead and this returns at once.
pub fn run(initial: Option<Command>) -> Result<(), String> {
    let (tx, mut rx) = mpsc::unbounded_channel::<Command>();
    let ipc_tx = tx.clone();
    match ipc::listen(Socket::Daemon, move |cmd| {
        let _ = ipc_tx.send(cmd);
    }) {
        Ok(()) => {}
        Err(ListenError::AlreadyRunning) => {
            if let Some(cmd) = initial {
                ipc::send(Socket::Daemon, &cmd);
            }
            return Ok(());
        }
        Err(ListenError::Io(e)) => return Err(e.to_string()),
    }
    crate::log::log(format!("Z-SHOT {} im Hintergrund gestartet", env!("CARGO_PKG_VERSION")));

    super::desktop::install();
    let album_dir = crate::model::album_dir();
    let settings_path = album_dir.join("settings.json");
    let settings = Settings::load(&settings_path);
    let updater = crate::update::Checker::start(album_dir, settings.check_updates, || {});
    if let Some(cmd) = initial {
        let _ = tx.send(cmd);
    }

    super::block_on(async move {
        super::portal::register_app().await;

        let hint = settings.region_hotkeys_label();
        let tray = ZTray { tx: tx.clone(), hint: hint.clone() };
        // Inside Flatpak the sandbox may not own the usual per-process D-Bus name.
        let _tray = match tray.disable_dbus_name(super::desktop::is_flatpak()).spawn().await {
            Ok(handle) => Some(handle),
            Err(e) => {
                crate::log::log(format!("Kein Symbol im Infobereich: {e}"));
                None
            }
        };
        let start_text = if _tray.is_some() {
            tr(
                format!("{hint} oder Klick auf das Symbol nimmt einen Bereich auf."),
                format!("{hint} or a click on the icon captures a region."),
            )
        } else {
            tr(
                format!("{hint} nimmt einen Bereich auf. Das Fenster öffnen Sie über das Anwendungsmenü."),
                format!("{hint} captures a region. Open the window from the application menu."),
            )
        };
        super::desktop::notify_async(tr("Z-SHOT läuft im Hintergrund", "Z-SHOT is running in the background"), &start_text).await;

        let start_shortcuts = |bindings| {
            let (stop_tx, stop_rx) = tokio::sync::oneshot::channel::<()>();
            let task = tokio::spawn(super::portal::run_shortcuts(bindings, tx.clone(), stop_rx));
            (stop_tx, task)
        };
        let mut shortcuts = start_shortcuts(settings.hotkey_bindings());
        let mut ui: Option<Child> = None;
        let mut clipboard: Option<arboard::Clipboard> = None;

        while let Some(cmd) = rx.recv().await {
            crate::log::log(format!("Hintergrund: {cmd:?}"));
            match cmd {
                Command::Quit => {
                    ipc::send(Socket::Ui, &Command::Quit);
                    stop_shortcuts(shortcuts).await;
                    break;
                }
                Command::Shortcuts => {
                    // Close the old portal session before opening a new one.
                    stop_shortcuts(shortcuts).await;
                    shortcuts = start_shortcuts(Settings::load(&settings_path).hotkey_bindings());
                }
                Command::Reload => {
                    updater.set_enabled(Settings::load(&settings_path).check_updates);
                }
                Command::Clipboard(path) => set_clipboard(&mut clipboard, &path),
                other => open_ui(&mut ui, &other),
            }
        }
    });

    ipc::remove_socket(Socket::Daemon);
    Ok(())
}

/// Asks the shortcuts task to close its portal session. It only notices between two
/// portal answers, so a dialog left open would block: after two seconds it is
/// dropped instead.
async fn stop_shortcuts((stop, task): (tokio::sync::oneshot::Sender<()>, tokio::task::JoinHandle<()>)) {
    let _ = stop.send(());
    let abort = task.abort_handle();
    if tokio::time::timeout(std::time::Duration::from_secs(2), task).await.is_err() {
        abort.abort();
    }
}

/// Hands `cmd` to the open window, or starts a UI process for it.
fn open_ui(ui: &mut Option<Child>, cmd: &Command) {
    if ipc::send(Socket::Ui, cmd) {
        return;
    }
    if let Some(child) = ui.as_mut() {
        // Started a moment ago and not listening yet: a second hotkey press must
        // not open a second window.
        if matches!(child.try_wait(), Ok(None)) && !ipc::is_running(Socket::Ui) {
            crate::log::log("Fenster startet noch, Befehl übersprungen");
            return;
        }
    }
    *ui = spawn_ui(cmd);
}

/// Starts `z-shot --ui <cmd>`.
pub fn spawn_ui(cmd: &Command) -> Option<Child> {
    let arg = cmd.ui_arg()?;
    let exe = std::env::current_exe().ok()?;
    match std::process::Command::new(exe).args(["--ui", arg]).spawn() {
        Ok(child) => Some(child),
        Err(e) => {
            crate::log::log(format!("Fenster nicht gestartet: {e}"));
            None
        }
    }
}

/// Puts a PNG handed over by a UI process on the clipboard. The daemon keeps the
/// clipboard alive: on Linux its content disappears with the process that set it,
/// and a UI process often ends right after a capture.
fn set_clipboard(clipboard: &mut Option<arboard::Clipboard>, path: &Path) {
    if !ipc::is_runtime_file(path) {
        crate::log::log(format!("Zwischenablage: Datei abgelehnt: {}", path.display()));
        return;
    }
    let img = image::open(path).map(|i| i.to_rgba8());
    let _ = std::fs::remove_file(path);
    let img = match img {
        Ok(img) => img,
        Err(e) => {
            crate::log::log(format!("Zwischenablage: Bild nicht lesbar: {e}"));
            return;
        }
    };
    if clipboard.is_none() {
        match arboard::Clipboard::new() {
            Ok(c) => *clipboard = Some(c),
            Err(e) => {
                crate::log::log(format!("Zwischenablage nicht verfügbar: {e}"));
                return;
            }
        }
    }
    if let Some(c) = clipboard.as_mut() {
        let (width, height) = img.dimensions();
        let data = arboard::ImageData {
            width: width as usize,
            height: height as usize,
            bytes: std::borrow::Cow::Owned(img.into_raw()),
        };
        if let Err(e) = c.set_image(data) {
            crate::log::log(format!("Zwischenablage: {e}"));
        }
    }
}

// ---------------------------------------------------------------------------
// Tray icon (StatusNotifierItem; GNOME shows it with the AppIndicator extension)
// ---------------------------------------------------------------------------

struct ZTray {
    tx: UnboundedSender<Command>,
    hint: String,
}

impl ZTray {
    fn item(label: String, cmd: Command) -> MenuItem<Self> {
        StandardItem {
            label,
            activate: Box::new(move |tray: &mut Self| {
                let _ = tray.tx.send(cmd.clone());
            }),
            ..Default::default()
        }
        .into()
    }
}

impl ksni::Tray for ZTray {
    fn id(&self) -> String {
        super::APP_ID.into()
    }

    fn title(&self) -> String {
        "Z-SHOT".into()
    }

    fn icon_name(&self) -> String {
        super::APP_ID.into()
    }

    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        [22u32, 32, 48]
            .into_iter()
            .map(|size| {
                let rgba = crate::icons::app_icon_rgba(size);
                // ARGB32 in network byte order.
                let data = rgba.chunks_exact(4).flat_map(|p| [p[3], p[0], p[1], p[2]]).collect();
                ksni::Icon { width: size as i32, height: size as i32, data }
            })
            .collect()
    }

    fn tool_tip(&self) -> ksni::ToolTip {
        ksni::ToolTip {
            title: "Z-SHOT".into(),
            description: tr(format!("{} für einen Bereich", self.hint), format!("{} for a region", self.hint)),
            ..Default::default()
        }
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        let _ = self.tx.send(Command::Region);
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        vec![
            Self::item(tr(format!("Bereich aufnehmen ({})", self.hint), format!("Capture region ({})", self.hint)), Command::Region),
            Self::item(tr("Vollbild aufnehmen", "Capture fullscreen").into(), Command::Full),
            MenuItem::Separator,
            Self::item(tr("Oberfläche öffnen", "Open window").into(), Command::Show),
            Self::item(tr("Einstellungen…", "Settings…").into(), Command::Settings),
            MenuItem::Separator,
            Self::item(tr("Beenden", "Quit").into(), Command::Quit),
        ]
    }
}
