//! Diagnosis text: the visit template, the plain text of a visit (for
//! showing, editing, listing and exporting it), and printing.
//!
//! Visits are written as plain text. Visits of the original application are
//! stored in the HTML of its rich text editor: they are shown as plain text,
//! and their HTML is kept until their text is edited.

use crate::{DiagnosisFormat, Patient, Visit, display_date, language, t};
use anyhow::Result;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

// ============================================================================
// The visit template, and the plain text of a visit.
// ============================================================================

/// The template of a new visit: the original application's, as plain text.
/// The space after `KU:` and `1.` lets typing continue the line.
pub const TEMPLATE: &str = concat!(
    "Subjektif\n",
    "KU: \n",
    "RPS: \n",
    "RPD: Maag (-), HT (-), DM (-), Asma (-), Tipes (-), Alergi obat (-), Sakit jantung (-), minum jamu / obat bebas (-)\n",
    "\n",
    "Objektif\n",
    "- KU: Baik, CM. TD: / mmHg, HR: x/mnt reguler, BB: kg, TB: cm.\n",
    "- Konj anemis (-), Sclera Icterus (-). JVP R-2 cm, Kel. tiroid: dbn\n",
    "- Thorax: SSD, SF: ka = ki. Pulmo: SD vesikuler, Ronchi: -, Wheezing: -.\n",
    "- Cor: konfigurasi dbn, S1-S2 murni, bising (-), gallop (-).\n",
    "- Abd: BU (+), venectasi (-), asites (-), Hepar: ttb, Lien: ttb. Nyeri ketok CF: ka (-) ki (-)\n",
    "- Extr: edema -/-, Ulcus -/-, tremor (-), Palmar eritem (-). jari tabuh (-). ptechiae (-).\n",
    "- GDS: mg/ml, AU: mg/ml, Kol. Total: mg/ml, EKG: \n",
    "\n",
    "Assesment\n",
    "1. \n",
    "\n",
    "Planning\n",
    "1. \n",
);

/// The text of a visit, as it is shown, edited, printed and exported.
pub fn plain_text(diagnosis: &str, format: DiagnosisFormat) -> String {
    match format {
        DiagnosisFormat::Text => diagnosis.trim_end().to_string(),
        DiagnosisFormat::Html => html_to_text(diagnosis),
    }
}

/// First words of the template's section headings, left out of summaries.
const SECTIONS: [&str; 7] = [
    "subjektif",
    "objektif",
    "assesment",
    "assessment",
    "analisis",
    "planning",
    "pengobatan",
];

/// `Planning`, or a note in a heading (`Subjektif ( alergi cipro )`).
fn is_heading(line: &str) -> bool {
    !line.contains(':')
        && line
            .split_whitespace()
            .next()
            .is_some_and(|word| SECTIONS.contains(&word.to_lowercase().as_str()))
}

/// `KU:` with nothing written after it.
fn is_empty_label(line: &str) -> bool {
    match line.split_once(':') {
        Some((label, rest)) => rest.trim().is_empty() && label.chars().count() <= 30,
        None => false,
    }
}

/// The line without its list marker (`- `, `1. `, `2) `).
fn without_list_marker(line: &str) -> &str {
    if line == "-" || line == "•" {
        return "";
    }
    if let Some(rest) = line.strip_prefix("- ").or_else(|| line.strip_prefix("• ")) {
        return rest;
    }
    let digits = line.chars().take_while(char::is_ascii_digit).count();
    match line[digits..].strip_prefix(['.', ')']) {
        Some(rest) if digits > 0 && (rest.is_empty() || rest.starts_with(' ')) => rest,
        _ => line,
    }
}

/// The first lines of a visit as one line, for lists: section headings,
/// empty labels (`RPS:`) and empty list items are left out, lines are joined
/// with ` · ` and the result is cut to `max_chars`.
pub fn summary(diagnosis: &str, format: DiagnosisFormat, max_chars: usize) -> String {
    let text = plain_text(diagnosis, format);
    let mut out = String::new();
    for line in text.lines() {
        let line = without_list_marker(line.trim())
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        if line.is_empty() || is_heading(&line) || is_empty_label(&line) {
            continue;
        }
        if !out.is_empty() {
            out.push_str(" · ");
        }
        out.push_str(&line);
        if out.chars().count() >= max_chars {
            break;
        }
    }
    if out.chars().count() > max_chars {
        out = out
            .chars()
            .take(max_chars.saturating_sub(1))
            .collect::<String>();
        out.push('…');
    }
    out
}

