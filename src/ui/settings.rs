//! The Settings section: the template of new entries, backups and export,
//! and database details.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariant, ButtonVariants as _};
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
use gpui_kit::{
    Context, Entity, EventEmitter, FontWeight, PathPromptOptions, Subscription, Window, div, px,
};

use crate::dates;
use crate::diagnosis::{self, TEMPLATE};
use crate::export;
use crate::store::{Backup, SCHEMA_VERSION, Store, timestamped_path};
use crate::ui::widgets;

pub enum SettingsEvent {
    /// Every record was replaced by a backup.
    DataReplaced,
}

pub struct SettingsView {
    store: Rc<Store>,
    db_path: PathBuf,
    template: Entity<TextareaState>,
    /// The template as stored.
    saved: String,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<SettingsEvent> for SettingsView {}

/// Where save dialogs start: the Documents folder, or the database's.
fn default_folder(db_path: &Path) -> PathBuf {
    dirs::document_dir()
        .or_else(|| db_path.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."))
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

    // Backup, restore, export -------------------------------------------

    /// Asks where to save a backup, then writes it.
    pub fn back_up(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let name = format!("kutilang-backup-{}.sqlite3", dates::to_stored(dates::today()));
        let chosen = cx.prompt_for_new_path(&default_folder(&self.db_path), Some(&name));
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(path))) = chosen.await else {
                return;
            };
            this.update_in(cx, |this, window, cx| match this.store.backup_to(&path) {
                Ok(()) => window.push_notification(
                    Notification::success(format!("Backup saved as {}.", path.display())),
                    cx,
                ),
                Err(error) => window.push_notification(Notification::error(format!("{error:#}")), cx),
            })
            .ok();
        })
        .detach();
    }

    /// Asks for a backup, shows what it holds, and restores it once confirmed.
    pub fn restore(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let chosen = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Restore".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(paths))) = chosen.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            this.update_in(cx, |this, window, cx| this.confirm_restore(&path, window, cx))
                .ok();
        })
        .detach();
    }

    pub fn confirm_restore(&mut self, path: &Path, window: &mut Window, cx: &mut Context<Self>) {
        let backup = match Store::open_backup(path) {
            Ok(backup) => backup,
            Err(error) => {
                window.push_notification(Notification::error(format!("{error:#}")), cx);
                return;
            }
        };
        let description = format!(
            "Every patient and visit now in Kutilang will be replaced by the {} patients \
             and {} visits of {}. A copy of the current data is saved first.",
            widgets::format_count(backup.patients),
            widgets::format_count(backup.visits),
            path.file_name().map(|n| n.to_string_lossy()).unwrap_or_default(),
        );
        // The dialog's builder can run more than once; the backup is used once.
        let backup = Rc::new(RefCell::new(Some(backup)));
        let this = cx.weak_entity();
        window.open_alert_dialog(cx, move |alert, _, _| {
            let (this, backup) = (this.clone(), backup.clone());
            alert
                .title("Restore this backup?")
                .description(description.clone())
                .show_cancel(true)
                .ok_text("Restore")
                .ok_variant(ButtonVariant::Danger)
                .on_ok(move |_, window, cx| {
                    if let Some(backup) = backup.borrow_mut().take() {
                        this.update(cx, |this, cx| this.apply_restore(backup, window, cx)).ok();
                    }
                    true
                })
        });
    }

    /// Restores an opened backup, saving the current data first.
    pub fn apply_restore(&mut self, backup: Backup, window: &mut Window, cx: &mut Context<Self>) {
        let previous = timestamped_path(&self.db_path, "before-restore");
        if let Err(error) = self.store.backup_to(&previous) {
            window.push_notification(
                Notification::error(format!(
                    "Nothing was restored: cannot save a copy of the current data first ({error:#})."
                )),
                cx,
            );
            return;
        }
        match self.store.restore(&backup) {
            Ok(()) => {
                self.saved = self.store.diagnosis_template();
                let saved = self.saved.clone();
                self.set_text(&saved, window, cx);
                window.push_notification(
                    Notification::success(format!(
                        "Restored {} patients and {} visits. The previous data was saved as {}.",
                        widgets::format_count(backup.patients),
                        widgets::format_count(backup.visits),
                        previous.display()
                    ))
                    .autohide(false),
                    cx,
                );
                cx.emit(SettingsEvent::DataReplaced);
            }
            Err(error) => window.push_notification(
                Notification::error(format!("Cannot restore: {error:#}")),
                cx,
            ),
        }
        cx.notify();
    }

    /// Asks where to save the spreadsheet, then writes it.
    pub fn export(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let name = format!("kutilang-{}.xlsx", dates::to_stored(dates::today()));
        let chosen = cx.prompt_for_new_path(&default_folder(&self.db_path), Some(&name));
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(path))) = chosen.await else {
                return;
            };
            this.update_in(cx, |this, window, cx| {
                match export::export_xlsx(&this.store, &path) {
                    Ok(report) => window.push_notification(
                        Notification::success(format!(
                            "Exported {} patients and {} visits to {}.",
                            widgets::format_count(report.patients),
                            widgets::format_count(report.visits),
                            path.display()
                        )),
                        cx,
                    ),
                    Err(error) => window.push_notification(
                        Notification::error(format!("Cannot export: {error:#}")),
                        cx,
                    ),
                }
            })
            .ok();
        })
        .detach();
    }

    fn render_data(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let row = |button: Button, text: &'static str, cx: &Context<Self>| {
            h_flex()
                .w_full()
                .gap_4()
                .child(div().w(px(190.)).flex_none().child(button))
                .child(widgets::muted(text, cx).text_sm())
        };
        GroupBox::new().fill().title("Backup & Export").child(
            v_flex()
                .w_full()
                .gap_3()
                .child(row(
                    Button::new("back-up")
                        .small()
                        .label("Back Up…")
                        .on_click(cx.listener(|this, _, window, cx| this.back_up(window, cx))),
                    "Save a copy of every patient, visit and setting to a file of your choice.",
                    cx,
                ))
                .child(row(
                    Button::new("restore")
                        .small()
                        .label("Restore…")
                        .on_click(cx.listener(|this, _, window, cx| this.restore(window, cx))),
                    "Replace everything with a backup (also a database of the old application). \
                     The current data is saved next to the database first.",
                    cx,
                ))
                .child(row(
                    Button::new("export")
                        .small()
                        .label("Export to Excel…")
                        .on_click(cx.listener(|this, _, window, cx| this.export(window, cx))),
                    "Write all patients and visits to an Excel workbook (.xlsx), one sheet each.",
                    cx,
                )),
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
                    .child(self.render_data(cx))
                    .child(self.render_template(cx))
                    .child(self.render_database(cx)),
            )
    }
}
