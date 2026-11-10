//! Kutilang v2: medical records for a clinic, with a GPUI desktop interface
//! over the same SQLite database as the original Go application.
//!
//! This file holds the basics: the data model, dates, the interface
//! language, search keywords and the list of recent databases. The rest is in
//! `store` (SQLite, migration, backup, Excel export), `diagnosis` (the text of
//! a visit: template, conversions, printing) and `ui` (the window).

pub mod diagnosis;
pub mod store;
pub mod ui;

use anyhow::Result;
use chrono::{DateTime, Datelike, FixedOffset, Local, NaiveDate};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU8, Ordering};

// ============================================================================
// Domain types and input validation.
// ============================================================================

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Patient {
    pub id: String,
    pub name: String,
    pub head_family_name: String,
    /// `YYYY-MM-DD`
    pub birth_date: String,
    pub address: String,
    /// `YYYY-MM-DD`, the most recent visit (or the registration date).
    pub last_visit: String,
}

/// How a visit's diagnosis text is stored.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiagnosisFormat {
    /// Written with the rich text editor of the original app.
    Html,
    /// Written with this app.
    Markdown,
}

impl DiagnosisFormat {
    pub fn as_str(self) -> &'static str {
        match self {
            DiagnosisFormat::Html => "html",
            DiagnosisFormat::Markdown => "markdown",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "html" => Some(DiagnosisFormat::Html),
            "markdown" => Some(DiagnosisFormat::Markdown),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Visit {
    pub id: String,
    pub patient_id: String,
    /// `YYYY-MM-DD`
    pub date: String,
    pub diagnosis: String,
    pub format: DiagnosisFormat,
}

/// A visit in the list of all visits, without its full text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VisitSummary {
    pub id: String,
    pub patient_id: String,
    pub patient_name: String,
    pub head_family_name: String,
    /// `YYYY-MM-DD`
    pub date: String,
    /// The first lines of the diagnosis, as plain text.
    pub summary: String,
}

/// How the list of patients is sorted.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PatientOrder {
    /// Most recently visited first.
    #[default]
    RecentVisit,
    /// Alphabetically by name.
    Name,
}

/// A page of results and the cursor of the next page, if there is one.
#[derive(Clone, Debug)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub next: Option<Cursor>,
}

/// Keyset pagination cursor: results continue strictly after
/// (`date`, `id`) in descending order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cursor {
    pub date: String,
    pub id: String,
}

/// Raw values of the patient profile form.
#[derive(Clone, Debug, Default)]
pub struct PatientInput {
    pub name: String,
    pub head_family_name: String,
    pub birth_date: String,
    pub address: String,
}

/// A validated patient profile, ready to be stored.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PatientProfile {
    pub name: String,
    pub head_family_name: String,
    pub birth_date: NaiveDate,
    pub address: String,
}

/// Per-field validation messages of the profile form.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PatientErrors {
    pub name: Option<String>,
    pub head_family_name: Option<String>,
    pub birth_date: Option<String>,
    pub address: Option<String>,
}

impl PatientErrors {
    pub fn is_empty(&self) -> bool {
        self.name.is_none()
            && self.head_family_name.is_none()
            && self.birth_date.is_none()
            && self.address.is_none()
    }
}

fn check_length(value: &str, max: usize) -> Option<String> {
    (value.chars().count() > max).then(|| {
        pick(
            format!("Maksimal {max} karakter"),
            format!("Maximum at {max} characters"),
        )
    })
}

impl PatientInput {
    /// Validates the form with the same rules as the original app.
    pub fn validate(&self, today: NaiveDate) -> Result<PatientProfile, PatientErrors> {
        let name = self.name.trim();
        let head_family_name = self.head_family_name.trim();
        let address = self.address.trim();

        let mut errors = PatientErrors {
            name: if name.is_empty() {
                Some(t("Wajib diisi", "Required").into())
            } else {
                check_length(name, 255)
            },
            head_family_name: check_length(head_family_name, 255),
            birth_date: None,
            address: if address.is_empty() {
                Some(t("Wajib diisi", "Required").into())
            } else {
                check_length(address, 4096)
            },
        };

        let birth_date = match parse_user_date(&self.birth_date) {
            Ok(date) if date > today => {
                errors.birth_date = Some(
                    t(
                        "Tanggal lahir ada di masa depan",
                        "Birthdate is in the future",
                    )
                    .into(),
                );
                None
            }
            Ok(date) => Some(date),
            Err(message) => {
                errors.birth_date = Some(message);
                None
            }
        };

        match birth_date {
            Some(birth_date) if errors.is_empty() => Ok(PatientProfile {
                name: name.to_string(),
                head_family_name: head_family_name.to_string(),
                birth_date,
                address: address.to_string(),
            }),
            _ => Err(errors),
        }
    }
}

