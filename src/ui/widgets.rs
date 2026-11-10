//! Small presentational helpers shared by the views.

use gpui_kit::component::{ActiveTheme as _, Icon, h_flex, v_flex};
use gpui_kit::prelude::*;
use gpui_kit::{AnyElement, App, Div, FontWeight, SharedString, div, px};

/// `2437` -> `2.437`, the Indonesian thousands separator.
pub fn format_count(count: usize) -> String {
    let digits = count.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push('.');
        }
        out.push(c);
    }
    out
}

/// Muted secondary text.
pub fn muted(text: impl Into<SharedString>, cx: &App) -> Div {
    div().text_color(cx.theme().muted_foreground).child(text.into())
}

/// The header row above a list: a title, a count and actions.
pub fn pane_header(title: &'static str, count: Option<usize>, cx: &App) -> Div {
    h_flex()
        .w_full()
        .flex_none()
        .h(px(44.))
        .px_3()
        .gap_2()
        .border_b_1()
        .border_color(cx.theme().border)
        .child(div().font_weight(FontWeight::SEMIBOLD).child(title))
        .children(count.map(|count| {
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(format_count(count))
        }))
        .child(div().flex_1())
}

/// A centered placeholder for an empty pane.
pub fn empty_state(
    icon: impl Into<Icon>,
    title: impl Into<SharedString>,
    hint: impl Into<SharedString>,
    cx: &App,
) -> AnyElement {
    v_flex()
        .size_full()
        .items_center()
        .justify_center()
        .gap_2()
        .text_color(cx.theme().muted_foreground)
        .child(icon.into().size(px(40.)).text_color(cx.theme().muted_foreground.opacity(0.5)))
        .child(div().font_weight(FontWeight::MEDIUM).child(title.into()))
        .child(div().text_sm().child(hint.into()))
        .into_any_element()
}

/// A validation message under a form field.
pub fn field_error(message: Option<&String>, cx: &App) -> Option<AnyElement> {
    message.map(|message| {
        div()
            .mt_1()
            .text_sm()
            .text_color(cx.theme().danger)
            .child(message.clone())
            .into_any_element()
    })
}

#[cfg(test)]
mod tests {
    use super::format_count;

    #[test]
    fn thousands_separators() {
        assert_eq!(format_count(0), "0");
        assert_eq!(format_count(999), "999");
        assert_eq!(format_count(2437), "2.437");
        assert_eq!(format_count(1234567), "1.234.567");
    }
}
