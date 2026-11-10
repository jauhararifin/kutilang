//! Diagnosis text: the visit template, display preparation, the conversion of
//! the original application's HTML to Markdown, and printing.
//!
//! New visits are written in Markdown. Visits of the original application
//! stay in their HTML until their text is edited.

use crate::{DiagnosisFormat, Patient, Visit, display_date};
use anyhow::Result;
use html5ever::tendril::TendrilSink as _;
use html5ever::{ParseOpts, parse_document};
use markup5ever_rcdom::{Handle, NodeData, RcDom};
use std::path::{Path, PathBuf};

// ============================================================================
// The visit template, and preparing a diagnosis for display.
// ============================================================================

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
                .unwrap_or_else(|_| format!("<pre>{}</pre>", escape_html(diagnosis)))
        }
    }
}

#[cfg(test)]
mod diagnosis_tests {
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

// ============================================================================
// Converts the HTML written by the original app's rich text editor
// (draft.js) into Markdown, so an old record can be edited.
//
// It covers what that editor produced: paragraphs, `h1`-`h6`, bulleted and
// numbered lists (also nested), bold, italic, links, line breaks, and
// `<span>`s carrying pasted styles. Colours, fonts and underline have no
// Markdown equivalent and are dropped; the text is kept.
// ============================================================================

#[derive(Debug)]
enum Block {
    Paragraph(String),
    Heading(usize, String),
    List {
        ordered: bool,
        items: Vec<Vec<Block>>,
    },
    Quote(Vec<Block>),
    Code(String),
    Rule,
}

pub fn html_to_markdown(html: &str) -> String {
    let dom = parse_document(RcDom::default(), ParseOpts::default()).one(html);
    let blocks = find_element(&dom.document, "body")
        .map(|body| collect_blocks(&body))
        .unwrap_or_default();

    let mut lines = Vec::new();
    render_blocks(&blocks, false, &mut lines);
    let mut markdown = lines.join("\n");
    while markdown.ends_with('\n') {
        markdown.pop();
    }
    markdown
}

fn element_name(node: &Handle) -> Option<String> {
    match &node.data {
        NodeData::Element { name, .. } => Some(name.local.to_string()),
        _ => None,
    }
}

fn attribute(node: &Handle, attr: &str) -> Option<String> {
    match &node.data {
        NodeData::Element { attrs, .. } => attrs
            .borrow()
            .iter()
            .find(|a| a.name.local.as_ref() == attr)
            .map(|a| a.value.to_string()),
        _ => None,
    }
}

fn find_element(node: &Handle, name: &str) -> Option<Handle> {
    if element_name(node).as_deref() == Some(name) {
        return Some(node.clone());
    }
    node.children
        .borrow()
        .iter()
        .find_map(|child| find_element(child, name))
}

fn is_block_element(name: &str) -> bool {
    matches!(
        name,
        "p" | "div"
            | "section"
            | "article"
            | "header"
            | "footer"
            | "main"
            | "center"
            | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "ul"
            | "ol"
            | "li"
            | "blockquote"
            | "pre"
            | "hr"
            | "table"
            | "thead"
            | "tbody"
            | "tfoot"
            | "tr"
            | "td"
            | "th"
            | "figure"
            | "figcaption"
            | "address"
    )
}

/// Collects the blocks inside a container. Inline content found directly in
/// the container becomes a paragraph.
fn collect_blocks(container: &Handle) -> Vec<Block> {
    let mut blocks = Vec::new();
    let mut inline = Inline::default();

    for child in container.children.borrow().iter() {
        let name = element_name(child);
        match name.as_deref() {
            Some(name) if is_block_element(name) => {
                flush_paragraph(&mut inline, &mut blocks);
                block_element(name, child, &mut blocks);
            }
            Some("script" | "style" | "head" | "title") => {}
            _ => inline.node(child),
        }
    }
    flush_paragraph(&mut inline, &mut blocks);
    blocks
}

fn flush_paragraph(inline: &mut Inline, blocks: &mut Vec<Block>) {
    let text = std::mem::take(inline).finish();
    if !text.is_empty() {
        blocks.push(Block::Paragraph(text));
    }
}

fn inline_content(node: &Handle) -> String {
    let mut inline = Inline::default();
    inline.children(node);
    inline.finish()
}

fn block_element(name: &str, node: &Handle, blocks: &mut Vec<Block>) {
    match name {
        "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
            let text = inline_content(node).replace('\n', " ");
            if !text.is_empty() {
                let level = name[1..].parse().unwrap_or(5);
                blocks.push(Block::Heading(level, text));
            }
        }
        "ul" | "ol" => {
            let list = list_block(node, name == "ol");
            if let Block::List { items, .. } = &list
                && !items.is_empty()
            {
                blocks.push(list);
            }
        }
        // A list item outside of a list.
        "li" => blocks.push(Block::List {
            ordered: false,
            items: vec![collect_blocks(node)],
        }),
        "blockquote" => {
            let inner = collect_blocks(node);
            if !inner.is_empty() {
                blocks.push(Block::Quote(inner));
            }
        }
        "pre" => {
            let mut text = String::new();
            node_text(node, &mut text);
            if !text.trim().is_empty() {
                blocks.push(Block::Code(text.trim_end().to_string()));
            }
        }
        "hr" => blocks.push(Block::Rule),
        // Containers, including table parts: keep their content.
        _ => blocks.extend(collect_blocks(node)),
    }
}