#[cfg(test)]
mod model_tests {
    use super::*;

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, 9).unwrap()
    }

    #[test]
    fn valid_input_is_trimmed() {
        let input = PatientInput {
            name: "  Siti  Aminah ".into(),
            head_family_name: " Joko ".into(),
            birth_date: "1990-1-5".into(),
            address: " Jl. Mawar 1 ".into(),
        };
        let profile = input.validate(today()).unwrap();
        assert_eq!(profile.name, "Siti  Aminah");
        assert_eq!(profile.head_family_name, "Joko");
        assert_eq!(
            profile.birth_date,
            NaiveDate::from_ymd_opt(1990, 1, 5).unwrap()
        );
        assert_eq!(profile.address, "Jl. Mawar 1");
    }

    #[test]
    fn reports_every_invalid_field() {
        let input = PatientInput {
            name: " ".into(),
            head_family_name: "x".repeat(256),
            birth_date: "2030-01-01".into(),
            address: "".into(),
        };
        let errors = input.validate(today()).unwrap_err();
        assert_eq!(errors.name.as_deref(), Some("Wajib diisi"));
        assert!(errors.head_family_name.is_some());
        assert_eq!(
            errors.birth_date.as_deref(),
            Some("Tanggal lahir ada di masa depan")
        );
        assert_eq!(errors.address.as_deref(), Some("Wajib diisi"));
    }

    #[test]
    fn head_family_name_is_optional() {
        let input = PatientInput {
            name: "Budi".into(),
            head_family_name: "".into(),
            birth_date: "2000-02-29".into(),
            address: "Desa".into(),
        };
        assert!(input.validate(today()).is_ok());
    }
}

// ============================================================================
// Date parsing, and the date formatting used across the interface, in the
// interface's language.
//
// Dates are stored as `YYYY-MM-DD` text, exactly like the original app, so
// they sort correctly as strings.
// ============================================================================

const MONTHS_ID: [&str; 12] = [
    "Januari",
    "Februari",
    "Maret",
    "April",
    "Mei",
    "Juni",
    "Juli",
    "Agustus",
    "September",
    "Oktober",
    "November",
    "Desember",
];
const MONTHS_EN: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];
const WEEKDAYS_ID: [&str; 7] = [
    "Senin", "Selasa", "Rabu", "Kamis", "Jumat", "Sabtu", "Minggu",
];
const WEEKDAYS_EN: [&str; 7] = [
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
    "Sunday",
];

fn month_name(language: Language, date: NaiveDate) -> &'static str {
    match language {
        Language::Indonesian => MONTHS_ID[date.month0() as usize],
        Language::English => MONTHS_EN[date.month0() as usize],
    }
}

/// Today's date in the local time zone.
pub fn today() -> NaiveDate {
    Local::now().date_naive()
}

/// Formats a date the way it is stored in the database (`YYYY-MM-DD`).
pub fn to_stored(date: NaiveDate) -> String {
    date.format("%Y-%m-%d").to_string()
}

/// Parses a stored `YYYY-MM-DD` date.
pub fn parse_stored(value: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(value.trim(), "%Y-%m-%d").ok()
}

