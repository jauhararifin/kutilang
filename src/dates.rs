//! Date parsing and the Indonesian date formatting used across the UI.
//!
//! Dates are stored as `YYYY-MM-DD` text, exactly like the original app, so
//! they sort correctly as strings.

use chrono::{Datelike, Local, NaiveDate};

const MONTHS: [&str; 12] = [
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
        return Err("Required".into());
    }

    let parts: Vec<&str> = input.split(['-', '/', '.']).map(str::trim).collect();
    let numbers: Option<Vec<u32>> = parts.iter().map(|p| p.parse::<u32>().ok()).collect();
    let (Some(numbers), 3) = (numbers, parts.len()) else {
        return Err("Invalid date, use YYYY-MM-DD".into());
    };

    let (year, month, day) = if parts[0].len() == 4 {
        (numbers[0], numbers[1], numbers[2])
    } else if parts[2].len() == 4 {
        (numbers[2], numbers[1], numbers[0])
    } else {
        return Err("Invalid date, use YYYY-MM-DD".into());
    };

    if !(1..=9999).contains(&year) {
        return Err("Invalid year".into());
    }
    NaiveDate::from_ymd_opt(year as i32, month, day).ok_or_else(|| "Invalid date".to_string())
}

/// `7 November 2026`.
pub fn format_long(date: NaiveDate) -> String {
    format!(
        "{} {} {}",
        date.day(),
        MONTHS[date.month0() as usize],
        date.year()
    )
}

const WEEKDAYS: [&str; 7] = ["Senin", "Selasa", "Rabu", "Kamis", "Jumat", "Sabtu", "Minggu"];

/// `Senin, 9 November 2026`.
pub fn format_with_weekday(date: NaiveDate) -> String {
    format!(
        "{}, {}",
        WEEKDAYS[date.weekday().num_days_from_monday() as usize],
        format_long(date)
    )
}

/// `9 Nov 2026`.
pub fn format_short(date: NaiveDate) -> String {
    let month = MONTHS[date.month0() as usize];
    let month = month.get(..3).unwrap_or(month);
    // Three letters: Jan, Feb, ..., Agu, ..., Des.
    format!("{} {} {}", date.day(), month, date.year())
}

/// Formats a stored date for display, falling back to the raw text when it
/// is not a valid date.
pub fn display(value: &str) -> String {
    parse_stored(value).map_or_else(|| value.to_string(), format_long)
}

/// A day-granular relative description in Indonesian, e.g. `3 hari yang lalu`.
pub fn format_relative(date: NaiveDate, today: NaiveDate) -> String {
    let days = (today - date).num_days();
    if days == 0 {
        return "hari ini".into();
    }
    if days == 1 {
        return "kemarin".into();
    }
    if days == -1 {
        return "besok".into();
    }

    let amount = describe_days(days.unsigned_abs());
    if days > 0 {
        format!("{amount} yang lalu")
    } else {
        format!("dalam {amount}")
    }
}

/// Mirrors the thresholds moment.js used in the original app.
fn describe_days(days: u64) -> String {
    let months = (days as f64 / 30.4).round() as u64;
    let years = (days as f64 / 365.25).round() as u64;
    match days {
        0..=25 => format!("{days} hari"),
        26..=45 => "sebulan".into(),
        _ if months < 11 => format!("{months} bulan"),
        _ if months < 18 => "setahun".into(),
        _ => format!("{} tahun", years.max(2)),
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
            "{} ({} tahun)",
            format_long(date),
            age_years(date, today)
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
        assert_eq!(format_with_weekday(d(2026, 11, 9)), "Senin, 9 November 2026");
        assert_eq!(format_with_weekday(d(2026, 11, 15)), "Minggu, 15 November 2026");
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
    fn ages() {
        let today = d(2026, 11, 9);
        assert_eq!(age_years(d(1949, 6, 6), today), 77);
        assert_eq!(age_years(d(2000, 11, 9), today), 26);
        assert_eq!(age_years(d(2000, 11, 10), today), 25);
        assert_eq!(display_with_age("1949-06-06", today), "6 Juni 1949 (77 tahun)");
    }
}