/// Text written in Markdown by earlier versions of this application, as
/// plain text: heading marks, emphasis and escapes go; list markers and line
/// breaks stay.
pub fn markdown_to_text(markdown: &str) -> String {
    // Split on `\n` rather than `lines()`, to keep a final line break.
    markdown
        .split('\n')
        .map(|line| {
            let line = line.strip_suffix('\r').unwrap_or(line);
            let trimmed = line.trim_start();
            let hashes = trimmed.chars().take_while(|c| *c == '#').count();
            let line = if (1..=6).contains(&hashes)
                && (trimmed.len() == hashes || trimmed[hashes..].starts_with(' '))
            {
                trimmed[hashes..].trim_start()
            } else if let Some(item) = trimmed.strip_prefix("* ") {
                // `* item` is a bullet, not emphasis.
                return format!("- {}", markdown_inline_to_text(item));
            } else {
                line
            };
            markdown_inline_to_text(line)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn markdown_inline_to_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => out.extend(chars.next()),
            '*' => {}
            _ => out.push(c),
        }
    }
    out
}

// ============================================================================
// The HTML written by the original application's rich text editor
// (draft.js), as plain text.
//
// That editor wrote little: `h5` headings, paragraphs, bulleted and numbered
// lists, line breaks, and inline tags for bold, italic, underline, links and
// pasted styles. A small scanner is enough; every visit of the original
// database is checked against a full HTML parser in the tests.
// ============================================================================

/// The text of the HTML: one line per paragraph, heading or list item
/// (written `- ` or `1. `), a blank line before each heading, and the line
/// breaks the text contains.
pub fn html_to_text(html: &str) -> String {
    let mut out = TextBuilder::default();
    let mut rest = html;
    while let Some(start) = rest.find('<') {
        out.text(&rest[..start]);
        rest = &rest[start..];
        // `<` starts a tag only before a letter, `/` or `!`.
        let next = rest[1..].chars().next();
        if !next.is_some_and(|c| c.is_ascii_alphabetic() || c == '/' || c == '!') {
            out.text("<");
            rest = &rest[1..];
            continue;
        }
        if let Some(comment) = rest.strip_prefix("<!--") {
            rest = comment.find("-->").map_or("", |end| &comment[end + 3..]);
            continue;
        }
        // An unfinished tag (a cut summary) ends the text.
        let Some(end) = tag_end(rest) else {
            rest = "";
            break;
        };
        let tag = &rest[1..end];
        rest = &rest[end + 1..];
        let closing = tag.starts_with('/');
        let name = tag
            .trim_start_matches('/')
            .split(|c: char| c.is_whitespace() || c == '/')
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase();
        match name.as_str() {
            "script" | "style" if !closing => {
                let close = format!("</{name}");
                rest = rest
                    .to_ascii_lowercase()
                    .find(&close)
                    .map_or("", |at| &rest[at..]);
            }
            "br" => out.line_break(),
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" if !closing => out.heading(),
            "ul" | "ol" if !closing => out.start_list(name == "ol"),
            "ul" | "ol" => out.end_list(),
            "li" if !closing => out.item(),
            "p" | "div" | "li" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "blockquote" | "pre"
            | "table" | "tr" | "section" | "article" | "header" | "footer" | "hr" => {
                out.end_block()
            }
            // Inline tags (`strong`, `em`, `span`, `a`...): only their text.
            _ => {}
        }
    }
    out.text(rest);
    out.finish()
}

