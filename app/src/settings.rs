// Z-SHOT — Copyright (C) 2026 R. Trunk
// SPDX-License-Identifier: GPL-3.0-or-later

//! User settings: global hotkeys, what happens after a capture, save folder, autostart.
//! Persisted as `settings.json` next to the album in `%LOCALAPPDATA%\Z-SHOT`.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Virtual-key codes used for the defaults (Win32 VK_*).
pub const VK_SNAPSHOT: u32 = 0x2C;

/// A global key combination: modifiers plus one Win32 virtual-key code.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hotkey {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub win: bool,
    pub vk: u32,
}

impl Hotkey {
    pub const fn key(vk: u32) -> Self {
        Self { ctrl: false, shift: false, alt: false, win: false, vk }
    }

    /// Human readable German label, e.g. `Strg+Umschalt+S` or `Druck`.
    pub fn label(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        if self.ctrl {
            parts.push("Strg".into());
        }
        if self.shift {
            parts.push("Umschalt".into());
        }
        if self.alt {
            parts.push("Alt".into());
        }
        if self.win {
            parts.push("Win".into());
        }
        parts.push(vk_name(self.vk));
        parts.join("+")
    }
}

/// German names for Win32 virtual-key codes.
pub fn vk_name(vk: u32) -> String {
    match vk {
        0x08 => "Rücktaste".into(),
        0x09 => "Tab".into(),
        0x0D => "Eingabe".into(),
        0x13 => "Pause".into(),
        0x14 => "Feststell".into(),
        0x1B => "Esc".into(),
        0x20 => "Leertaste".into(),
        0x21 => "Bild↑".into(),
        0x22 => "Bild↓".into(),
        0x23 => "Ende".into(),
        0x24 => "Pos1".into(),
        0x25 => "Links".into(),
        0x26 => "Oben".into(),
        0x27 => "Rechts".into(),
        0x28 => "Unten".into(),
        0x2C => "Druck".into(),
        0x2D => "Einfg".into(),
        0x2E => "Entf".into(),
        0x30..=0x39 | 0x41..=0x5A => char::from_u32(vk).map(String::from).unwrap_or_default(),
        0x60..=0x69 => format!("Num {}", vk - 0x60),
        0x6A => "Num *".into(),
        0x6B => "Num +".into(),
        0x6D => "Num -".into(),
        0x6E => "Num ,".into(),
        0x6F => "Num /".into(),
        0x70..=0x87 => format!("F{}", vk - 0x6F),
        0x91 => "Rollen".into(),
        0xBA => "Ü".into(),
        0xBB => "+".into(),
        0xBC => ",".into(),
        0xBD => "-".into(),
        0xBE => ".".into(),
        0xBF => "#".into(),
        0xC0 => "Ö".into(),
        0xDB => "ß".into(),
        0xDC => "^".into(),
        0xDD => "´".into(),
        0xDE => "Ä".into(),
        0xE2 => "<".into(),
        other => format!("Taste 0x{:02X}", other),
    }
}

/// Actions that can be bound to a global hotkey. The discriminant doubles as the
/// Win32 hotkey id, so keep it stable.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HotkeyAction {
    Region = 1,
    RegionAlt = 2,
    Full = 3,
}

impl HotkeyAction {
    pub const ALL: [HotkeyAction; 3] =
        [HotkeyAction::Region, HotkeyAction::RegionAlt, HotkeyAction::Full];

    pub fn id(self) -> i32 {
        self as i32
    }

    pub fn from_id(id: i32) -> Option<Self> {
        Self::ALL.into_iter().find(|a| a.id() == id)
    }

    pub fn title(self) -> &'static str {
        match self {
            HotkeyAction::Region => "Bereich aufnehmen",
            HotkeyAction::RegionAlt => "Bereich aufnehmen (alternativ)",
            HotkeyAction::Full => "Vollbild aufnehmen",
        }
    }
}

/// What happens after a region (or full-screen) capture.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AfterCapture {
    OpenEditor,
    SaveDirect,
    SaveAndOpenEditor,
}

impl AfterCapture {
    pub fn saves(self) -> bool {
        matches!(self, AfterCapture::SaveDirect | AfterCapture::SaveAndOpenEditor)
    }
    pub fn opens_editor(self) -> bool {
        matches!(self, AfterCapture::OpenEditor | AfterCapture::SaveAndOpenEditor)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub hotkey_region: Option<Hotkey>,
    pub hotkey_region_alt: Option<Hotkey>,
    pub hotkey_full: Option<Hotkey>,
    pub after_capture: AfterCapture,
    pub copy_to_clipboard: bool,
    /// `None` means the default `Bilder\Z-SHOT` folder.
    pub save_dir: Option<PathBuf>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            hotkey_region: Some(Hotkey::key(VK_SNAPSHOT)),
            hotkey_region_alt: Some(Hotkey { ctrl: true, shift: true, ..Hotkey::key(b'S' as u32) }),
            hotkey_full: None,
            after_capture: AfterCapture::OpenEditor,
            copy_to_clipboard: false,
            save_dir: None,
        }
    }
}