fn list_block(node: &Handle, ordered: bool) -> Block {
    let mut items: Vec<Vec<Block>> = Vec::new();
    for child in node.children.borrow().iter() {
        match element_name(child).as_deref() {
            Some("li") => items.push(collect_blocks(child)),
            // draft.js wrote nested lists directly inside the parent list.
            Some(name @ ("ul" | "ol")) => {
                let nested = list_block(child, name == "ol");
                match items.last_mut() {
                    Some(item) => item.push(nested),
                    None => items.push(vec![nested]),
                }
            }
            _ => {
                let mut inline = Inline::default();
                inline.node(child);
                let text = inline.finish();
                if !text.is_empty() {
                    items.push(vec![Block::Paragraph(text)]);
                }
            }
        }
    }
    Block::List { ordered, items }
}

fn node_text(node: &Handle, out: &mut String) {
    match &node.data {
        NodeData::Text { contents } => out.push_str(&contents.borrow()),
        NodeData::Element { name, .. } if name.local.as_ref() == "br" => out.push('\n'),
        _ => {
            for child in node.children.borrow().iter() {
                node_text(child, out);
            }
        }
    }
}

/// Renders blocks separated by blank lines. Inside a list item, a nested list
/// directly follows the item's text so the list stays tight.
fn render_blocks(blocks: &[Block], in_list_item: bool, lines: &mut Vec<String>) {
    for (index, block) in blocks.iter().enumerate() {
        let nested_list = in_list_item && matches!(block, Block::List { .. });
        if index > 0 && !nested_list {
            lines.push(String::new());
        }
        match block {
            Block::Paragraph(text) => {
                for line in text.split('\n') {
                    lines.push(escape_line_start(line.trim()));
                }
            }
            Block::Heading(level, text) => {
                lines.push(format!("{} {}", "#".repeat((*level).clamp(1, 6)), text));
            }
            Block::List { ordered, items } => {
                for (number, item) in items.iter().enumerate() {
                    let marker = if *ordered {
                        format!("{}. ", number + 1)
                    } else {
                        "- ".to_string()
                    };
                    let indent = " ".repeat(marker.len());
                    let mut item_lines = Vec::new();
                    render_blocks(item, true, &mut item_lines);
                    if item_lines.is_empty() {
                        lines.push(marker.trim_end().to_string());
                        continue;
                    }
                    for (i, line) in item_lines.into_iter().enumerate() {
                        if i == 0 {
                            lines.push(format!("{marker}{line}"));
                        } else if line.is_empty() {
                            lines.push(line);
                        } else {
                            lines.push(format!("{indent}{line}"));
                        }
                    }
                }
            }
            Block::Quote(inner) => {
                let mut inner_lines = Vec::new();
                render_blocks(inner, false, &mut inner_lines);
                for line in inner_lines {
                    lines.push(if line.is_empty() {
                        ">".into()
                    } else {
                        format!("> {line}")
                    });
                }
            }
            Block::Code(code) => {
                lines.push("```".into());
                lines.extend(code.lines().map(str::to_string));
                lines.push("```".into());
            }
            Block::Rule => lines.push("---".into()),
        }
    }
}