/// Parses a date typed by a user.
///
/// Accepts `YYYY-MM-DD` (leading zeros optional, as the original app did) and
/// the day-first `DD-MM-YYYY`. `/` and `.` work as separators too.
pub fn parse_user_date(input: &str) -> Result<NaiveDate, String> {
    let input = input.trim();
    if input.is_empty() {
        return Err(t("Wajib diisi", "Required").into());
    }
    let invalid_format = || {
        t(
            "Tanggal tidak valid, gunakan YYYY-MM-DD",
            "Invalid date, use YYYY-MM-DD",
        )
        .to_string()
    };

    let parts: Vec<&str> = input.split(['-', '/', '.']).map(str::trim).collect();
    let numbers: Option<Vec<u32>> = parts.iter().map(|p| p.parse::<u32>().ok()).collect();
    let (Some(numbers), 3) = (numbers, parts.len()) else {
        return Err(invalid_format());
    };

    let (year, month, day) = if parts[0].len() == 4 {
        (numbers[0], numbers[1], numbers[2])
    } else if parts[2].len() == 4 {
        (numbers[2], numbers[1], numbers[0])
    } else {
        return Err(invalid_format());
    };

    if !(1..=9999).contains(&year) {
        return Err(t("Tahun tidak valid", "Invalid year").into());
    }
    NaiveDate::from_ymd_opt(year as i32, month, day)
        .ok_or_else(|| t("Tanggal tidak valid", "Invalid date").to_string())
}

/// `7 November 2026`.
pub fn format_long(date: NaiveDate) -> String {
    format_long_in(language(), date)
}

pub fn format_long_in(language: Language, date: NaiveDate) -> String {
    match language {
        Language::Indonesian => format!(
            "{} {} {}",
            date.day(),
            month_name(language, date),
            date.year()
        ),
        Language::English => format!(
            "{} {} {}",
            date.day(),
            month_name(language, date),
            date.year()
        ),
    }
}

/// `Senin, 9 November 2026`.
pub fn format_with_weekday(date: NaiveDate) -> String {
    format_with_weekday_in(language(), date)
}

pub fn format_with_weekday_in(language: Language, date: NaiveDate) -> String {
    let index = date.weekday().num_days_from_monday() as usize;
    let weekday = match language {
        Language::Indonesian => WEEKDAYS_ID[index],
        Language::English => WEEKDAYS_EN[index],
    };
    format!("{weekday}, {}", format_long_in(language, date))
}

/// `9 Nov 2026`: three-letter months (Agu and Des in Indonesian).
pub fn format_short(date: NaiveDate) -> String {
    format_short_in(language(), date)
}

pub fn format_short_in(language: Language, date: NaiveDate) -> String {
    let month = month_name(language, date);
    let month = month.get(..3).unwrap_or(month);
    format!("{} {} {}", date.day(), month, date.year())
}

/// Formats a stored date for display, falling back to the raw text when it
/// is not a valid date.
pub fn display_date(value: &str) -> String {
    parse_stored(value).map_or_else(|| value.to_string(), format_long)
}

/// A day-granular relative description, e.g. `3 hari yang lalu`.
pub fn format_relative(date: NaiveDate, today: NaiveDate) -> String {
    format_relative_in(language(), date, today)
}

pub fn format_relative_in(language: Language, date: NaiveDate, today: NaiveDate) -> String {
    let id = language == Language::Indonesian;
    let days = (today - date).num_days();
    match days {
        0 => return if id { "hari ini" } else { "today" }.into(),
        1 => return if id { "kemarin" } else { "yesterday" }.into(),
        -1 => return if id { "besok" } else { "tomorrow" }.into(),
        _ => {}
    }
    let amount = describe_days(language, days.unsigned_abs());
    match (id, days > 0) {
        (true, true) => format!("{amount} yang lalu"),
        (true, false) => format!("dalam {amount}"),
        (false, true) => format!("{amount} ago"),
        (false, false) => format!("in {amount}"),
    }
}

/// Mirrors the thresholds moment.js used in the original app.
fn describe_days(language: Language, days: u64) -> String {
    let id = language == Language::Indonesian;
    let months = (days as f64 / 30.4).round() as u64;
    let years = (days as f64 / 365.25).round() as u64;
    match days {
        0..=25 => {
            if id {
                format!("{days} hari")
            } else {
                format!("{days} days")
            }
        }
        26..=45 => if id { "sebulan" } else { "a month" }.into(),
        _ if months < 11 => {
            if id {
                format!("{months} bulan")
            } else {
                format!("{months} months")
            }
        }
        _ if months < 18 => if id { "setahun" } else { "a year" }.into(),
        _ => {
            let years = years.max(2);
            if id {
                format!("{years} tahun")
            } else {
                format!("{years} years")
            }
        }
    }
}

