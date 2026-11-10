//! Checks against a real database, which is not part of the repository.
//!
//! ```sh
//! KUTILANG_REAL_DB=/path/to/copy/of/db.sqlite3 cargo test --test real_data -- --ignored
//! ```

use html5ever::tendril::TendrilSink as _;
use html5ever::{ParseOpts, parse_document};
use kutilangv2::diagnosis::{html_to_markdown, with_line_breaks};
use markup5ever_rcdom::{Handle, NodeData, RcDom};
use rusqlite::{Connection, OpenFlags};

/// Non-whitespace text of an HTML fragment, and the part of it that is bold.
fn text_and_bold(html: &str) -> (String, String) {
    fn walk(node: &Handle, bold: bool, text: &mut String, bold_text: &mut String) {
        match &node.data {
            NodeData::Text { contents } => {
                for c in contents.borrow().chars().filter(|c| !c.is_whitespace()) {
                    text.push(c);
                    if bold {
                        bold_text.push(c);
                    }
                }
            }
            NodeData::Element { name, .. } => {
                let tag = name.local.as_ref();
                if tag == "script" || tag == "style" {
                    return;
                }
                let bold = bold || tag == "strong" || tag == "b";
                for child in node.children.borrow().iter() {
                    walk(child, bold, text, bold_text);
                }
            }
            _ => {
                for child in node.children.borrow().iter() {
                    walk(child, bold, text, bold_text);
                }
            }
        }
    }
    let dom = parse_document(RcDom::default(), ParseOpts::default()).one(html);
    let (mut text, mut bold) = (String::new(), String::new());
    walk(&dom.document, false, &mut text, &mut bold);
    (text, bold)
}

#[test]
#[ignore = "needs KUTILANG_REAL_DB"]
fn every_old_entry_converts_without_losing_text_or_bold() {
    let path = std::env::var("KUTILANG_REAL_DB").expect("KUTILANG_REAL_DB");
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    let migrated: bool = conn
        .query_row(
            "SELECT count(*) > 0 FROM pragma_table_info('visits') WHERE name = 'diagnosis_format'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let sql = if migrated {
        "SELECT id, diagnosis FROM visits WHERE diagnosis_format = 'html'"
    } else {
        "SELECT id, diagnosis FROM visits"
    };
    let mut stmt = conn.prepare(sql).unwrap();
    let rows: Vec<(String, String)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert!(!rows.is_empty());

    let mut failures = Vec::new();
    for (id, html) in &rows {
        let markdown = html_to_markdown(html);
        let rendered =
            markdown::to_html_with_options(&with_line_breaks(&markdown), &markdown::Options::gfm())
                .unwrap();
        let (text, bold) = text_and_bold(html);
        let (new_text, new_bold) = text_and_bold(&rendered);
        if text != new_text || bold != new_bold {
            failures.push(id.clone());
        }
    }
    println!("checked {} entries, {} differ", rows.len(), failures.len());
    assert!(
        failures.is_empty(),
        "entries that differ: {:?}",
        &failures[..failures.len().min(20)]
    );
}
