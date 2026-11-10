//! The editor of a visit entry: a date and a Markdown diagnosis.

use chrono::NaiveDate;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::calendar::Date;
use gpui_kit::component::date_picker::{DatePicker, DatePickerState};
use gpui_kit::component::input::{Textarea, TextareaState};
use gpui_kit::component::notification::Notification;
use gpui_kit::component::text::TextView;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Selectable as _, Sizable as _, WindowExt as _, h_flex,
    v_flex,
};
use gpui_kit::prelude::*;
use gpui_kit::{Context, Entity, EventEmitter, SharedString, Window, div, px};

use crate::dates;
use crate::diagnosis;
use crate::model::{DiagnosisFormat, Visit};
use crate::ui::widgets;

pub enum VisitEditorEvent {
    /// `diagnosis` is `None` when an existing entry's text was left as it was.
    Submit {
        date: NaiveDate,
        diagnosis: Option<String>,
    },
    Cancel,
}

pub struct VisitEditor {
    id: SharedString,
    date: Entity<DatePickerState>,
    text: Entity<TextareaState>,
    /// The text the editor started with.
    initial_text: String,
    /// Editing an existing visit (as opposed to writing a new one).
    existing: bool,
    /// The visit was written with the original application.
    from_html: bool,
    preview: bool,
}

impl EventEmitter<VisitEditorEvent> for VisitEditor {}

impl VisitEditor {
    /// An editor for a new entry, starting from `template`.
    pub fn new_entry(template: String, window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self::build("new-entry".into(), dates::today(), template, false, false, window, cx)
    }

    /// An editor for an existing visit.
    pub fn edit(visit: &Visit, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let date = dates::parse_stored(&visit.date).unwrap_or_else(dates::today);
        let text = diagnosis::editable_markdown(&visit.diagnosis, visit.format);
        let from_html = visit.format == DiagnosisFormat::Html;
        Self::build(format!("edit-{}", visit.id).into(), date, text, true, from_html, window, cx)
    }

    fn build(
        id: SharedString,
        date: NaiveDate,
        text: String,
        existing: bool,
        from_html: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let date_state = cx.new(|cx| {
            let mut state = DatePickerState::new(window, cx).date_format("%Y-%m-%d");
            state.set_date(date, window, cx);
            state
        });
        let text_state = cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(10, 60)
                .default_value(text.clone())
        });
        Self {
            id,
            date: date_state,
            text: text_state,
            initial_text: text,
            existing,
            from_html,
            preview: false,
        }
    }

    pub fn focus(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.text.update(cx, |state, cx| state.focus(window, cx));
    }

    /// The text being edited.
    pub fn content(&self, cx: &gpui_kit::App) -> SharedString {
        self.text.read(cx).value()
    }

    fn text(&self, cx: &Context<Self>) -> String {
        self.text.read(cx).value().to_string()
    }

    pub fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Date::Single(Some(date)) = self.date.read(cx).date() else {
            window.push_notification(Notification::error("Please choose the date of the visit"), cx);
            return;
        };
        let text = self.text(cx);
        if text.trim().is_empty() {
            window.push_notification(Notification::error("Missing diagnosis"), cx);
            return;
        }
        let diagnosis = (!self.existing || text != self.initial_text).then_some(text);
        cx.emit(VisitEditorEvent::Submit { date, diagnosis });
    }

    /// Wraps the selected text in `**`.
    fn bold(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.text.update(cx, |state, cx| {
            let selected = state.selected_value();
            let trimmed = selected.trim();
            if !trimmed.is_empty() {
                // Keep surrounding spaces outside of the markers.
                let leading = &selected[..selected.len() - selected.trim_start().len()];
                let trailing = &selected[selected.trim_end().len()..];
                state.replace(format!("{leading}**{trimmed}**{trailing}"), window, cx);
            }
            state.focus(window, cx);
        });
    }

    pub fn toggle_preview(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.preview = !self.preview;
        if !self.preview {
            self.text.update(cx, |state, cx| state.focus(window, cx));
        }
        cx.notify();
    }
}

impl Render for VisitEditor {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let body = if self.preview {
            let markdown = diagnosis::with_line_breaks(&self.text(cx));
            div()
                .w_full()
                .min_h(px(200.))
                .px_3()
                .py_2()
                .border_1()
                .border_color(cx.theme().border)
                .rounded(cx.theme().radius)
                .child(TextView::markdown(
                    SharedString::from(format!("{}-preview", self.id)),
                    markdown,
                ))
                .into_any_element()
        } else {
            Textarea::new(&self.text).into_any_element()
        };

        let toolbar = h_flex()
            .w_full()
            .gap_1()
            .child(div().w(px(170.)).child(DatePicker::new(&self.date).small()))
            .child(div().flex_1())
            .child(
                Button::new("bold")
                    .icon(IconName::Bold)
                    .ghost()
                    .small()
                    .tooltip("Bold (wraps the selection in **)")
                    .disabled(self.preview)
                    .on_click(cx.listener(|this, _, window, cx| this.bold(window, cx))),
            )
            .child(
                Button::new("preview")
                    .icon(IconName::Eye)
                    .label("Preview")
                    .ghost()
                    .small()
                    .selected(self.preview)
                    .on_click(cx.listener(|this, _, window, cx| this.toggle_preview(window, cx))),
            );

        let note = self.from_html.then(|| {
            widgets::muted(
                "Written with the old editor and shown here converted. \
                 The original is kept unless you change the text.",
                cx,
            )
            .text_sm()
        });

        v_flex()
            .w_full()
            .gap_2()
            .child(toolbar)
            .children(note)
            .child(body)
            .child(
                h_flex()
                    .w_full()
                    .justify_end()
                    .gap_2()
                    .child(
                        Button::new("visit-cancel")
                            .ghost()
                            .small()
                            .label("Cancel")
                            .on_click(cx.listener(|_, _, _, cx| cx.emit(VisitEditorEvent::Cancel))),
                    )
                    .child(
                        Button::new("visit-save")
                            .primary()
                            .small()
                            .label("Save")
                            .on_click(cx.listener(|this, _, window, cx| this.submit(window, cx))),
                    ),
            )
    }
}
