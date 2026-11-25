// Z-SHOT — Copyright (C) 2026 R. Trunk
// SPDX-License-Identifier: GPL-3.0-or-later

//! Desktop portals (xdg-desktop-portal): screenshots and global shortcuts.
//!
//! Both ask the user the first time (the desktop shows its own dialog) and the
//! portal remembers the answer for [`super::APP_ID`].

use super::ipc::Command;
use crate::i18n::tr;
use crate::settings::{Hotkey, HotkeyAction};
use ashpd::desktop::global_shortcuts::{GlobalShortcuts, NewShortcut};
use ashpd::desktop::screenshot::Screenshot;
use futures_util::StreamExt;
use image::RgbaImage;
use std::sync::Mutex;

/// Tells the portals which application this process is (needs the desktop entry
/// installed, see `desktop::install`). Inside Flatpak this is a no-op.
pub async fn register_app() {
    let id = match ashpd::AppID::try_from(super::APP_ID) {
        Ok(id) => id,
        Err(e) => {
            crate::log::log(format!("Ungültige App-ID: {e}"));
            return;
        }
    };
    if let Err(e) = ashpd::register_host_app(id).await {
        // Older portals (before 1.19) have no registry; they still work, only
        // permissions may not be remembered per app.
        crate::log::log(format!("Portal-Registrierung nicht möglich: {e}"));
    }
}

// ---------------------------------------------------------------------------
// Screenshot
// ---------------------------------------------------------------------------

/// Screenshot taken before the window opened (see [`precapture`]).
static PRECAPTURED: Mutex<Option<Result<RgbaImage, String>>> = Mutex::new(None);

/// Takes the screenshot now, while this process has no window yet, and keeps it for
/// the next [`capture_screen`] call.
pub fn precapture() {
    let shot = capture_now();
    if let Err(e) = &shot {
        crate::log::log(format!("Screenshot-Portal: {e}"));
    }
    if let Ok(mut slot) = PRECAPTURED.lock() {
        *slot = Some(shot);
    }
}

/// The whole desktop (all monitors), as the portal delivers it.
pub fn capture_screen() -> Result<RgbaImage, String> {
    if let Some(shot) = PRECAPTURED.lock().ok().and_then(|mut s| s.take()) {
        return shot;
    }
    capture_now()
}

fn capture_now() -> Result<RgbaImage, String> {
    let uri = super::block_on(async {
        register_app().await;
        Screenshot::request()
            .interactive(false)
            .modal(false)
            .send()
            .await?
            .response()
            .map(|s| s.uri().clone())
    })
    .map_err(|e| match e {
        ashpd::Error::Response(ashpd::desktop::ResponseError::Cancelled) => {
            tr("Bildschirmaufnahme abgelehnt", "Screen capture was denied").to_string()
        }
        other => tr(format!("Bildschirmaufnahme nicht möglich: {other}"), format!("Screen capture not possible: {other}")),
    })?;
    let path = uri
        .to_file_path()
        .map_err(|_| tr(format!("Unerwartete Adresse vom Portal: {uri}"), format!("Unexpected address from the portal: {uri}")))?;
    let img = image::open(&path).map(|i| i.to_rgba8()).map_err(|e| e.to_string());
    // The portal wrote the file only for us (GNOME into Pictures/Screenshots);
    // the capture goes to the album and, if chosen, to the capture folder.
    let _ = std::fs::remove_file(&path);
    img
}

// ---------------------------------------------------------------------------
// Global shortcuts
// ---------------------------------------------------------------------------

fn shortcut_id(action: HotkeyAction) -> &'static str {
    match action {
        HotkeyAction::Region => "region",
        HotkeyAction::RegionAlt => "region-alt",
        HotkeyAction::Full => "full",
    }
}

/// Registers the shortcuts and sends a command for every activation, until `stop`
/// fires (then the portal session is closed). Returns early when the portal is
/// missing or refused (then the user binds `z-shot --capture …` in the desktop
/// settings instead).
pub async fn run_shortcuts(
    bindings: Vec<(HotkeyAction, Hotkey)>,
    tx: tokio::sync::mpsc::UnboundedSender<Command>,
    stop: tokio::sync::oneshot::Receiver<()>,
) {
    if let Err(e) = shortcuts_loop(&bindings, &tx, stop).await {
        crate::log::log(format!("Globale Tastenkürzel (Portal) nicht verfügbar: {e}"));
    }
}

