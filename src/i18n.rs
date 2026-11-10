//! The language of the interface: Indonesian (the default) or English.
//!
//! Texts are written in both languages where they are used,
//! `t("Pasien", "Patients")`, so each pair is reviewed together.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU8, Ordering};

use crate::recent::app_dir;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Language {
    #[default]
    Indonesian,
    English,
}

impl Language {
    pub const ALL: [Language; 2] = [Language::Indonesian, Language::English];

    pub fn code(self) -> &'static str {
        match self {
            Language::Indonesian => "id",
            Language::English => "en",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        match code.trim() {
            "id" => Some(Language::Indonesian),
            "en" => Some(Language::English),
            _ => None,
        }
    }

    /// The language's own name.
    pub fn name(self) -> &'static str {
        match self {
            Language::Indonesian => "Bahasa Indonesia",
            Language::English => "English",
        }
    }
}

static CURRENT: AtomicU8 = AtomicU8::new(0);

pub fn language() -> Language {
    match CURRENT.load(Ordering::Relaxed) {
        1 => Language::English,
        _ => Language::Indonesian,
    }
}

pub fn set_language(language: Language) {
    CURRENT.store(language as u8, Ordering::Relaxed);
}

/// The text in the current language.
pub fn t(indonesian: &'static str, english: &'static str) -> &'static str {
    pick(indonesian, english)
}

/// The value for the current language, for texts built with `format!`.
pub fn pick<T>(indonesian: T, english: T) -> T {
    match language() {
        Language::Indonesian => indonesian,
        Language::English => english,
    }
}

fn preference_file() -> PathBuf {
    app_dir().join("language.txt")
}

/// The language chosen earlier, or Indonesian.
pub fn load_preference() -> Language {
    std::fs::read_to_string(preference_file())
        .ok()
        .and_then(|code| Language::from_code(&code))
        .unwrap_or_default()
}

pub fn save_preference(language: Language) -> std::io::Result<()> {
    let file = preference_file();
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(file, language.code())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_round_trip() {
        for language in Language::ALL {
            assert_eq!(Language::from_code(language.code()), Some(language));
        }
        assert_eq!(Language::from_code("fr"), None);
        assert_eq!(Language::default(), Language::Indonesian);
    }
}
