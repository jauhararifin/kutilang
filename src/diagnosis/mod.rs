//! Diagnosis text: the entry template, display preparation and conversions.
//!
//! New entries are written in Markdown. Entries of the original application
//! stay in their HTML until their text is edited.

mod html_to_markdown;

pub use html_to_markdown::html_to_markdown;

use crate::model::DiagnosisFormat;

/// The template of a new entry, the Markdown version of the original
/// application's template.
///
/// Built line by line so the trailing spaces after `**KU**:` and `1.` are
/// explicit: typing right after `1.` must continue the numbered list, which
/// needs the space.
pub const TEMPLATE: &str = concat!(
    "##### Subjektif\n",
    "\n",
    "**KU**: \n",
    "\n",
    "**RPS**: \n",
    "\n",
    "**RPD**: Maag (-), HT (-), DM (-), Asma (-), Tipes (-), Alergi obat (-), Sakit jantung (-), minum jamu / obat bebas (-)\n",
    "\n",
    "##### Objektif\n",
    "\n",
    "- **KU**: Baik, CM. **TD**: / mmHg, **HR**: x/mnt reguler, **BB**: kg, **TB**: cm.\n",
    "- **Konj** anemis (-), **Sclera** Icterus (-). JVP R-2 cm, **Kel. tiroid**: dbn\n",
    "- **Thorax**: SSD, SF: ka = ki. **Pulmo**: SD vesikuler, Ronchi: -, Wheezing: -.\n",
    "- **Cor**: konfigurasi dbn, S1-S2 murni, bising (-), gallop (-).\n",
    "- **Abd**: BU (+), venectasi (-), asites (-), **Hepar**: ttb, **Lien**: ttb. **Nyeri ketok CF**: ka (-) ki (-)\n",
    "- **Extr**: edema -/-, Ulcus -/-, tremor (-), Palmar eritem (-). jari tabuh (-). ptechiae (-).\n",
    "- **GDS**: mg/ml, **AU**: mg/ml, **Kol. Total**: mg/ml, **EKG**: \n",
    "\n",
    "##### Assesment\n",
    "\n",
    "1. \n",
    "\n",
    "##### Planning\n",
    "\n",
    "1. \n",
);

/// The Markdown shown in the editor for a diagnosis.
pub fn editable_markdown(diagnosis: &str, format: DiagnosisFormat) -> String {
    match format {
        DiagnosisFormat::Markdown => diagnosis.to_string(),
        DiagnosisFormat::Html => html_to_markdown(diagnosis),
    }
}

fn fence_marker(line: &str) -> Option<(char, usize)> {
    let trimmed = line.trim_start();
    let c = trimmed.chars().next()?;
    if c != '`' && c != '~' {
        return None;
    }
    let count = trimmed.chars().take_while(|x| *x == c).count();
    (count >= 3).then_some((c, count))
}

/// Prepares Markdown for display: every line break typed in the editor is
/// shown as a line break, the way the old editor behaved, instead of
/// Markdown's rule of joining consecutive lines into one paragraph.
pub fn with_line_breaks(markdown: &str) -> String {
    let lines: Vec<&str> = markdown.lines().collect();
    let mut out = String::with_capacity(markdown.len() + lines.len() * 2);
    let mut fence: Option<(char, usize)> = None;

    for (index, line) in lines.iter().enumerate() {
        if let Some((c, count)) = fence_marker(line) {
            match fence {
                None => fence = Some((c, count)),
                Some((open, open_count)) if open == c && count >= open_count => fence = None,
                _ => {}
            }
            out.push_str(line);
            out.push('\n');
            continue;
        }

        out.push_str(line);
        let next_has_text = lines
            .get(index + 1)
            .is_some_and(|next| !next.trim().is_empty() && fence_marker(next).is_none());
        let already_breaks = line.ends_with("  ") || line.ends_with('\\');
        if fence.is_none() && !line.trim().is_empty() && next_has_text && !already_breaks {
            out.push_str("  ");
        }
        out.push('\n');
    }
    out
}

