//! Converts the HTML written by the original app's rich text editor
//! (draft.js) into Markdown, so an old record can be edited.
//!
//! It covers what that editor produced: paragraphs, `h1`-`h6`, bulleted and
//! numbered lists (also nested), bold, italic, links, line breaks, and
//! `<span>`s carrying pasted styles. Colours, fonts and underline have no
//! Markdown equivalent and are dropped; the text is kept.

use html5ever::tendril::TendrilSink as _;
use html5ever::{ParseOpts, parse_document};
use markup5ever_rcdom::{Handle, NodeData, RcDom};

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
            plain_text(node, &mut text);
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

fn plain_text(node: &Handle, out: &mut String) {
    match &node.data {
        NodeData::Text { contents } => out.push_str(&contents.borrow()),
        NodeData::Element { name, .. } if name.local.as_ref() == "br" => out.push('\n'),
        _ => {
            for child in node.children.borrow().iter() {
                plain_text(child, out);
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
        plain_text(node, &mut text);
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
mod tests {
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
