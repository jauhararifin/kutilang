//! The Settings section: the template of new entries, and database details.

use std::path::PathBuf;
use std::rc::Rc;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::description_list::DescriptionList;
use gpui_kit::component::group_box::{GroupBox, GroupBoxVariants as _};
use gpui_kit::component::input::{InputEvent, Textarea, TextareaState};
use gpui_kit::component::notification::Notification;
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::text::TextView;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Sizable as _, WindowExt as _, h_flex, v_flex,
};
use gpui_kit::prelude::*;
use gpui_kit::{Context, Entity, FontWeight, Subscription, Window, div, px};

use crate::diagnosis::{self, TEMPLATE};
use crate::store::{SCHEMA_VERSION, Store};
use crate::ui::widgets;

pub struct SettingsView {
    store: Rc<Store>,
    db_path: PathBuf,
    template: Entity<TextareaState>,
    /// The template as stored.
    saved: String,
    _subscriptions: Vec<Subscription>,
}

impl SettingsView {
    pub fn new(store: Rc<Store>, db_path: PathBuf, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let saved = store.diagnosis_template();
        let template = cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(16, 60)
                .default_value(saved.clone())
        });
        // Keep the preview and the buttons in step with the text.
        let subscription = cx.subscribe(&template, |_, _, event: &InputEvent, cx| {
            if let InputEvent::Change = event {
                cx.notify();
            }
        });
        Self {
            store,
            db_path,
            template,
            saved,
            _subscriptions: vec![subscription],
        }
    }

    fn text(&self, cx: &Context<Self>) -> String {
        self.template.read(cx).value().to_string()
    }

    fn set_text(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.template
            .update(cx, |state, cx| state.set_value(text.to_string(), window, cx));
        cx.notify();
    }

    pub fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.text(cx);
        match self.store.set_diagnosis_template(Some(&text)) {
            Ok(()) => {
                self.saved = text;
                window.push_notification(
                    Notification::success("New entries will start from this template."),
                    cx,
                );
            }
            Err(error) => window.push_notification(Notification::error(error.to_string()), cx),
        }
        cx.notify();
    }

    fn render_template(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let text = self.text(cx);
        let changed = text != self.saved;
        let is_default = text == TEMPLATE;
        let column_title = |title: &'static str| {
            div()
                .text_sm()
                .font_weight(FontWeight::MEDIUM)
                .text_color(cx.theme().muted_foreground)
                .child(title)
        };

        GroupBox::new()
            .fill()
            .title("New entry template")
            .child(
                v_flex()
                    .w_full()
                    .gap_3()
                    .child(widgets::muted(
                        "Every new entry starts with this text. It is written in Markdown: \
                         **bold**, \"- \" for a bullet, \"1. \" for a numbered item, \
                         \"##### \" for a heading. Entries already written do not change.",
                        cx,
                    ).text_sm())
                    .child(
                        h_flex()
                            .w_full()
                            .items_start()
                            .gap_4()
                            .child(
                                v_flex()
                                    .flex_1()
                                    .min_w(px(0.))
                                    .gap_1()
                                    .child(column_title("Markdown"))
                                    .child(Textarea::new(&self.template)),
                            )
                            .child(
                                v_flex()
                                    .flex_1()
                                    .min_w(px(0.))
                                    .gap_1()
                                    .child(column_title("Preview"))
                                    .child(
                                        div()
                                            .w_full()
                                            .px_3()
                                            .py_2()
                                            .border_1()
                                            .border_color(cx.theme().border)
                                            .rounded(cx.theme().radius)
                                            .bg(cx.theme().background)
                                            .child(TextView::markdown(
                                                "template-preview",
                                                diagnosis::with_line_breaks(&text),
                                            )),
                                    ),
                            ),
                    )
                    .child(
                        h_flex()
                            .w_full()
                            .gap_2()
                            .child(
                                Button::new("template-default")
                                    .ghost()
                                    .small()
                                    .icon(IconName::RotateCcw)
                                    .label("Reset to Default")
                                    .disabled(is_default)
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.set_text(TEMPLATE, window, cx)
                                    })),
                            )
                            .child(div().flex_1())
                            .child(
                                Button::new("template-discard")
                                    .ghost()
                                    .small()
                                    .label("Discard Changes")
                                    .disabled(!changed)
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        let saved = this.saved.clone();
                                        this.set_text(&saved, window, cx)
                                    })),
                            )
                            .child(
                                Button::new("template-save")
                                    .primary()
                                    .small()
                                    .label("Save")
                                    .disabled(!changed)
                                    .on_click(cx.listener(|this, _, window, cx| this.save(window, cx))),
                            ),
                    ),
            )
    }

    fn render_database(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let count = |result: anyhow::Result<usize>| {
            result.map(widgets::format_count).unwrap_or_else(|_| "-".into())
        };
        GroupBox::new().fill().title("Database").child(
            DescriptionList::horizontal()
                .columns(1)
                .label_width(px(140.))
                .item("File", self.db_path.display().to_string(), 1)
                .item("Patients", count(self.store.count_patients()), 1)
                .item("Visits", count(self.store.count_visits()), 1)
                .item("Format", format!("Kutilang v2, schema {SCHEMA_VERSION}"), 1),
        )
        .text_color(cx.theme().foreground)
    }
}

impl Render for SettingsView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("settings-scroll")
            .size_full()
            .overflow_y_scrollbar()
            .child(
                v_flex()
                    .w_full()
                    .max_w(px(1100.))
                    .gap_6()
                    .px_6()
                    .py_5()
                    .child(div().text_xl().font_weight(FontWeight::SEMIBOLD).child("Settings"))
                    .child(self.render_template(cx))
                    .child(self.render_database(cx)),
            )
    }
}