/// Full years between `birth` and `today`.
pub fn age_years(birth: NaiveDate, today: NaiveDate) -> i32 {
    let mut age = today.year() - birth.year();
    if (today.month(), today.day()) < (birth.month(), birth.day()) {
        age -= 1;
    }
    age
}

/// `61 tahun` / `61 years`.
pub fn format_age(years: i32) -> String {
    pick(format!("{years} tahun"), format!("{years} years"))
}

/// `61 th` / `61 y`, for lists.
pub fn format_age_short(years: i32) -> String {
    pick(format!("{years} th"), format!("{years} y"))
}

/// `7 November 2026 (2 hari yang lalu)`.
pub fn display_with_relative(value: &str, today: NaiveDate) -> String {
    match parse_stored(value) {
        Some(date) => format!("{} ({})", format_long(date), format_relative(date, today)),
        None => value.to_string(),
    }
}

/// `6 Juni 1949 (77 tahun)`.
pub fn display_with_age(value: &str, today: NaiveDate) -> String {
    match parse_stored(value) {
        Some(date) => format!(
            "{} ({})",
            format_long(date),
            format_age(age_years(date, today))
        ),
        None => value.to_string(),
    }
}

#[cfg(test)]
mod dates_tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[test]
    fn parses_user_dates() {
        assert_eq!(parse_user_date("1990-1-5"), Ok(d(1990, 1, 5)));
        assert_eq!(parse_user_date(" 1949-06-06 "), Ok(d(1949, 6, 6)));
        assert_eq!(parse_user_date("05/01/1990"), Ok(d(1990, 1, 5)));
        assert_eq!(parse_user_date("5.1.1990"), Ok(d(1990, 1, 5)));
        assert_eq!(parse_user_date("2024-02-29"), Ok(d(2024, 2, 29)));
        assert!(parse_user_date("2021-02-29").is_err());
        assert!(parse_user_date("2021-13-01").is_err());
        assert!(parse_user_date("21-01-01").is_err());
        assert!(parse_user_date("").is_err());
        assert!(parse_user_date("kemarin").is_err());
    }

    #[test]
    fn stores_zero_padded_years() {
        assert_eq!(to_stored(d(8, 1, 29)), "0008-01-29");
        assert_eq!(parse_stored("0008-01-29"), Some(d(8, 1, 29)));
    }

    #[test]
    fn formats_in_indonesian() {
        assert_eq!(format_long(d(2026, 11, 7)), "7 November 2026");
        assert_eq!(display_date("2021-08-23"), "23 Agustus 2021");
        assert_eq!(display_date("garbage"), "garbage");
    }

    #[test]
    fn weekday_and_short_formats() {
        assert_eq!(
            format_with_weekday(d(2026, 11, 9)),
            "Senin, 9 November 2026"
        );
        assert_eq!(
            format_with_weekday(d(2026, 11, 15)),
            "Minggu, 15 November 2026"
        );
        assert_eq!(format_short(d(2021, 8, 23)), "23 Agu 2021");
        assert_eq!(format_short(d(2026, 5, 1)), "1 Mei 2026");
    }

    #[test]
    fn relative_dates() {
        let today = d(2026, 11, 9);
        assert_eq!(format_relative(today, today), "hari ini");
        assert_eq!(format_relative(d(2026, 11, 8), today), "kemarin");
        assert_eq!(format_relative(d(2026, 11, 6), today), "3 hari yang lalu");
        assert_eq!(format_relative(d(2026, 10, 9), today), "sebulan yang lalu");
        assert_eq!(format_relative(d(2026, 6, 9), today), "5 bulan yang lalu");
        assert_eq!(format_relative(d(2025, 11, 9), today), "setahun yang lalu");
        assert_eq!(format_relative(d(2021, 8, 23), today), "5 tahun yang lalu");
        assert_eq!(format_relative(d(2026, 11, 10), today), "besok");
        assert_eq!(format_relative(d(2026, 11, 14), today), "dalam 5 hari");
    }

    #[test]
    fn english() {
        let today = d(2026, 11, 9);
        let en = Language::English;
        assert_eq!(format_long_in(en, d(2026, 11, 7)), "7 November 2026");
        assert_eq!(
            format_with_weekday_in(en, d(2026, 11, 9)),
            "Monday, 9 November 2026"
        );
        assert_eq!(format_short_in(en, d(2021, 8, 23)), "23 Aug 2021");
        assert_eq!(format_relative_in(en, today, today), "today");
        assert_eq!(format_relative_in(en, d(2026, 11, 8), today), "yesterday");
        assert_eq!(format_relative_in(en, d(2026, 11, 6), today), "3 days ago");
        assert_eq!(format_relative_in(en, d(2026, 6, 9), today), "5 months ago");
        assert_eq!(format_relative_in(en, d(2026, 11, 14), today), "in 5 days");
    }

    #[test]
    fn ages() {
        let today = d(2026, 11, 9);
        assert_eq!(age_years(d(1949, 6, 6), today), 77);
        assert_eq!(age_years(d(2000, 11, 9), today), 26);
        assert_eq!(age_years(d(2000, 11, 10), today), 25);
        assert_eq!(
            display_with_age("1949-06-06", today),
            "6 Juni 1949 (77 tahun)"
        );
    }
}