/// Where the tag at the start of `html` ends: the first `>` outside a quoted
/// attribute value. As in HTML, a quote opens a value only right after `=`;
/// the old editor wrote `style="font-family: "Times New Roman", serif;"`,
/// whose other quotes are plain characters.
fn tag_end(html: &str) -> Option<usize> {
    let bytes = html.as_bytes();
    let mut after_equals = false;
    let mut i = 1;
    while i < bytes.len() {
        match bytes[i] {
            b'>' => return Some(i),
            b'=' => after_equals = true,
            quote @ (b'"' | b'\'') if after_equals => {
                let close = bytes[i + 1..].iter().position(|&b| b == quote)?;
                i += close + 1;
                after_equals = false;
            }
            b if b.is_ascii_whitespace() => {}
            _ => after_equals = false,
        }
        i += 1;
    }
    None
}

#[derive(Default)]
struct TextBuilder {
    lines: Vec<String>,
    line: String,
    /// Where the text of `line` starts, after the indentation of a list item
    /// continued on a new line.
    text_start: usize,
    /// `line` starts a list item: it is kept even when the item is empty.
    item: bool,
    /// Whitespace came, to be written as one space before the next text.
    space: bool,
    /// The open lists: `None` for bullets, `Some(last number)` for numbers.
    lists: Vec<Option<usize>>,
    /// The indentation of the lines continuing the current list item.
    hanging: String,
}

impl TextBuilder {
    fn has_text(&self) -> bool {
        self.line.len() > self.text_start
    }

    fn text(&mut self, html_text: &str) {
        for c in decode_entities(html_text).chars() {
            if c == '\u{a0}' {
                // A space that HTML does not collapse, but not at a line start.
                if self.has_text() {
                    self.space = false;
                    self.line.push(' ');
                }
            } else if c.is_whitespace() {
                self.space = true;
            } else {
                if std::mem::take(&mut self.space) && self.has_text() && !self.line.ends_with(' ') {
                    self.line.push(' ');
                }
                self.line.push(c);
            }
        }
    }

    fn push_line(&mut self) {
        let line = std::mem::take(&mut self.line);
        self.lines.push(line.trim_end().to_string());
        self.space = false;
        self.item = false;
    }

    /// `<br>`: the line ends, even when empty (an empty paragraph).
    fn line_break(&mut self) {
        self.push_line();
        self.line = self.hanging.clone();
        self.text_start = self.line.len();
    }

    /// The end of a paragraph, heading or list item.
    fn end_block(&mut self) {
        if self.has_text() || self.item {
            self.push_line();
        }
        self.line.clear();
        self.text_start = 0;
        self.space = false;
        self.item = false;
        self.hanging.clear();
    }

    fn heading(&mut self) {
        self.end_block();
        if self.lines.last().is_some_and(|line| !line.is_empty()) {
            self.lines.push(String::new());
        }
    }

    fn start_list(&mut self, ordered: bool) {
        self.end_block();
        self.lists.push(ordered.then_some(0));
    }

    fn end_list(&mut self) {
        self.end_block();
        self.lists.pop();
    }

    fn item(&mut self) {
        self.end_block();
        let indent = "   ".repeat(self.lists.len().saturating_sub(1));
        let marker = match self.lists.last_mut() {
            Some(Some(number)) => {
                *number += 1;
                format!("{number}. ")
            }
            _ => "- ".to_string(),
        };
        self.line = format!("{indent}{marker}");
        self.hanging = " ".repeat(self.line.len());
        self.text_start = self.line.len();
        self.item = true;
    }

    fn finish(mut self) -> String {
        self.end_block();
        while self.lines.last().is_some_and(String::is_empty) {
            self.lines.pop();
        }
        let start = self.lines.iter().take_while(|l| l.is_empty()).count();
        self.lines[start..].join("\n")
    }
}