/// Escapes what would make a line start a heading, quote, list or setext
/// underline instead of being plain text.
fn escape_line_start(line: &str) -> String {
    let mut chars = line.chars();
    let Some(first) = chars.next() else {
        return String::new();
    };
    let rest = chars.as_str();
    let space_or_end = |s: &str| s.is_empty() || s.starts_with(' ');

    if first == '#' || first == '>' || first == '=' {
        return format!("\\{line}");
    }
    if (first == '-' || first == '+') && (space_or_end(rest) || rest.chars().all(|c| c == first)) {
        return format!("\\{line}");
    }
    let digits = line.chars().take_while(char::is_ascii_digit).count();
    if digits > 0 && digits <= 9 {
        let after = &line[digits..];
        if let Some(stripped) = after.strip_prefix(['.', ')'])
            && space_or_end(stripped)
        {
            return format!("{}\\{}", &line[..digits], after);
        }
    }
    line.to_string()
}

/// Whether the characters after an `&` would form an HTML entity reference
/// (`amp;`, `#160;`, `#xA0;`), which Markdown would decode.
fn starts_entity(rest: &[char]) -> bool {
    let Some(end) = rest.iter().take(33).position(|c| *c == ';') else {
        return false;
    };
    if end == 0 {
        return false;
    }
    let body: String = rest[..end].iter().collect();
    if let Some(number) = body.strip_prefix('#') {
        let digits = number.strip_prefix(['x', 'X']).unwrap_or(number);
        return !digits.is_empty() && digits.chars().all(|c| c.is_ascii_hexdigit());
    }
    body.chars().all(|c| c.is_ascii_alphanumeric())
}

fn is_markdown_punctuation(c: char) -> bool {
    c.is_ascii_punctuation() || (!c.is_alphanumeric() && !c.is_whitespace())
}

/// Builds the Markdown of inline content: collapses HTML whitespace, escapes
/// Markdown syntax and writes emphasis so that it parses back correctly.
#[derive(Default)]
struct Inline {
    out: String,
    pending_space: bool,
    /// Whitespace came before any content.
    leading_space: bool,
    /// Set right after a closing emphasis delimiter whose content ended with
    /// punctuation: if a letter follows directly, the delimiter would not
    /// close, so a space is inserted.
    closed_after_punctuation: bool,
    /// The delimiter just closed and where it ends, to merge `**a****b**`.
    just_closed: Option<(&'static str, usize)>,
}

impl Inline {
    fn children(&mut self, node: &Handle) {
        for child in node.children.borrow().iter() {
            self.node(child);
        }
    }

    fn node(&mut self, node: &Handle) {
        match &node.data {
            NodeData::Text { contents } => self.text(&contents.borrow()),
            NodeData::Element { name, .. } => match name.local.as_ref() {
                "br" => self.line_break(),
                "strong" | "b" => self.emphasis("**", node),
                "em" | "i" | "cite" => self.emphasis("*", node),
                "code" | "kbd" | "samp" => self.code(node),
                "a" => self.link(node),
                "img" => {
                    if let Some(alt) = attribute(node, "alt") {
                        self.text(&alt);
                    }
                }
                "script" | "style" => {}
                name if is_block_element(name) => {
                    // A block inside inline content (e.g. a list inside a
                    // heading): keep its text on its own line.
                    self.line_break();
                    self.children(node);
                    self.line_break();
                }
                _ => self.children(node),
            },
            _ => {}
        }
    }

    fn text(&mut self, text: &str) {
        let chars: Vec<char> = text.chars().collect();
        for (i, &c) in chars.iter().enumerate() {
            let c = if c == '\u{a0}' { ' ' } else { c };
            if c.is_whitespace() {
                if self.out.is_empty() {
                    self.leading_space = true;
                }
                self.pending_space = true;
                continue;
            }
            self.flush_space();
            if self.closed_after_punctuation && c.is_alphanumeric() {
                self.out.push(' ');
            }
            self.closed_after_punctuation = false;
            self.just_closed = None;

            let next = chars.get(i + 1).copied();
            match c {
                '\\' | '*' | '_' | '`' | '[' | ']' | '~' | '$' => {
                    self.out.push('\\');
                    self.out.push(c);
                }
                '<' if next.is_some_and(|n| n.is_ascii_alphabetic() || "/!?".contains(n)) => {
                    self.out.push_str("\\<");
                }
                '&' if starts_entity(&chars[i + 1..]) => {
                    self.out.push_str("\\&");
                }
                _ => self.out.push(c),
            }
        }
    }

    fn flush_space(&mut self) {
        if self.pending_space {
            if !self.out.is_empty() && !self.out.ends_with('\n') {
                self.out.push(' ');
            }
            self.pending_space = false;
            self.closed_after_punctuation = false;
            self.just_closed = None;
        }
    }