impl Settings {
    pub fn hotkey(&self, action: HotkeyAction) -> Option<Hotkey> {
        match action {
            HotkeyAction::Region => self.hotkey_region,
            HotkeyAction::RegionAlt => self.hotkey_region_alt,
            HotkeyAction::Full => self.hotkey_full,
        }
    }

    /// Assigns `hk` to `action`. A combination can only belong to one action, so any
    /// other action already using it is cleared.
    pub fn set_hotkey(&mut self, action: HotkeyAction, hk: Option<Hotkey>) {
        if hk.is_some() {
            for other in HotkeyAction::ALL {
                if other != action && self.hotkey(other) == hk {
                    *self.slot(other) = None;
                }
            }
        }
        *self.slot(action) = hk;
    }

    fn slot(&mut self, action: HotkeyAction) -> &mut Option<Hotkey> {
        match action {
            HotkeyAction::Region => &mut self.hotkey_region,
            HotkeyAction::RegionAlt => &mut self.hotkey_region_alt,
            HotkeyAction::Full => &mut self.hotkey_full,
        }
    }

    pub fn hotkey_bindings(&self) -> Vec<(HotkeyAction, Hotkey)> {
        HotkeyAction::ALL
            .into_iter()
            .filter_map(|a| self.hotkey(a).map(|hk| (a, hk)))
            .collect()
    }

    /// Short text for hints, e.g. `Druck / Strg+Umschalt+S`.
    pub fn region_hotkeys_label(&self) -> String {
        let labels: Vec<String> = [self.hotkey_region, self.hotkey_region_alt]
            .into_iter()
            .flatten()
            .map(|h| h.label())
            .collect();
        if labels.is_empty() {
            "kein Tastenkürzel".into()
        } else {
            labels.join(" / ")
        }
    }

    pub fn effective_save_dir(&self) -> PathBuf {
        self.save_dir.clone().unwrap_or_else(default_save_dir)
    }

    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(path, json).map_err(|e| e.to_string())
    }
}

/// `Bilder\Z-SHOT` in the user's (possibly redirected / OneDrive) Pictures folder.
pub fn default_save_dir() -> PathBuf {
    pictures_dir().join("Z-SHOT")
}

#[cfg(windows)]
fn pictures_dir() -> PathBuf {
    use windows_sys::Win32::System::Com::CoTaskMemFree;
    use windows_sys::Win32::UI::Shell::{FOLDERID_Pictures, SHGetKnownFolderPath, KF_FLAG_DEFAULT};
    unsafe {
        let mut raw: windows_sys::core::PWSTR = std::ptr::null_mut();
        let hr = SHGetKnownFolderPath(&FOLDERID_Pictures, KF_FLAG_DEFAULT as _, std::ptr::null_mut(), &mut raw);
        let result = if hr >= 0 && !raw.is_null() {
            let len = (0..).take_while(|&i| *raw.add(i) != 0).count();
            let slice = std::slice::from_raw_parts(raw, len);
            Some(PathBuf::from(String::from_utf16_lossy(slice)))
        } else {
            None
        };
        if !raw.is_null() {
            CoTaskMemFree(raw as *const _);
        }
        result.unwrap_or_else(fallback_pictures_dir)
    }
}

#[cfg(not(windows))]
fn pictures_dir() -> PathBuf {
    fallback_pictures_dir()
}

fn fallback_pictures_dir() -> PathBuf {
    std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .map(|home| PathBuf::from(home).join("Pictures"))
        .unwrap_or_else(|_| PathBuf::from("Pictures"))
}

/// File name for a capture taken at `at`: `2026-11-21_14-05-33.png`.
pub fn capture_file_name(at: chrono::DateTime<chrono::Local>) -> String {
    format!("{}.png", at.format("%Y-%m-%d_%H-%M-%S"))
}

/// Path in `dir` for a capture taken at `at` that does not overwrite an existing file
/// (appends `_2`, `_3`, … when several captures fall into the same second).
pub fn unique_capture_path(dir: &Path, at: chrono::DateTime<chrono::Local>) -> PathBuf {
    let first = dir.join(capture_file_name(at));
    if !first.exists() {
        return first;
    }
    let stem = at.format("%Y-%m-%d_%H-%M-%S").to_string();
    (2..)
        .map(|n| dir.join(format!("{stem}_{n}.png")))
        .find(|p| !p.exists())
        .expect("unbounded range always yields a free name")
}

// ---------------------------------------------------------------------------
// Autostart (HKCU\Software\Microsoft\Windows\CurrentVersion\Run)
// ---------------------------------------------------------------------------

#[cfg(windows)]
const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
#[cfg(windows)]
const RUN_VALUE: &str = "Z-SHOT";