// ============================================================================
// The language of the interface: Indonesian (the default) or English.
//
// Texts are written in both languages where they are used,
// `t("Pasien", "Patients")`, so each pair is reviewed together.
// ============================================================================

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
mod i18n_tests {
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

// ============================================================================
// Search keywords for patients.
//
// The original app split names on spaces only, so `Ma'ruf` or `Sumarni,`
// were indexed with their punctuation and could only be found by typing it
// exactly. Here apostrophes are dropped (`ma'ruf` -> `maruf`) and every other
// non-alphanumeric character separates words. The same normalisation is
// applied to queries, so whatever the user types matches the stored form.
// ============================================================================

/// Maximum number of distinct words accepted in one search, as before.
pub const MAX_QUERY_KEYWORDS: usize = 30;

fn is_apostrophe(c: char) -> bool {
    matches!(c, '\'' | '\u{2019}' | '\u{2018}' | '`' | '\u{02BC}')
}

/// Lower-cased, de-duplicated, sorted words of `text`.
pub fn tokenize(text: &str) -> Vec<String> {
    let mut words = BTreeSet::new();
    let mut current = String::new();
    for c in text.chars() {
        if is_apostrophe(c) {
            continue;
        }
        if c.is_alphanumeric() {
            current.extend(c.to_lowercase());
        } else if !current.is_empty() {
            words.insert(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        words.insert(current);
    }
    words.into_iter().collect()
}

/// Keywords indexed for a patient: the words of the name and the head of
/// family name.
pub fn patient_keywords(name: &str, head_family_name: &str) -> Vec<String> {
    tokenize(&format!("{name} {head_family_name}"))
}

#[cfg(test)]
mod keywords_tests {
    use super::*;

    #[test]
    fn splits_and_lowercases() {
        assert_eq!(tokenize("  Siti   NUR Aisyah "), ["aisyah", "nur", "siti"]);
    }

    #[test]
    fn handles_punctuation() {
        assert_eq!(tokenize("Ma'ruf"), ["maruf"]);
        assert_eq!(tokenize("Ma\u{2019}ruf"), ["maruf"]);
        assert_eq!(tokenize("Sumarni, S.Pd"), ["pd", "s", "sumarni"]);
        assert_eq!(
            tokenize("Bayi Ny. (Rina)/Joko"),
            ["bayi", "joko", "ny", "rina"]
        );
        assert!(tokenize("( ). /").is_empty());
    }

    #[test]
    fn deduplicates_name_and_family_name() {
        assert_eq!(
            patient_keywords("Joko Santoso", "Santoso"),
            ["joko", "santoso"]
        );
    }

    #[test]
    fn keeps_non_ascii_letters() {
        assert_eq!(tokenize("Éka ÇAHYA"), ["çahya", "éka"]);
    }
}

// ============================================================================
// The databases opened recently, newest first, kept in a small text file:
// one `<RFC 3339 time>\t<path>` line per database.
// ============================================================================

/// How many databases are remembered.
pub const MAX_RECENT: usize = 10;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecentDatabase {
    pub path: PathBuf,
    pub opened_at: DateTime<FixedOffset>,
}

pub struct RecentDatabases {
    file: PathBuf,
    entries: Vec<RecentDatabase>,
}

/// The folder of the application's own files, the one the original
/// application used (Go's `os.UserConfigDir`).
pub fn app_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("kutilang")
}

/// The database the original application used.
pub fn default_database() -> PathBuf {
    app_dir().join("db.sqlite3")
}

impl RecentDatabases {
    /// The list kept in the application folder.
    pub fn load_default() -> Self {
        Self::load(app_dir().join("recent-databases.txt"))
    }

    /// Reads the list from `file`; a missing or unreadable file is an empty
    /// list, and lines that cannot be read are skipped.
    pub fn load(file: PathBuf) -> Self {
        let entries = std::fs::read_to_string(&file)
            .unwrap_or_default()
            .lines()
            .filter_map(|line| {
                let (time, path) = line.split_once('\t')?;
                Some(RecentDatabase {
                    path: PathBuf::from(path),
                    opened_at: DateTime::parse_from_rfc3339(time).ok()?,
                })
            })
            .take(MAX_RECENT)
            .collect();
        Self { file, entries }
    }

    pub fn entries(&self) -> &[RecentDatabase] {
        &self.entries
    }

    /// Puts `path` first, as opened now.
    pub fn add(&mut self, path: &Path) -> Result<()> {
        let path = absolute(path);
        self.entries.retain(|entry| entry.path != path);
        self.entries.insert(
            0,
            RecentDatabase {
                path,
                opened_at: Local::now().fixed_offset(),
            },
        );
        self.entries.truncate(MAX_RECENT);
        self.save()
    }

    pub fn remove(&mut self, path: &Path) -> Result<()> {
        self.entries.retain(|entry| entry.path != path);
        self.save()
    }

    fn save(&self) -> Result<()> {
        if let Some(dir) = self.file.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let text: String = self
            .entries
            .iter()
            .map(|entry| {
                format!(
                    "{}\t{}\n",
                    entry.opened_at.to_rfc3339(),
                    entry.path.display()
                )
            })
            .collect();
        std::fs::write(&self.file, text)?;
        Ok(())
    }
}

/// The path made absolute, and resolved when the file exists, so the same
/// file is remembered once.
fn absolute(path: &Path) -> PathBuf {
    path.canonicalize()
        .or_else(|_| std::path::absolute(path))
        .unwrap_or_else(|_| path.to_path_buf())
}

#[cfg(test)]
mod recent_tests {
    use super::*;

    #[test]
    fn remembers_databases_newest_first() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("config/recent.txt");
        let a = dir.path().join("a.sqlite3");
        let b = dir.path().join("b.sqlite3");
        std::fs::write(&a, "").unwrap();

        let mut recent = RecentDatabases::load(file.clone());
        assert!(recent.entries().is_empty());
        recent.add(&a).unwrap();
        recent.add(&b).unwrap();
        // Opening `a` again moves it first instead of listing it twice.
        recent.add(&dir.path().join("./a.sqlite3")).unwrap();

        let reloaded = RecentDatabases::load(file.clone());
        let paths: Vec<_> = reloaded.entries().iter().map(|e| e.path.clone()).collect();
        assert_eq!(paths, [a.canonicalize().unwrap(), absolute(&b)]);

        let mut reloaded = reloaded;
        reloaded.remove(&absolute(&b)).unwrap();
        assert_eq!(RecentDatabases::load(file).entries().len(), 1);
    }

    #[test]
    fn keeps_the_most_recent_ten_and_skips_bad_lines() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("recent.txt");
        let mut recent = RecentDatabases::load(file.clone());
        for i in 0..15 {
            recent
                .add(&dir.path().join(format!("{i}.sqlite3")))
                .unwrap();
        }
        let mut text = std::fs::read_to_string(&file).unwrap();
        text.insert_str(0, "not a line\nyesterday\t/x.sqlite3\n");
        std::fs::write(&file, text).unwrap();

        let recent = RecentDatabases::load(file);
        assert_eq!(recent.entries().len(), MAX_RECENT);
        assert!(recent.entries()[0].path.ends_with("14.sqlite3"));
    }
}
