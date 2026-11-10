//! Date parsing, and the date formatting used across the interface, in the
//! interface's language.
//!
//! Dates are stored as `YYYY-MM-DD` text, exactly like the original app, so
//! they sort correctly as strings.

use chrono::{Datelike, Local, NaiveDate};

use crate::i18n::{self, Language, t};

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
    format_long_in(i18n::language(), date)
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
    format_with_weekday_in(i18n::language(), date)
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
    format_short_in(i18n::language(), date)
}

pub fn format_short_in(language: Language, date: NaiveDate) -> String {
    let month = month_name(language, date);
    let month = month.get(..3).unwrap_or(month);
    format!("{} {} {}", date.day(), month, date.year())
}

/// Formats a stored date for display, falling back to the raw text when it
/// is not a valid date.
pub fn display(value: &str) -> String {
    parse_stored(value).map_or_else(|| value.to_string(), format_long)
}

/// A day-granular relative description, e.g. `3 hari yang lalu`.
pub fn format_relative(date: NaiveDate, today: NaiveDate) -> String {
    format_relative_in(i18n::language(), date, today)
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
    i18n::pick(format!("{years} tahun"), format!("{years} years"))
}

/// `61 th` / `61 y`, for lists.
pub fn format_age_short(years: i32) -> String {
    i18n::pick(format!("{years} th"), format!("{years} y"))
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
mod tests {
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
        assert_eq!(display("2021-08-23"), "23 Agustus 2021");
        assert_eq!(display("garbage"), "garbage");
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
