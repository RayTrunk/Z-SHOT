// Z-SHOT — Copyright (C) 2026 R. Trunk
// SPDX-License-Identifier: GPL-3.0-or-later

//! Freedesktop integration: folders, desktop entry and icon, autostart,
//! notifications, opening folders.

use super::APP_ID;
use crate::i18n::tr;
use std::path::{Path, PathBuf};

/// Running inside Flatpak: the manifest installs desktop entry and icon, and
/// autostart goes through the Background portal.
pub fn is_flatpak() -> bool {
    Path::new("/.flatpak-info").exists()
}

fn home() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."))
}

fn xdg_dir(var: &str, fallback: &str) -> PathBuf {
    match std::env::var_os(var) {
        Some(v) if Path::new(&v).is_absolute() => PathBuf::from(v),
        _ => home().join(fallback),
    }
}

/// `~/.local/share/z-shot`: album, settings, log.
pub fn data_dir() -> PathBuf {
    xdg_dir("XDG_DATA_HOME", ".local/share").join("z-shot")
}

fn config_home() -> PathBuf {
    xdg_dir("XDG_CONFIG_HOME", ".config")
}

/// The user's pictures folder from `user-dirs.dirs` (localised, e.g. `~/Bilder`).
pub fn pictures_dir() -> PathBuf {
    std::fs::read_to_string(config_home().join("user-dirs.dirs"))
        .ok()
        .and_then(|text| parse_user_dir(&text, "XDG_PICTURES_DIR", &home()))
        .unwrap_or_else(|| home().join("Pictures"))
}

/// Reads `KEY="$HOME/dir"` from a `user-dirs.dirs` file.
fn parse_user_dir(text: &str, key: &str, home: &Path) -> Option<PathBuf> {
    let line = text.lines().map(str::trim).find(|l| l.starts_with(key) && l[key.len()..].starts_with('='))?;
    let value = line[key.len() + 1..].trim().trim_matches('"');
    let path = if let Some(rest) = value.strip_prefix("$HOME") {
        home.join(rest.trim_start_matches('/'))
    } else {
        PathBuf::from(value)
    };
    // "$HOME/" alone means "not set" (the folder would be the home directory).
    (path.is_absolute() && path != home).then_some(path)
}

/// The command that starts this Z-SHOT: the AppImage itself when run from one
/// (its binary sits in a temporary mount), else the executable.
fn exe_command() -> String {
    let exe = std::env::var_os("APPIMAGE")
        .map(PathBuf::from)
        .or_else(|| std::env::current_exe().ok())
        .unwrap_or_else(|| PathBuf::from("z-shot"));
    quote_exec(&exe.display().to_string())
}

/// Quotes one argument of a desktop entry `Exec` line (desktop entry spec,
/// "The Exec key"): reserved characters need double quotes, and inside them `"`,
/// `` ` ``, `$` and `\` a backslash, which the string escaping then doubles.
fn quote_exec(arg: &str) -> String {
    let plain = arg.chars().all(|c| c.is_ascii_alphanumeric() || "/._-+".contains(c));
    if plain {
        return arg.to_string();
    }
    let mut out = String::from("\"");
    for c in arg.chars() {
        match c {
            '"' | '`' | '$' => {
                out.push_str("\\\\");
                out.push(c);
            }
            '\\' => out.push_str("\\\\\\\\"),
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}

fn desktop_entry(exe: &str) -> String {
    format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=Z-SHOT\n\
         GenericName=Screenshot Tool\n\
         GenericName[de]=Screenshot-Werkzeug\n\
         Comment=Capture a screen region, mark it up and keep it in an album\n\
         Comment[de]=Bildschirmbereich aufnehmen, markieren und im Album sammeln\n\
         Exec={exe}\n\
         Icon={APP_ID}\n\
         Terminal=false\n\
         Categories=Utility;Graphics;\n\
         Keywords=screenshot;capture;snip;Bildschirmfoto;\n\
         StartupWMClass={APP_ID}\n\
         Actions=region;full;\n\
         \n\
         [Desktop Action region]\n\
         Name=Capture region\n\
         Name[de]=Bereich aufnehmen\n\
         Exec={exe} --capture region\n\
         \n\
         [Desktop Action full]\n\
         Name=Capture fullscreen\n\
         Name[de]=Vollbild aufnehmen\n\
         Exec={exe} --capture full\n"
    )
}

/// Writes (or updates) the desktop entry and the icon in `~/.local/share`, so the
/// app shows up in the launcher and the portals know it (they look the app id up
/// there). Inside Flatpak the package provides both.
pub fn install() {
    if is_flatpak() {
        return;
    }
    let share = xdg_dir("XDG_DATA_HOME", ".local/share");
    let entry_path = share.join("applications").join(format!("{APP_ID}.desktop"));
    write_if_changed(&entry_path, &desktop_entry(&exe_command()));

    let icon_path = share.join("icons/hicolor/256x256/apps").join(format!("{APP_ID}.png"));
    if !icon_path.exists() {
        let rgba = crate::icons::app_icon_rgba(256);
        if let Some(img) = image::RgbaImage::from_raw(256, 256, rgba) {
            if let Some(dir) = icon_path.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            if let Err(e) = img.save(&icon_path) {
                crate::log::log(format!("Symbol nicht gespeichert: {e}"));
            }
        }
    }
}

fn write_if_changed(path: &Path, content: &str) {
    if std::fs::read_to_string(path).ok().as_deref() == Some(content) {
        return;
    }
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Err(e) = std::fs::write(path, content) {
        crate::log::log(format!("{} nicht geschrieben: {e}", path.display()));
    }
}

// ---------------------------------------------------------------------------
// Autostart
// ---------------------------------------------------------------------------

fn autostart_path() -> PathBuf {
    config_home().join("autostart").join(format!("{APP_ID}.desktop"))
}

/// Inside Flatpak the portal's answer cannot be read back; this file remembers it.
fn flatpak_marker() -> PathBuf {
    data_dir().join("autostart-requested")
}

pub fn autostart_enabled() -> bool {
    if is_flatpak() {
        flatpak_marker().exists()
    } else {
        autostart_path().exists()
    }
}

pub fn set_autostart(enabled: bool) -> Result<(), String> {
    if is_flatpak() {
        return set_autostart_portal(enabled);
    }
    let path = autostart_path();
    if enabled {
        let entry = format!(
            "[Desktop Entry]\n\
             Type=Application\n\
             Name=Z-SHOT\n\
             Exec={} --background\n\
             Icon={APP_ID}\n\
             Terminal=false\n\
             NoDisplay=true\n\
             X-GNOME-Autostart-enabled=true\n",
            exe_command()
        );
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        std::fs::write(&path, entry).map_err(|e| e.to_string())
    } else {
        match std::fs::remove_file(&path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.to_string()),
            _ => Ok(()),
        }
    }
}

