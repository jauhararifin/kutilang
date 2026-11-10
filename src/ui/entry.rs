//! How a visit entry is displayed.

use gpui_kit::component::text::TextView;
use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::prelude::*;
use gpui_kit::{AnyElement, App, Div, FontWeight, SharedString, div};

use crate::dates;
use crate::diagnosis;
use crate::model::{DiagnosisFormat, Visit};

/// A visit as a journal entry: its date, actions, and the rendered text.
/// `scope` keeps the rendered text of the same visit in two views apart.
pub fn entry_view(
    scope: &str,
    visit: &Visit,
    actions: impl IntoIterator<Item = AnyElement>,
    cx: &App,
) -> Div {
    let today = dates::today();
    let (title, relative) = match dates::parse_stored(&visit.date) {
        Some(date) => (
            dates::format_with_weekday(date),
            dates::format_relative(date, today),
        ),
        None => (visit.date.clone(), String::new()),
    };
    let text_id = SharedString::from(format!("{scope}-visit-{}", visit.id));
    let body = match visit.format {
        DiagnosisFormat::Html => TextView::html(text_id, visit.diagnosis.clone()),
        DiagnosisFormat::Markdown => {
            TextView::markdown(text_id, diagnosis::with_line_breaks(&visit.diagnosis))
        }
    };

    v_flex()
        .w_full()
        .gap_2()
        .child(
            h_flex()
                .w_full()
                .gap_2()
                .child(div().font_weight(FontWeight::SEMIBOLD).child(title))
                .child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(relative),
                )
                .child(div().flex_1())
                .children(actions),
        )
        .child(div().w_full().child(body))
}