fn decode_entities(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find('&') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let decoded = after.find(';').filter(|end| *end <= 8).and_then(|end| {
            let entity = &after[..end];
            let c = match entity {
                "nbsp" => Some('\u{a0}'),
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" => Some('\''),
                _ => entity
                    .strip_prefix('#')
                    .and_then(|n| match n.strip_prefix(['x', 'X']) {
                        Some(hex) => u32::from_str_radix(hex, 16).ok(),
                        None => n.parse::<u32>().ok(),
                    })
                    .filter(|&n| n != 0)
                    .and_then(char::from_u32),
            };
            c.map(|c| (c, end))
        });
        match decoded {
            Some((c, end)) => {
                out.push(c);
                rest = &after[end + 1..];
            }
            None => {
                out.push('&');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod diagnosis_tests {
    use super::*;

    /// The template of the original application, as it wrote it.
    const ORIGINAL_TEMPLATE: &str = "<h5>Subjektif</h5>
<p><strong>KU</strong>:</p>
<p><strong>RPS</strong>:</p>
<p><strong>RPD</strong>: Maag (-), HT (-), DM (-), Asma (-), Tipes (-), Alergi obat (-), Sakit jantung (-), minum jamu / obat bebas (-)</p>
<h5>Objektif</h5>
<ul>
\t<li><strong>KU</strong>: Baik, CM. <strong>TD</strong>: / mmHg, <strong>HR</strong>: x/mnt reguler, <strong>BB</strong>: kg, <strong>TB</strong>: cm.</li>
\t<li><strong>Konj</strong> anemis (-), <strong>Sclera</strong> Icterus (-). JVP R-2 cm, <strong>Kel. tiroid</strong>: dbn</li>
\t<li><strong>Thorax</strong>: SSD, SF: ka = ki. <strong>Pulmo</strong>: SD vesikuler, Ronchi: -, Wheezing: -.</li>
\t<li><strong>Cor</strong>: konfigurasi dbn, S1-S2 murni, bising (-), gallop (-).</li>
\t<li><strong>Abd</strong>: BU (+), venectasi (-), asites (-), <strong>Hepar</strong>: ttb, <strong>Lien</strong>: ttb. <strong>Nyeri ketok CF:</strong>ka (-) ki (-)</li>
\t<li><strong>Extr</strong>: edema -/-, Ulcus -/-, tremor (-), Palmar eritem (-). jari tabuh (-). ptechiae (-).</li>
\t<li><strong>GDS</strong>: mg/ml, <strong>AU</strong>: mg/ml, <strong>Kol. Total</strong>: mg/ml, <strong>EKG</strong>:
</ul>
<h5>Assesment</h5>
<ol>
\t<li></li>
</ol>
<h5>Planning</h5>
<ol>
\t<li></li>
</ol>
";

    #[test]
    fn old_visits_read_like_the_template() {
        let lines = |text: &str| {
            text.lines()
                .map(str::trim_end)
                .collect::<Vec<_>>()
                .join("\n")
        };
        // The one difference: the template adds the missing space after `CF:`.
        assert_eq!(
            html_to_text(ORIGINAL_TEMPLATE).replace("CF:ka", "CF: ka"),
            lines(TEMPLATE)
        );
    }

    #[test]
    fn html_lines_lists_and_breaks() {
        assert_eq!(
            html_to_text(
                "<h5>Subjektif</h5><p><strong>KU</strong>:&nbsp;meriang&nbsp;&nbsp;lemas </p>\
                 <p><br></p><p>a<br>b<br></p><h5>Planning</h5>\
                 <ol><li>Paracetamol<br>3dd1</li><li><em>L Bio</em> 1dd1<br></li><li></li></ol>"
            ),
            "Subjektif\nKU: meriang  lemas\n\na\nb\n\nPlanning\n\
             1. Paracetamol\n   3dd1\n2. L Bio 1dd1\n3."
        );
        // draft.js wrote nested lists inside the parent list.
        assert_eq!(
            html_to_text("<ul><li>a</li><ul><li>b</li></ul><li>c</li></ul>"),
            "- a\n   - b\n- c"
        );
        assert_eq!(
            html_to_text("<p>1 &lt; 2 &amp;&#x41;&#66; &unknown; a < b</p>"),
            "1 < 2 &AB &unknown; a < b"
        );
        assert_eq!(
            html_to_text("<p title=\"a>b\">x</p><!-- <p>no</p> --><style>p{}</style>y"),
            "x\ny"
        );
        // Quotes the old editor nested inside a style.
        assert_eq!(
            html_to_text(
                "<p><span style=\"color: black; font-family: \"Times New Roman\", serif;\">KU</span>: baik</p>\
                 <p><span style='a: 'b' c'>x</span> y</p>"
            ),
            "KU: baik\nx y"
        );
        assert_eq!(html_to_text("<p>cut here</p><p><str"), "cut here");
        assert_eq!(html_to_text(""), "");
    }

    #[test]
    fn summaries_skip_headings_empty_labels_and_markers() {
        let html = "<h5>Subjektif</h5>\n<p><strong>KU</strong>: meriang&nbsp;</p>\
                    <p><strong>RPS</strong>:</p><ul><li><strong>TD</strong>: 120/80 &amp; HR 80</li></ul>";
        assert_eq!(
            summary(html, DiagnosisFormat::Html, 160),
            "KU: meriang · TD: 120/80 & HR 80"
        );
        let text = TEMPLATE
            .replace("KU: \n", "KU: lemas\n")
            .replacen("1. \n", "1. ISPA\n", 1);
        let line = summary(&text, DiagnosisFormat::Text, 2000);
        assert!(line.starts_with("KU: lemas · RPD: Maag (-)"), "{line}");
        assert!(line.ends_with("ISPA"), "{line}");
        assert!(!line.contains("Subjektif") && !line.contains("RPS") && !line.contains("1."));
        let short = summary(TEMPLATE, DiagnosisFormat::Text, 20);
        assert_eq!(short.chars().count(), 20);
        assert!(short.ends_with('…'));
        assert_eq!(summary("", DiagnosisFormat::Html, 20), "");
    }

    #[test]
    fn plain_text_of_both_formats() {
        assert_eq!(
            plain_text("KU: lemas\n- TD: 120/80\n\n", DiagnosisFormat::Text),
            "KU: lemas\n- TD: 120/80"
        );
        assert_eq!(
            plain_text("<p><strong>KU</strong>: baik</p>", DiagnosisFormat::Html),
            "KU: baik"
        );
    }

    #[test]
    fn markdown_of_earlier_versions_becomes_text() {
        assert_eq!(
            markdown_to_text(
                "##### Subjektif\n\n**KU**: lemas \\*x\n\n- **TD**: 120/80\n* *b*\n1\\. tidak\n1. ISPA"
            ),
            "Subjektif\n\nKU: lemas *x\n\n- TD: 120/80\n- b\n1. tidak\n1. ISPA"
        );
        assert_eq!(markdown_to_text("**Keluhan**: \r\n"), "Keluhan: \n");
    }
}

// ============================================================================
// Printing a visit.
//
// GPUI has no printing support, so the record is written as an HTML page
// that opens the print dialog when the browser loads it, the same output the
// original application printed.
// ============================================================================

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
    let visit_date = display_date(&visit.date);
    format!(
        r#"<!doctype html>
<html lang="{lang}">
<head>
<meta charset="utf-8">
<title>{title}</title>
<style>
  body {{ font-family: -apple-system, "Segoe UI", Roboto, Helvetica, Arial, sans-serif; margin: 24px; color: #000; }}
  .profile {{ border: 1px solid #000; padding: 10px; }}
  .profile th {{ text-align: left; padding: 4px 16px 4px 0; vertical-align: top; white-space: nowrap; }}
  .profile td {{ padding: 4px 0; }}
  .date {{ text-align: center; padding-top: 20px; font-size: 1.15em; }}
  .diagnosis {{ margin-top: 20px; padding: 5px; font-size: 12px; white-space: pre-wrap; }}
  @media print {{ body {{ margin: 0; }} }}
</style>
</head>
<body onload="window.print()">
<div class="profile">
<table>
<tr><th>{name_label}</th><td>{name}</td></tr>
<tr><th>{birth_date_label}</th><td>{birth_date}</td></tr>
<tr><th>{address_label}</th><td>{address}</td></tr>
<tr><th>{last_visit_label}</th><td>{last_visit}</td></tr>
</table>
</div>
<h5 class="date">{visit_date}</h5>
<div class="diagnosis">{diagnosis}</div>
</body>
</html>
"#,
        lang = language().code(),
        name_label = t("Nama", "Name"),
        birth_date_label = t("Tanggal Lahir", "Date of Birth"),
        address_label = t("Alamat", "Address"),
        last_visit_label = t("Kunjungan Terakhir", "Last Visit"),
        title = escape_html(&format!("{} - {}", patient.name, visit_date)),
        birth_date = escape_html(&display_date(&patient.birth_date)),
        address = escape_html(&patient.address).replace('\n', "<br>"),
        last_visit = escape_html(&display_date(&patient.last_visit)),
        visit_date = escape_html(&visit_date),
        diagnosis = escape_html(&plain_text(&visit.diagnosis, visit.format)),
    )
}

/// How long a printable page is kept: long enough for the browser to load
/// it, after which the copy of the record is removed.
const PRINT_PAGE_LIFETIME: Duration = Duration::from_secs(10 * 60);

/// The folder of the printable pages: the user's own cache folder, not the
/// shared temporary one, since the pages hold medical records.
fn print_dir() -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("kutilang")
        .join("print")
}

/// Writes the printable page of a visit, and removes the old ones.
pub fn write_visit_page(patient: &Patient, visit: &Visit) -> Result<PathBuf> {
    let dir = print_dir();
    std::fs::create_dir_all(&dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700))?;
    }
    remove_old_pages(&dir, PRINT_PAGE_LIFETIME);
    let path = dir.join(format!("visit-{}.html", visit.id));
    std::fs::write(&path, visit_page(patient, visit))?;
    Ok(path)
}

/// Removes the printable pages left from earlier, including those of
/// earlier versions, which wrote them to the temporary folder.
pub fn remove_old_print_pages() {
    remove_old_pages(&print_dir(), PRINT_PAGE_LIFETIME);
    remove_old_pages(&std::env::temp_dir().join("kutilang-print"), Duration::ZERO);
}

/// Removes the `visit-*.html` files of `dir` older than `age`.
fn remove_old_pages(dir: &Path, age: Duration) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let now = SystemTime::now();
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !(name.starts_with("visit-") && name.ends_with(".html")) {
            continue;
        }
        let old = entry
            .metadata()
            .and_then(|m| m.modified())
            .is_ok_and(|modified| now.duration_since(modified).unwrap_or_default() >= age);
        if old {
            std::fs::remove_file(entry.path()).ok();
        }
    }
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
mod print_tests {
    use super::*;
    use crate::DiagnosisFormat;

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
            diagnosis: "KU: baik <3\n- TD: 120/80".into(),
            format: DiagnosisFormat::Text,
        };
        let page = visit_page(&patient(), &visit);
        assert!(page.contains("Siti &lt;Aminah&gt; / Joko"));
        assert!(page.contains("5 Januari 1990"));
        assert!(page.contains("<h5 class=\"date\">7 November 2026</h5>"));
        assert!(page.contains("KU: baik &lt;3\n- TD: 120/80"));
        assert!(page.contains("window.print()"));
    }

    #[test]
    fn old_visits_print_as_text() {
        let visit = Visit {
            id: "v".into(),
            patient_id: "p".into(),
            date: "2021-08-23".into(),
            diagnosis: "<p><strong>KU</strong>: baik</p>".into(),
            format: DiagnosisFormat::Html,
        };
        let page = visit_page(&patient(), &visit);
        assert!(page.contains(">KU: baik</div>"), "{page}");
        assert!(!page.contains("<strong>"));
    }

    #[test]
    fn old_pages_are_removed() {
        let dir = tempfile::tempdir().unwrap();
        let page = dir.path().join("visit-v.html");
        let other = dir.path().join("notes.html");
        std::fs::write(&page, "x").unwrap();
        std::fs::write(&other, "x").unwrap();
        remove_old_pages(dir.path(), Duration::from_secs(600));
        assert!(page.exists(), "a recent page is kept for the browser");
        remove_old_pages(dir.path(), Duration::ZERO);
        assert!(!page.exists());
        assert!(other.exists(), "only printable pages are removed");
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