fn set_autostart_portal(enabled: bool) -> Result<(), String> {
    use ashpd::desktop::background::Background;
    let answer = super::block_on(async {
        Background::request()
            .auto_start(enabled)
            .command(["z-shot", "--background"])
            .reason(tr("Screenshots per Tastenkürzel", "Screenshots by hotkey"))
            .send()
            .await?
            .response()
    })
    .map_err(|e| e.to_string())?;
    if enabled && !answer.auto_start() {
        return Err(tr("vom System abgelehnt", "refused by the system").into());
    }
    if enabled {
        let _ = std::fs::create_dir_all(data_dir());
        std::fs::write(flatpak_marker(), b"").map_err(|e| e.to_string())
    } else {
        let _ = std::fs::remove_file(flatpak_marker());
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Notifications and folders
// ---------------------------------------------------------------------------

/// Desktop notification (org.freedesktop.Notifications). Failures are only logged:
/// a notification is never essential. Inside the daemon's runtime it is sent on a
/// task (a second runtime cannot be started there).
pub fn notify(title: &str, text: &str) {
    if tokio::runtime::Handle::try_current().is_ok() {
        let (title, text) = (title.to_owned(), text.to_owned());
        tokio::spawn(async move { notify_async(&title, &text).await });
    } else {
        super::block_on(notify_async(title, text));
    }
}

pub async fn notify_async(title: &str, text: &str) {
    use ashpd::zbus::zvariant::Value;
    use std::collections::HashMap;
    let result = async {
        let conn = ashpd::zbus::Connection::session().await?;
        conn.call_method(
            Some("org.freedesktop.Notifications"),
            "/org/freedesktop/Notifications",
            Some("org.freedesktop.Notifications"),
            "Notify",
            &(
                "Z-SHOT",
                0u32,
                APP_ID,
                title,
                text,
                Vec::<&str>::new(),
                HashMap::<&str, Value>::new(),
                -1i32,
            ),
        )
        .await
        .map(|_| ())
    }
    .await;
    if let Err(e) = result {
        crate::log::log(format!("Benachrichtigung nicht möglich: {e}"));
    }
}

/// Opens a folder in the file manager.
pub fn open_folder(dir: &Path) {
    if let Err(e) = std::process::Command::new("xdg-open").arg(dir).spawn() {
        crate::log::log(format!("xdg-open: {e}"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_dirs_file_is_read() {
        let home = Path::new("/home/anna");
        let text = "# comment\nXDG_DESKTOP_DIR=\"$HOME/Schreibtisch\"\nXDG_PICTURES_DIR=\"$HOME/Bilder\"\n";
        assert_eq!(parse_user_dir(text, "XDG_PICTURES_DIR", home), Some(PathBuf::from("/home/anna/Bilder")));
        assert_eq!(parse_user_dir("XDG_PICTURES_DIR=\"/data/fotos\"", "XDG_PICTURES_DIR", home), Some(PathBuf::from("/data/fotos")));
        assert_eq!(parse_user_dir("XDG_PICTURES_DIR=\"$HOME/\"", "XDG_PICTURES_DIR", home), None);
        assert_eq!(parse_user_dir(text, "XDG_MUSIC_DIR", home), None);
    }

    #[test]
    fn exec_arguments_are_quoted_per_spec() {
        assert_eq!(quote_exec("/usr/bin/z-shot"), "/usr/bin/z-shot");
        assert_eq!(quote_exec("/home/a b/z-shot"), "\"/home/a b/z-shot\"");
        assert_eq!(quote_exec("/opt/$x"), "\"/opt/\\\\$x\"");
    }
}
