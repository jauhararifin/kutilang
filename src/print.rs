//! Printing a visit.
//!
//! GPUI has no printing support, so the record is written as an HTML page
//! that opens the print dialog when the browser loads it, the same output the
//! original application printed.

use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::dates;
use crate::diagnosis;
use crate::model::{Patient, Visit};

pub fn escape_html(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

/// The printable page of a visit.
pub fn visit_page(patient: &Patient, visit: &Visit) -> String {
    let name = if patient.head_family_name.trim().is_empty() {
        escape_html(&patient.name)
    } else {
        format!(
            "{} / {}",
            escape_html(&patient.name),
            escape_html(&patient.head_family_name)
        )
    };
    let visit_date = dates::display(&visit.date);
    format!(
        r#"<!doctype html>
<html lang="id">
<head>
<meta charset="utf-8">
<title>{title}</title>
<style>
  body {{ font-family: -apple-system, "Segoe UI", Roboto, Helvetica, Arial, sans-serif; margin: 24px; color: #000; }}
  .profile {{ border: 1px solid #000; padding: 10px; }}
  .profile th {{ text-align: left; padding: 4px 16px 4px 0; vertical-align: top; white-space: nowrap; }}
  .profile td {{ padding: 4px 0; }}
  .date {{ text-align: center; padding-top: 20px; font-size: 1.15em; }}
  .diagnosis {{ margin-top: 20px; padding: 5px; font-size: 12px; }}
  @media print {{ body {{ margin: 0; }} }}
</style>
</head>
<body onload="window.print()">
<div class="profile">
<table>
<tr><th>Nama</th><td>{name}</td></tr>
<tr><th>Tanggal Lahir</th><td>{birth_date}</td></tr>
<tr><th>Alamat</th><td>{address}</td></tr>
<tr><th>Kunjungan Terakhir</th><td>{last_visit}</td></tr>
</table>
</div>
<h5 class="date">{visit_date}</h5>
<div class="diagnosis">{diagnosis}</div>
</body>
</html>
"#,
        title = escape_html(&format!("{} - {}", patient.name, visit_date)),
        birth_date = escape_html(&dates::display(&patient.birth_date)),
        address = escape_html(&patient.address).replace('\n', "<br>"),
        last_visit = escape_html(&dates::display(&patient.last_visit)),
        visit_date = escape_html(&visit_date),
        diagnosis = diagnosis::to_html(&visit.diagnosis, visit.format),
    )
}

/// Writes the printable page into the temporary directory.
pub fn write_visit_page(patient: &Patient, visit: &Visit) -> Result<PathBuf> {
    let dir = std::env::temp_dir().join("kutilang-print");
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("visit-{}.html", visit.id));
    std::fs::write(&path, visit_page(patient, visit))?;
    Ok(path)
}

/// A `file://` URL for a local path, percent-encoded.
pub fn file_url(path: &Path) -> String {
    let path = path.to_string_lossy().replace('\\', "/");
    let mut url = String::from("file://");
    if !path.starts_with('/') {
        url.push('/'); // Windows drive paths: file:///C:/...
    }
    for byte in path.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' | b':' => {
                url.push(byte as char)
            }
            _ => url.push_str(&format!("%{byte:02X}")),
        }
    }
    url
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::DiagnosisFormat;

    fn patient() -> Patient {
        Patient {
            id: "p".into(),
            name: "Siti <Aminah>".into(),
            head_family_name: "Joko".into(),
            birth_date: "1990-01-05".into(),
            address: "Jl. Mawar".into(),
            last_visit: "2026-11-07".into(),
        }
    }

    #[test]
    fn page_contains_profile_and_diagnosis() {
        let visit = Visit {
            id: "v".into(),
            patient_id: "p".into(),
            date: "2026-11-07".into(),
            diagnosis: "**KU**: baik".into(),
            format: DiagnosisFormat::Markdown,
        };
        let page = visit_page(&patient(), &visit);
        assert!(page.contains("Siti &lt;Aminah&gt; / Joko"));
        assert!(page.contains("5 Januari 1990"));
        assert!(page.contains("<h5 class=\"date\">7 November 2026</h5>"));
        assert!(page.contains("<strong>KU</strong>: baik"));
        assert!(page.contains("window.print()"));
    }

    #[test]
    fn html_entries_print_unchanged() {
        let visit = Visit {
            id: "v".into(),
            patient_id: "p".into(),
            date: "2021-08-23".into(),
            diagnosis: "<p><strong>KU</strong>: baik</p>".into(),
            format: DiagnosisFormat::Html,
        };
        assert!(visit_page(&patient(), &visit).contains("<p><strong>KU</strong>: baik</p>"));
    }

    #[test]
    fn file_urls() {
        assert_eq!(
            file_url(Path::new("/tmp/kutilang print/v.html")),
            "file:///tmp/kutilang%20print/v.html"
        );
        assert_eq!(
            file_url(Path::new("C:\\Users\\Dr A\\v.html")),
            "file:///C:/Users/Dr%20A/v.html"
        );
    }
}