/// The first lines of a diagnosis as plain text, for lists: section headings
/// and template labels left empty (`RPS:`) are skipped, lines are joined with
/// ` · ` and the result is cut to `max_chars`.
pub fn summary(text: &str, format: DiagnosisFormat, max_chars: usize) -> String {
    let lines = match format {
        DiagnosisFormat::Html => html_lines(text, false),
        DiagnosisFormat::Markdown => markdown_lines(text, false),
    };
    let mut out = String::new();
    for line in lines {
        let line = line.split_whitespace().collect::<Vec<_>>().join(" ");
        if line.is_empty() || is_empty_label(&line) {
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

/// The whole diagnosis as plain text, one line per paragraph, list item or
/// heading, for exporting.
pub fn plain_text(text: &str, format: DiagnosisFormat) -> String {
    let lines = match format {
        DiagnosisFormat::Html => html_lines(text, true),
        DiagnosisFormat::Markdown => markdown_lines(text, true),
    };
    lines
        .into_iter()
        .map(|line| line.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

/// `KU:` with nothing written after it.
fn is_empty_label(line: &str) -> bool {
    match line.split_once(':') {
        Some((label, rest)) => rest.trim().is_empty() && label.chars().count() <= 30,
        None => false,
    }
}

/// Text lines of HTML, with or without the headings. A small scanner is
/// enough here: the text is only read, never shown as HTML.
fn html_lines(html: &str, keep_headings: bool) -> Vec<String> {
    const BREAKS: [&str; 13] = [
        "p", "div", "li", "br", "tr", "ul", "ol", "h1", "h2", "h3", "h4", "h5", "h6",
    ];
    let mut lines = Vec::new();
    let mut current = String::new();
    let mut heading = false;
    let mut rest = html;

    let flush = |current: &mut String, heading: bool, lines: &mut Vec<String>| {
        if !heading || keep_headings {
            lines.push(decode_entities(current));
        }
        current.clear();
    };

    while let Some(start) = rest.find('<') {
        current.push_str(&rest[..start]);
        let Some(end) = rest[start..].find('>') else {
            rest = &rest[start..];
            break;
        };
        let tag = &rest[start + 1..start + end];
        rest = &rest[start + end + 1..];

        let closing = tag.starts_with('/');
        let name = tag
            .trim_start_matches('/')
            .split(|c: char| c.is_whitespace() || c == '/')
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase();
        if BREAKS.contains(&name.as_str()) {
            flush(&mut current, heading, &mut lines);
            if name.len() == 2 && name.starts_with('h') && name != "hr" {
                heading = !closing;
            }
        }
    }
    current.push_str(rest);
    flush(&mut current, heading, &mut lines);
    lines
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
                "nbsp" => Some(' '),
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" | "#39" => Some('\''),
                _ => entity
                    .strip_prefix('#')
                    .and_then(|n| n.parse::<u32>().ok())
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

/// Text lines of Markdown, with or without the headings, without list
/// markers or emphasis.
fn markdown_lines(markdown: &str, keep_headings: bool) -> Vec<String> {
    markdown
        .lines()
        .map(str::trim)
        .filter(|line| keep_headings || !line.starts_with('#'))
        .map(|line| line.trim_start_matches('#').trim_start())
        .map(|line| {
            let line = line
                .strip_prefix("- ")
                .or_else(|| line.strip_prefix("* "))
                .or_else(|| line.strip_prefix("+ "))
                .unwrap_or_else(|| {
                    let digits = line.chars().take_while(char::is_ascii_digit).count();
                    match line[digits..].strip_prefix(['.', ')']) {
                        Some(rest) if digits > 0 && (rest.is_empty() || rest.starts_with(' ')) => {
                            rest
                        }
                        _ => line,
                    }
                });
            let mut text = String::with_capacity(line.len());
            let mut chars = line.chars().peekable();
            while let Some(c) = chars.next() {
                match c {
                    '\\' => text.extend(chars.next()),
                    '*' => {}
                    _ => text.push(c),
                }
            }
            text
        })
        .collect()
}

/// The diagnosis as HTML, for printing.
pub fn to_html(diagnosis: &str, format: DiagnosisFormat) -> String {
    match format {
        // Exactly what the original application printed.
        DiagnosisFormat::Html => diagnosis.to_string(),
        DiagnosisFormat::Markdown => {
            // Raw HTML in the Markdown is escaped, not interpreted.
            markdown::to_html_with_options(&with_line_breaks(diagnosis), &markdown::Options::gfm())
                .unwrap_or_else(|_| format!("<pre>{}</pre>", crate::print::escape_html(diagnosis)))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_breaks_become_hard_breaks() {
        assert_eq!(
            with_line_breaks("**KU**: lemas\n**RPS**: demam\n\n- a\n- b"),
            "**KU**: lemas  \n**RPS**: demam\n\n- a  \n- b\n"
        );
    }

    #[test]
    fn code_fences_are_left_alone() {
        assert_eq!(
            with_line_breaks("a\n```\nx\ny\n```\nb"),
            "a\n```\nx\ny\n```\nb\n"
        );
    }

    #[test]
    fn markdown_prints_with_breaks_and_escaped_html() {
        let html = to_html("**KU**: lemas\nRPS: <b>x</b>", DiagnosisFormat::Markdown);
        assert!(html.contains("<strong>KU</strong>: lemas<br />"), "{html}");
        assert!(html.contains("&lt;b&gt;x&lt;/b&gt;"), "{html}");
    }

    #[test]
    fn summaries_skip_headings_and_empty_labels() {
        let html = "<h5>Subjektif</h5>\n<p><strong>KU</strong>: meriang&nbsp;</p>\
                    <p><strong>RPS</strong>:</p><ul><li><strong>TD</strong>: 120/80 &amp; HR 80</li></ul>";
        assert_eq!(
            summary(html, DiagnosisFormat::Html, 160),
            "KU: meriang · TD: 120/80 & HR 80"
        );
        let markdown =
            TEMPLATE
                .replace("**KU**: \n", "**KU**: lemas\n")
                .replacen("1. \n", "1. ISPA\n", 1);
        let text = summary(&markdown, DiagnosisFormat::Markdown, 2000);
        assert!(text.starts_with("KU: lemas · RPD: Maag (-)"), "{text}");
        assert!(text.ends_with("ISPA"), "{text}");
        assert!(!text.contains("Subjektif") && !text.contains("RPS") && !text.contains("**"));
        assert_eq!(
            summary(TEMPLATE, DiagnosisFormat::Markdown, 20)
                .chars()
                .count(),
            20
        );
        assert!(summary(TEMPLATE, DiagnosisFormat::Markdown, 20).ends_with('…'));
        assert_eq!(summary("", DiagnosisFormat::Html, 20), "");
    }

    #[test]
    fn plain_text_keeps_every_line() {
        let html = "<h5>Subjektif</h5><p><strong>KU</strong>: meriang&nbsp;</p><ul><li>TD: 120/80</li></ul>";
        assert_eq!(
            plain_text(html, DiagnosisFormat::Html),
            "Subjektif\nKU: meriang\nTD: 120/80"
        );
        let markdown = "##### Subjektif\n\n**KU**: lemas\n\n- **TD**: 120/80\n1. ISPA";
        assert_eq!(
            plain_text(markdown, DiagnosisFormat::Markdown),
            "Subjektif\nKU: lemas\nTD: 120/80\nISPA"
        );
    }

    #[test]
    fn template_leaves_room_to_type_after_labels_and_numbers() {
        assert!(TEMPLATE.contains("**KU**: \n"));
        assert_eq!(TEMPLATE.matches("\n1. \n").count(), 2);
        // Typing after "1. " continues the numbered list.
        let filled = TEMPLATE.replacen("1. \n", "1. ISPA\n", 1);
        assert!(to_html(&filled, DiagnosisFormat::Markdown).contains("<li>ISPA</li>"));
    }

    #[test]
    fn template_renders_its_sections() {
        let html = to_html(TEMPLATE, DiagnosisFormat::Markdown);
        for section in ["Subjektif", "Objektif", "Assesment", "Planning"] {
            assert!(
                html.contains(&format!("<h5>{section}</h5>")),
                "{section}: {html}"
            );
        }
        assert!(html.contains("<strong>Nyeri ketok CF</strong>: ka (-) ki (-)"));
        assert_eq!(html.matches("<li>").count(), 9, "{html}");
    }

    #[test]
    fn html_entries_are_converted_for_editing() {
        assert_eq!(
            editable_markdown("<p><strong>KU</strong>: baik</p>", DiagnosisFormat::Html),
            "**KU**: baik"
        );
        assert_eq!(
            editable_markdown("**x**", DiagnosisFormat::Markdown),
            "**x**"
        );
    }
}
