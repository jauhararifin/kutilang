//! Exporting every patient and visit to an Excel workbook.

use std::path::Path;

use anyhow::Result;
use chrono::Datelike as _;
use rust_xlsxwriter::{ExcelDateTime, Format, FormatAlign, Workbook, Worksheet};

use crate::dates;
use crate::diagnosis;
use crate::i18n::t;
use crate::store::Store;

/// The most text an Excel cell can hold.
const CELL_LIMIT: usize = 32_767;

/// What was written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExportReport {
    pub patients: usize,
    pub visits: usize,
}

struct Formats {
    header: Format,
    date: Format,
    wrap: Format,
}

/// A stored `YYYY-MM-DD` date as an Excel date, or as text when Excel
/// cannot represent it (before 1900, or not a date).
fn write_date(
    sheet: &mut Worksheet,
    row: u32,
    col: u16,
    value: &str,
    formats: &Formats,
) -> Result<()> {
    let date = dates::parse_stored(value).and_then(|d| {
        ExcelDateTime::from_ymd(d.year().try_into().ok()?, d.month() as u8, d.day() as u8).ok()
    });
    match date {
        Some(date) => sheet.write_datetime_with_format(row, col, &date, &formats.date)?,
        None => sheet.write_string(row, col, value)?,
    };
    Ok(())
}

fn limit(text: String) -> String {
    if text.chars().count() <= CELL_LIMIT {
        text
    } else {
        text.chars().take(CELL_LIMIT).collect()
    }
}

fn header(sheet: &mut Worksheet, columns: &[(&str, f64)], formats: &Formats) -> Result<()> {
    for (col, (title, width)) in columns.iter().enumerate() {
        sheet.write_string_with_format(0, col as u16, *title, &formats.header)?;
        sheet.set_column_width(col as u16, *width)?;
    }
    sheet.set_freeze_panes(1, 0)?;
    Ok(())
}

/// Writes a workbook with a sheet of patients and a sheet of visits.
pub fn export_xlsx(store: &Store, path: &Path) -> Result<ExportReport> {
    let today = dates::today();
    let formats = Formats {
        header: Format::new().set_bold().set_background_color(0xE7E6E6),
        date: Format::new()
            .set_num_format("dd/mm/yyyy")
            .set_align(FormatAlign::Top),
        wrap: Format::new().set_text_wrap().set_align(FormatAlign::Top),
    };
    let mut workbook = Workbook::new();

    let patients = store.export_patients()?;
    let sheet = workbook.add_worksheet();
    sheet.set_name(t("Pasien", "Patients"))?;
    let columns = [
        (t("Nama", "Name"), 28.0),
        (t("Nama KK", "Head of Family"), 24.0),
        (t("Tanggal Lahir", "Date of Birth"), 14.0),
        (t("Umur", "Age"), 7.0),
        (t("Alamat", "Address"), 40.0),
        (t("Kunjungan Terakhir", "Last Visit"), 18.0),
        (t("Jumlah Kunjungan", "Visits"), 17.0),
        (t("ID Pasien", "Patient ID"), 34.0),
    ];
    header(sheet, &columns, &formats)?;
    for (i, patient) in patients.iter().enumerate() {
        let row = i as u32 + 1;
        sheet.write_string(row, 0, &patient.name)?;
        sheet.write_string(row, 1, &patient.head_family_name)?;
        write_date(sheet, row, 2, &patient.birth_date, &formats)?;
        if let Some(birth) = dates::parse_stored(&patient.birth_date) {
            sheet.write_number(row, 3, dates::age_years(birth, today))?;
        }
        sheet.write_string(row, 4, &patient.address)?;
        write_date(sheet, row, 5, &patient.last_visit, &formats)?;
        sheet.write_number(row, 6, patient.visits as f64)?;
        sheet.write_string(row, 7, &patient.id)?;
    }
    sheet.autofilter(0, 0, patients.len() as u32, columns.len() as u16 - 1)?;

    let visits = store.export_visits()?;
    let sheet = workbook.add_worksheet();
    sheet.set_name(t("Kunjungan", "Visits"))?;
    let columns = [
        (t("Tanggal", "Date"), 12.0),
        (t("Nama", "Name"), 28.0),
        (t("Nama KK", "Head of Family"), 24.0),
        (t("Diagnosis", "Diagnosis"), 90.0),
        (t("ID Pasien", "Patient ID"), 34.0),
        (t("ID Kunjungan", "Visit ID"), 34.0),
    ];
    header(sheet, &columns, &formats)?;
    for (i, visit) in visits.iter().enumerate() {
        let row = i as u32 + 1;
        write_date(sheet, row, 0, &visit.date, &formats)?;
        sheet.write_string_with_format(row, 1, &visit.patient_name, &formats.wrap)?;
        sheet.write_string_with_format(row, 2, &visit.head_family_name, &formats.wrap)?;
        let text = limit(diagnosis::plain_text(&visit.diagnosis, visit.format));
        sheet.write_string_with_format(row, 3, text, &formats.wrap)?;
        sheet.write_string_with_format(row, 4, &visit.patient_id, &formats.wrap)?;
        sheet.write_string_with_format(row, 5, &visit.id, &formats.wrap)?;
    }
    sheet.autofilter(0, 0, visits.len() as u32, columns.len() as u16 - 1)?;

    workbook.save(path)?;
    Ok(ExportReport {
        patients: patients.len(),
        visits: visits.len(),
    })
}