    fn line_break(&mut self) {
        self.pending_space = false;
        self.closed_after_punctuation = false;
        self.just_closed = None;
        while self.out.ends_with(' ') {
            self.out.pop();
        }
        if !self.out.is_empty() {
            self.out.push('\n');
        }
    }

    fn emphasis(&mut self, delimiter: &'static str, node: &Handle) {
        let mut inner = Inline::default();
        inner.children(node);
        let starts_with_space = inner.leading_space;
        let ends_with_space = inner.pending_space || inner.out.ends_with(' ');
        let content = inner.finish();

        if content.is_empty() {
            if starts_with_space || ends_with_space {
                self.pending_space = true;
            }
            return;
        }
        // Emphasis cannot span lines reliably; emphasise each line.
        if content.contains('\n') {
            let lines: Vec<&str> = content.split('\n').collect();
            for (i, line) in lines.iter().enumerate() {
                if i > 0 {
                    self.line_break();
                }
                self.emphasised(delimiter, line, starts_with_space && i == 0);
            }
        } else {
            self.emphasised(delimiter, &content, starts_with_space);
        }
        if ends_with_space {
            self.pending_space = true;
        }
    }

    fn emphasised(&mut self, delimiter: &'static str, content: &str, space_before: bool) {
        if content.is_empty() {
            return;
        }
        if space_before {
            self.pending_space = true;
        }

        // `**a**` directly followed by `**b**` becomes `**ab**`.
        if !self.pending_space
            && let Some((closed, end)) = self.just_closed
            && closed == delimiter
            && end == self.out.len()
        {
            self.out.truncate(end - delimiter.len());
            self.out.push_str(content);
            self.out.push_str(delimiter);
            self.finish_emphasis(delimiter, content);
            return;
        }

        self.flush_space();
        let previous = self.out.chars().last();
        let first = content.chars().next().unwrap_or(' ');
        // An opening delimiter between a letter and punctuation would not open.
        if previous.is_some_and(char::is_alphanumeric) && is_markdown_punctuation(first) {
            self.out.push(' ');
        }
        if self.closed_after_punctuation && previous.is_some_and(|p| p == '*') {
            self.out.push(' ');
        }
        self.out.push_str(delimiter);
        self.out.push_str(content);
        self.out.push_str(delimiter);
        self.finish_emphasis(delimiter, content);
    }

    fn finish_emphasis(&mut self, delimiter: &'static str, content: &str) {
        let last = content.chars().last().unwrap_or(' ');
        self.closed_after_punctuation = is_markdown_punctuation(last);
        self.just_closed = Some((delimiter, self.out.len()));
    }

    fn code(&mut self, node: &Handle) {
        let mut text = String::new();
        node_text(node, &mut text);
        let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
        if text.is_empty() {
            return;
        }
        self.flush_space();
        let fence = if text.contains('`') { "``" } else { "`" };
        self.out.push_str(&format!("{fence}{text}{fence}"));
        self.closed_after_punctuation = false;
        self.just_closed = None;
    }

    fn link(&mut self, node: &Handle) {
        let label = inline_content(node).replace('\n', " ");
        let href = attribute(node, "href").unwrap_or_default();
        let href = href.trim();
        if href.is_empty() || href.starts_with("javascript:") {
            self.text_markdown(&label);
            return;
        }
        let url = if href.contains([' ', '(', ')']) {
            format!("<{href}>")
        } else {
            href.to_string()
        };
        let label = if label.is_empty() {
            href.to_string()
        } else {
            label
        };
        self.text_markdown(&format!("[{label}]({url})"));
    }

    /// Appends already-escaped Markdown.
    fn text_markdown(&mut self, markdown: &str) {
        if markdown.is_empty() {
            return;
        }
        self.flush_space();
        self.out.push_str(markdown);
        self.closed_after_punctuation = false;
        self.just_closed = None;
    }

    fn finish(self) -> String {
        let lines: Vec<&str> = self.out.split('\n').map(str::trim).collect();
        let mut text = lines.join("\n");
        while text.ends_with('\n') {
            text.pop();
        }
        while text.starts_with('\n') {
            text.remove(0);
        }
        text
    }
}

#[cfg(test)]
mod html_to_markdown_tests {
    use super::html_to_markdown as md;

    #[test]
    fn paragraphs_and_bold_labels() {
        assert_eq!(
            md("<p><strong>KU</strong>: lemas</p><p><strong>RPS</strong>: demam 3 hari</p>"),
            "**KU**: lemas\n\n**RPS**: demam 3 hari"
        );
    }

