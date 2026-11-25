// Z-SHOT — Copyright (C) 2026 R. Trunk
// SPDX-License-Identifier: GPL-3.0-or-later

//! UI language: German when the Windows display language is German, English
//! otherwise. Decided once at start; `Z_SHOT_LANG=de|en` overrides it (for tests).
//!
//! Texts are written in place as pairs: `tr("Löschen", "Delete")`, or
//! `tr(format!(..), format!(..))` for texts with values.

use std::sync::OnceLock;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang {
    De,
    En,
}

static LANG: OnceLock<Lang> = OnceLock::new();

pub fn lang() -> Lang {
    *LANG.get_or_init(detect)
}

pub fn is_de() -> bool {
    lang() == Lang::De
}

/// The text for the UI language.
pub fn tr<T>(de: T, en: T) -> T {
    if is_de() {
        de
    } else {
        en
    }
}

fn detect() -> Lang {
    if let Ok(forced) = std::env::var("Z_SHOT_LANG") {
        if let Some(lang) = parse(&forced) {
            return lang;
        }
    }
    system_lang()
}

fn parse(code: &str) -> Option<Lang> {
    match code.trim().to_ascii_lowercase().get(..2)? {
        "de" => Some(Lang::De),
        "en" => Some(Lang::En),
        _ => None,
    }
}

#[cfg(windows)]
fn system_lang() -> Lang {
    use windows_sys::Win32::Globalization::GetUserDefaultUILanguage;
    // The low 10 bits are the primary language; 0x07 = German (any region).
    let id = unsafe { GetUserDefaultUILanguage() };
    from_langid(id)
}

/// POSIX: the first set of `LC_ALL`, `LC_MESSAGES`, `LANG` decides (in this order,
/// as for message catalogues).
#[cfg(not(windows))]
fn system_lang() -> Lang {
    ["LC_ALL", "LC_MESSAGES", "LANG"]
        .into_iter()
        .filter_map(|var| std::env::var(var).ok())
        .find(|v| !v.is_empty())
        .and_then(|v| parse(&v))
        .unwrap_or(Lang::En)
}

#[cfg_attr(not(windows), allow(dead_code))]
fn from_langid(id: u16) -> Lang {
    if id & 0x3ff == 0x07 {
        Lang::De
    } else {
        Lang::En
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn german_regions_and_others() {
        assert_eq!(from_langid(0x0407), Lang::De); // de-DE
        assert_eq!(from_langid(0x0C07), Lang::De); // de-AT
        assert_eq!(from_langid(0x0807), Lang::De); // de-CH
        assert_eq!(from_langid(0x0409), Lang::En); // en-US
        assert_eq!(from_langid(0x040C), Lang::En); // fr-FR -> English
    }

    #[test]
    fn override_codes() {
        assert_eq!(parse("de"), Some(Lang::De));
        assert_eq!(parse("EN-us"), Some(Lang::En));
        assert_eq!(parse("fr"), None);
        assert_eq!(parse(""), None);
    }
}