#[cfg(windows)]
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(windows)]
pub fn autostart_enabled() -> bool {
    use windows_sys::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_SZ};
    let key = wide(RUN_KEY);
    let val = wide(RUN_VALUE);
    unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            val.as_ptr(),
            RRF_RT_REG_SZ,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        ) == 0
    }
}

#[cfg(windows)]
pub fn set_autostart(enabled: bool) -> Result<(), String> {
    use windows_sys::Win32::System::Registry::{
        RegDeleteKeyValueW, RegSetKeyValueW, HKEY_CURRENT_USER, REG_SZ,
    };
    let key = wide(RUN_KEY);
    let val = wide(RUN_VALUE);
    let status = unsafe {
        if enabled {
            let exe = std::env::current_exe().map_err(|e| e.to_string())?;
            let cmd = wide(&format!("\"{}\"", exe.display()));
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                key.as_ptr(),
                val.as_ptr(),
                REG_SZ,
                cmd.as_ptr() as *const _,
                (cmd.len() * 2) as u32,
            )
        } else {
            let s = RegDeleteKeyValueW(HKEY_CURRENT_USER, key.as_ptr(), val.as_ptr());
            // ERROR_FILE_NOT_FOUND: already disabled.
            if s == 2 { 0 } else { s }
        }
    };
    if status == 0 {
        Ok(())
    } else {
        Err(format!("Registry-Fehler {status}"))
    }
}

#[cfg(not(windows))]
pub fn autostart_enabled() -> bool {
    false
}

#[cfg(not(windows))]
pub fn set_autostart(_enabled: bool) -> Result<(), String> {
    Err("Autostart wird nur unter Windows unterstützt".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn at(h: u32, m: u32, s: u32) -> chrono::DateTime<chrono::Local> {
        chrono::Local.with_ymd_and_hms(2026, 11, 21, h, m, s).unwrap()
    }

    #[test]
    fn file_name_is_date_then_time() {
        assert_eq!(capture_file_name(at(14, 5, 3)), "2026-11-21_14-05-03.png");
    }

    #[test]
    fn unique_path_appends_counter_on_collision() {
        let dir = std::env::temp_dir().join(format!("zshot-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let t = at(9, 0, 0);
        let p1 = unique_capture_path(&dir, t);
        assert_eq!(p1.file_name().unwrap(), "2026-11-21_09-00-00.png");
        std::fs::write(&p1, b"x").unwrap();
        let p2 = unique_capture_path(&dir, t);
        assert_eq!(p2.file_name().unwrap(), "2026-11-21_09-00-00_2.png");
        std::fs::write(&p2, b"x").unwrap();
        assert_eq!(unique_capture_path(&dir, t).file_name().unwrap(), "2026-11-21_09-00-00_3.png");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn hotkey_labels_are_german() {
        assert_eq!(Hotkey::key(VK_SNAPSHOT).label(), "Druck");
        let hk = Hotkey { ctrl: true, shift: true, ..Hotkey::key(b'S' as u32) };
        assert_eq!(hk.label(), "Strg+Umschalt+S");
        assert_eq!(Hotkey { alt: true, ..Hotkey::key(0x70) }.label(), "Alt+F1");
    }

    #[test]
    fn defaults_bind_print_screen_to_region() {
        let s = Settings::default();
        assert_eq!(s.hotkey(HotkeyAction::Region), Some(Hotkey::key(VK_SNAPSHOT)));
        assert_eq!(s.after_capture, AfterCapture::OpenEditor);
        assert!(s.effective_save_dir().ends_with("Z-SHOT"));
    }

    #[test]
    fn assigning_a_used_hotkey_moves_it() {
        let mut s = Settings::default();
        let druck = Hotkey::key(VK_SNAPSHOT);
        s.set_hotkey(HotkeyAction::Full, Some(druck));
        assert_eq!(s.hotkey(HotkeyAction::Full), Some(druck));
        assert_eq!(s.hotkey(HotkeyAction::Region), None);
    }

    #[test]
    fn settings_roundtrip_and_missing_fields_use_defaults() {
        let mut s = Settings::default();
        s.after_capture = AfterCapture::SaveDirect;
        s.copy_to_clipboard = true;
        let json = serde_json::to_string(&s).unwrap();
        assert_eq!(serde_json::from_str::<Settings>(&json).unwrap(), s);

        let partial: Settings = serde_json::from_str(r#"{"after_capture":"SaveDirect"}"#).unwrap();
        assert_eq!(partial.after_capture, AfterCapture::SaveDirect);
        assert_eq!(partial.hotkey_region, Settings::default().hotkey_region);
    }

    #[test]
    fn action_ids_roundtrip() {
        for a in HotkeyAction::ALL {
            assert_eq!(HotkeyAction::from_id(a.id()), Some(a));
        }
        assert_eq!(HotkeyAction::from_id(99), None);
    }
}