    #[test]
    fn headings_and_lists() {
        let html = "<h5>Objektif</h5><ul><li><strong>TD</strong>: 120/80 mmHg</li>\
                    <li>Cor dbn<br></li></ul><h5>Assesment</h5><ol><li>ISPA</li><li></li></ol>";
        assert_eq!(
            md(html),
            "##### Objektif\n\n- **TD**: 120/80 mmHg\n- Cor dbn\n\n##### Assesment\n\n1. ISPA\n2."
        );
    }

    #[test]
    fn nbsp_and_whitespace_collapse() {
        assert_eq!(
            md("<p>&nbsp;TD:&nbsp;&nbsp;120/80 \n mmHg&nbsp;</p>"),
            "TD: 120/80 mmHg"
        );
        assert_eq!(md("<p></p><p>&nbsp;</p><p>x</p>"), "x");
    }

    #[test]
    fn whitespace_moves_outside_emphasis() {
        assert_eq!(md("<p><strong>KU </strong>baik</p>"), "**KU** baik");
        assert_eq!(md("<p>a<strong> b</strong></p>"), "a **b**");
    }

    #[test]
    fn emphasis_ending_in_punctuation_before_a_letter() {
        // `**CF:**ka` would not parse as bold.
        assert_eq!(
            md("<p><strong>Nyeri ketok CF:</strong>ka (-)</p>"),
            "**Nyeri ketok CF:** ka (-)"
        );
        // and an opening delimiter between a letter and punctuation.
        assert_eq!(md("<p>a<strong>(x)</strong></p>"), "a **(x)**");
    }

    #[test]
    fn adjacent_emphasis_is_merged() {
        assert_eq!(
            md("<p><strong>K</strong><strong>U</strong>:</p>"),
            "**KU**:"
        );
        assert_eq!(md("<p><strong></strong>x</p>"), "x");
    }

    #[test]
    fn breaks_inside_paragraphs() {
        assert_eq!(
            md("<p><span style=\"color: red\">a : b<br>   c , d</span>&nbsp;</p>"),
            "a : b\nc , d"
        );
    }

    #[test]
    fn markdown_syntax_in_text_is_escaped() {
        assert_eq!(
            md("<p>2*3 = 6_ok [x] ~2 hari $5</p>"),
            "2\\*3 = 6\\_ok \\[x\\] \\~2 hari \\$5"
        );
        assert_eq!(
            md("<p>&lt;br&gt; &lt; 5 a&amp;b &amp;amp;</p>"),
            "\\<br> < 5 a&b \\&amp;"
        );
        assert_eq!(
            md("<p># 1</p><p>- x</p><p>3. obat</p><p>--</p>"),
            "\\# 1\n\n\\- x\n\n3\\. obat\n\n\\--"
        );
    }

    #[test]
    fn nested_lists_from_draft_js() {
        assert_eq!(
            md("<ol><li>a</li><ol><li>b</li></ol><li>c</li></ol>"),
            "1. a\n   1. b\n2. c"
        );
        assert_eq!(md("<ul><li>a<ul><li>b</li></ul></li></ul>"), "- a\n  - b");
    }

    #[test]
    fn links_and_italic() {
        assert_eq!(
            md("<li><a href=\"https://x.id/a b\" target=\"_blank\">hasil lab</a> <em>ok</em></li>"),
            "- [hasil lab](<https://x.id/a b>) *ok*"
        );
    }

    #[test]
    fn underline_and_sub_keep_text() {
        assert_eq!(
            md("<p><em><ins>catatan</ins></em> H<sub>2</sub>O</p>"),
            "*catatan* H2O"
        );
    }

    #[test]
    fn converts_the_original_template() {
        let html = r#"
<h5>Subjektif</h5>
<p><strong>KU</strong>:</p>
<ul>
	<li><strong>GDS</strong>: mg/ml, <strong>EKG</strong>:
</ul>
<h5>Planning</h5>
<ol>
	<li></li>
</ol>
"#;
        assert_eq!(
            md(html),
            "##### Subjektif\n\n**KU**:\n\n- **GDS**: mg/ml, **EKG**:\n\n##### Planning\n\n1."
        );
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
        birth_date = escape_html(&display_date(&patient.birth_date)),
        address = escape_html(&patient.address).replace('\n', "<br>"),
        last_visit = escape_html(&display_date(&patient.last_visit)),
        visit_date = escape_html(&visit_date),
        diagnosis = to_html(&visit.diagnosis, visit.format),
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