async fn shortcuts_loop(
    bindings: &[(HotkeyAction, Hotkey)],
    tx: &tokio::sync::mpsc::UnboundedSender<Command>,
    mut stop: tokio::sync::oneshot::Receiver<()>,
) -> Result<(), ashpd::Error> {
    use futures_util::future::{select, Either};
    let portal = GlobalShortcuts::new().await?;
    let session = portal.create_session().await?;
    // Listen before binding, so no activation right after the dialog gets lost.
    let mut activated = portal.receive_activated().await?;

    let shortcuts: Vec<NewShortcut> = HotkeyAction::ALL
        .into_iter()
        .map(|action| {
            let preferred = bindings.iter().find(|(a, _)| *a == action).and_then(|(_, hk)| xdg_trigger(hk));
            NewShortcut::new(shortcut_id(action), action.title()).preferred_trigger(preferred.as_deref())
        })
        .collect();
    let bound = portal.bind_shortcuts(&session, &shortcuts, None).await?.response()?;
    for s in bound.shortcuts() {
        crate::log::log(format!("Tastenkürzel {}: {}", s.id(), s.trigger_description()));
    }

    loop {
        let event = match select(activated.next(), &mut stop).await {
            Either::Left((Some(event), _)) => event,
            // Stream ended, or asked to stop (re-registration, quit).
            Either::Left((None, _)) | Either::Right(_) => break,
        };
        let cmd = match event.shortcut_id() {
            "region" | "region-alt" => Command::Region,
            "full" => Command::Full,
            _ => continue,
        };
        if tx.send(cmd).is_err() {
            break;
        }
    }
    // Otherwise the session lives on with the D-Bus connection, and a new one
    // would deliver every key press twice.
    let _ = session.close().await;
    Ok(())
}

/// The combination in the notation of the XDG shortcuts specification
/// (`CTRL+SHIFT+s`, `Print`), as a suggestion for the desktop's dialog.
pub fn xdg_trigger(hk: &Hotkey) -> Option<String> {
    let key = match hk.vk {
        0x30..=0x39 => char::from_u32(hk.vk)?.to_string(),
        0x41..=0x5A => char::from_u32(hk.vk)?.to_ascii_lowercase().to_string(),
        0x70..=0x87 => format!("F{}", hk.vk - 0x6F),
        0x2C => "Print".into(),
        0x13 => "Pause".into(),
        0x20 => "space".into(),
        0x09 => "Tab".into(),
        0x0D => "Return".into(),
        0x2D => "Insert".into(),
        0x2E => "Delete".into(),
        0x24 => "Home".into(),
        0x23 => "End".into(),
        0x21 => "Page_Up".into(),
        0x22 => "Page_Down".into(),
        0x25 => "Left".into(),
        0x26 => "Up".into(),
        0x27 => "Right".into(),
        0x28 => "Down".into(),
        _ => return None,
    };
    let mut parts = Vec::new();
    if hk.ctrl {
        parts.push("CTRL".to_string());
    }
    if hk.alt {
        parts.push("ALT".to_string());
    }
    if hk.shift {
        parts.push("SHIFT".to_string());
    }
    if hk.win {
        parts.push("LOGO".to_string());
    }
    parts.push(key);
    Some(parts.join("+"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::VK_SNAPSHOT;

    #[test]
    fn triggers_follow_the_xdg_notation() {
        assert_eq!(xdg_trigger(&Hotkey::key(VK_SNAPSHOT)).as_deref(), Some("Print"));
        let hk = Hotkey { ctrl: true, shift: true, ..Hotkey::key(b'S' as u32) };
        assert_eq!(xdg_trigger(&hk).as_deref(), Some("CTRL+SHIFT+s"));
        let hk = Hotkey { win: true, alt: true, ..Hotkey::key(0x70) };
        assert_eq!(xdg_trigger(&hk).as_deref(), Some("ALT+LOGO+F1"));
        assert_eq!(xdg_trigger(&Hotkey::key(0xDE)), None);
    }
}
