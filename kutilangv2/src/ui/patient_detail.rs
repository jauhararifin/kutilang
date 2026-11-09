//! The detail pane of a patient: profile, new entry, and the visit history.

use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;

use chrono::NaiveDate;
use gpui_kit::assets::IconName;
use gpui_kit::component::avatar::Avatar;
use gpui_kit::component::button::{Button, ButtonVariant, ButtonVariants as _};
use gpui_kit::component::description_list::DescriptionList;
use gpui_kit::component::dialog::DialogFooter;
use gpui_kit::component::notification::Notification;
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Sizable as _, WindowExt as _, h_flex, v_flex,
};
use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, Context, Entity, EventEmitter, FontWeight, SharedString, Subscription, Window, div,
    px,
};

use crate::dates;
use crate::model::{Cursor, Patient, PatientProfile, Visit};
use crate::print;
use crate::store::Store;
use crate::ui::entry::entry_view;
use crate::ui::profile_form::{ProfileForm, ProfileFormEvent};
use crate::ui::visit_editor::{VisitEditor, VisitEditorEvent};
use crate::ui::widgets;

const PAGE_SIZE: usize = 30;

pub enum PatientDetailEvent {
    /// The patient's data changed: lists showing it are stale.
    Changed,
    /// The patient was deleted.
    Deleted,
}

struct Entry {
    visit: Visit,
    editor: Option<Entity<VisitEditor>>,
}

pub struct PatientDetail {
    store: Rc<Store>,
    patient_id: String,
    patient: Option<Patient>,
    visit_count: usize,
    /// The editor of a new entry, while one is being written.
    composer: Option<Entity<VisitEditor>>,
    entries: Vec<Entry>,
    next: Option<Cursor>,
    /// Subscriptions of the composer, open entry editors and the profile
    /// dialog, by owner.
    subscriptions: HashMap<String, Subscription>,
}

impl EventEmitter<PatientDetailEvent> for PatientDetail {}

impl PatientDetail {
    pub fn new(store: Rc<Store>, patient_id: String, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut detail = Self {
            store,
            patient_id,
            patient: None,
            visit_count: 0,
            composer: None,
            entries: Vec::new(),
            next: None,
            subscriptions: HashMap::new(),
        };
        detail.reload_patient(window, cx);
        if detail.patient.is_some() {
            detail.load_entries(window, cx);
        }
        detail
    }

    pub fn patient_id(&self) -> &str {
        &self.patient_id
    }

    /// Ids of the entries shown, newest first.
    pub fn entry_ids(&self) -> Vec<String> {
        self.entries.iter().map(|e| e.visit.id.clone()).collect()
    }

    /// The open editor of an entry.
    pub fn entry_editor(&self, id: &str) -> Option<Entity<VisitEditor>> {
        self.entries.iter().find(|e| e.visit.id == id).and_then(|e| e.editor.clone())
    }

    /// The editor of the new entry, when open.
    pub fn composer(&self) -> Option<&Entity<VisitEditor>> {
        self.composer.as_ref()
    }

    fn notify_error(window: &mut Window, error: impl ToString, cx: &mut Context<Self>) {
        window.push_notification(Notification::error(error.to_string()), cx);
    }

    fn reload_patient(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self.store.get_patient(&self.patient_id) {
            Ok(patient) => self.patient = patient,
            Err(error) => Self::notify_error(window, error, cx),
        }
        self.visit_count = self.store.count_patient_visits(&self.patient_id).unwrap_or(0);
        cx.notify();
    }

    fn load_entries(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self.store.list_visits(&self.patient_id, self.next.as_ref(), PAGE_SIZE) {
            Ok(page) => {
                self.entries
                    .extend(page.items.into_iter().map(|visit| Entry { visit, editor: None }));
                self.next = page.next;
            }
            Err(error) => Self::notify_error(window, error, cx),
        }
        cx.notify();
    }

    /// Puts a visit where it belongs in the newest-first history. A visit
    /// older than everything loaded so far shows up with the later pages.
    fn insert_sorted(&mut self, visit: Visit) {
        let key = |v: &Visit| (v.date.clone(), v.id.clone());
        let position = self
            .entries
            .iter()
            .position(|entry| key(&visit) > key(&entry.visit))
            .unwrap_or(self.entries.len());
        if position == self.entries.len() && self.next.is_some() {
            return;
        }
        self.entries.insert(position, Entry { visit, editor: None });
    }

    // New entry -------------------------------------------------------------

    pub fn open_composer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.composer.is_some() {
            return;
        }
        let template = self.store.diagnosis_template();
        let composer = cx.new(|cx| VisitEditor::new_entry(template, window, cx));
        let subscription = cx.subscribe_in(&composer, window, |this, _, event, window, cx| match event {
            VisitEditorEvent::Submit {
                date,
                diagnosis: Some(diagnosis),
            } => this.add_entry(*date, diagnosis, window, cx),
            VisitEditorEvent::Submit { diagnosis: None, .. } => {}
            VisitEditorEvent::Cancel => this.close_composer(cx),
        });
        composer.update(cx, |composer, cx| composer.focus(window, cx));
        self.subscriptions.insert("composer".into(), subscription);
        self.composer = Some(composer);
        cx.notify();
    }

    fn close_composer(&mut self, cx: &mut Context<Self>) {
        self.composer = None;
        self.subscriptions.remove("composer");
        cx.notify();
    }

    fn add_entry(&mut self, date: NaiveDate, diagnosis: &str, window: &mut Window, cx: &mut Context<Self>) {
        match self.store.add_visit(&self.patient_id, date, diagnosis) {
            Ok(visit) => {
                self.insert_sorted(visit);
                self.close_composer(cx);
                self.reload_patient(window, cx);
                window.push_notification(Notification::success("Visit inserted"), cx);
                cx.emit(PatientDetailEvent::Changed);
            }
            Err(error) => Self::notify_error(window, error, cx),
        }
        cx.notify();
    }

    // Existing entries ------------------------------------------------------

    pub fn edit_entry(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(visit) = self.entries.iter().find(|e| e.visit.id == id).map(|e| e.visit.clone()) else {
            return;
        };
        let editor = cx.new(|cx| VisitEditor::edit(&visit, window, cx));
        let visit_id = visit.id.clone();
        let subscription = cx.subscribe_in(&editor, window, move |this, _, event, window, cx| match event {
            VisitEditorEvent::Submit { date, diagnosis } => {
                this.update_entry(&visit_id, *date, diagnosis.as_deref(), window, cx)
            }
            VisitEditorEvent::Cancel => this.close_entry_editor(&visit_id, cx),
        });
        editor.update(cx, |editor, cx| editor.focus(window, cx));
        if let Some(entry) = self.entries.iter_mut().find(|e| e.visit.id == id) {
            entry.editor = Some(editor);
        }
        self.subscriptions.insert(visit.id, subscription);
        cx.notify();
    }

    fn close_entry_editor(&mut self, id: &str, cx: &mut Context<Self>) {
        if let Some(entry) = self.entries.iter_mut().find(|e| e.visit.id == id) {
            entry.editor = None;
        }
        self.subscriptions.remove(id);
        cx.notify();
    }

    fn update_entry(
        &mut self,
        id: &str,
        date: NaiveDate,
        diagnosis: Option<&str>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.store.update_visit(id, date, diagnosis) {
            Ok(visit) => {
                self.entries.retain(|entry| entry.visit.id != id);
                self.subscriptions.remove(id);
                self.insert_sorted(visit);
                self.reload_patient(window, cx);
                window.push_notification(Notification::success("Visit updated"), cx);
                cx.emit(PatientDetailEvent::Changed);
            }
            Err(error) => Self::notify_error(window, error, cx),
        }
        cx.notify();
    }

    pub fn confirm_delete_entry(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
        let this = cx.weak_entity();
        window.open_alert_dialog(cx, move |alert, _, _| {
            let this = this.clone();
            let id = id.clone();
            alert
                .title("Delete this entry?")
                .description("The entry will be removed from the history.")
                .show_cancel(true)
                .ok_text("Delete")
                .ok_variant(ButtonVariant::Danger)
                .on_ok(move |_, window, cx| {
                    this.update(cx, |detail, cx| detail.delete_entry(&id, window, cx)).ok();
                    true
                })
        });
    }

    fn delete_entry(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        match self.store.delete_visit(id) {
            Ok(()) => {
                self.entries.retain(|entry| entry.visit.id != id);
                self.subscriptions.remove(id);
                self.reload_patient(window, cx);
                window.push_notification(Notification::success("Visit deleted"), cx);
                cx.emit(PatientDetailEvent::Changed);
            }
            Err(error) => Self::notify_error(window, error, cx),
        }
        cx.notify();
    }

    fn print_entry(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(patient) = &self.patient else {
            return;
        };
        let Some(entry) = self.entries.iter().find(|e| e.visit.id == id) else {
            return;
        };
        match print::write_visit_page(patient, &entry.visit) {
            Ok(path) => cx.open_url(&print::file_url(&path)),
            Err(error) => Self::notify_error(window, format!("Cannot print: {error}"), cx),
        }
    }

    // Profile ---------------------------------------------------------------

    pub fn edit_profile(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(patient) = self.patient.clone() else {
            return;
        };
        let form = cx.new(|cx| ProfileForm::new(Some(&patient), window, cx));
        let subscription = cx.subscribe_in(&form, window, |this, _, event, window, cx| {
            let ProfileFormEvent::Submit(profile) = event;
            this.save_profile(profile, window, cx);
        });
        self.subscriptions.insert("profile".into(), subscription);

        let first_render = Cell::new(true);
        window.open_dialog(cx, move |dialog, window, cx| {
            if first_render.replace(false) {
                form.update(cx, |form, cx| form.focus(window, cx));
            }
            let save = form.clone();
            dialog
                .title("Edit Patient")
                .w(px(520.))
                .child(form.clone())
                .footer(
                    DialogFooter::new()
                        .child(
                            Button::new("profile-cancel")
                                .label("Cancel")
                                .on_click(|_, window, cx| window.close_dialog(cx)),
                        )
                        .child(Button::new("profile-save").primary().label("Save").on_click(
                            move |_, window, cx| save.update(cx, |form, cx| form.submit(window, cx)),
                        )),
                )
        });
    }

    fn save_profile(&mut self, profile: &PatientProfile, window: &mut Window, cx: &mut Context<Self>) {
        match self.store.update_patient(&self.patient_id, profile) {
            Ok(patient) => {
                self.patient = Some(patient);
                self.subscriptions.remove("profile");
                window.close_dialog(cx);
                window.push_notification(Notification::success("Patient updated"), cx);
                cx.emit(PatientDetailEvent::Changed);
            }
            Err(error) => Self::notify_error(window, error, cx),
        }
        cx.notify();
    }

    pub fn confirm_delete_patient(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let this = cx.weak_entity();
        window.open_alert_dialog(cx, move |alert, _, _| {
            let this = this.clone();
            alert
                .title("Delete this patient?")
                .description("The patient will no longer appear in the lists.")
                .show_cancel(true)
                .ok_text("Delete")
                .ok_variant(ButtonVariant::Danger)
                .on_ok(move |_, window, cx| {
                    this.update(cx, |detail, cx| detail.delete_patient(window, cx)).ok();
                    true
                })
        });
    }

    fn delete_patient(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self.store.delete_patient(&self.patient_id) {
            Ok(()) => {
                window.push_notification(Notification::success("Patient deleted"), cx);
                cx.emit(PatientDetailEvent::Deleted);
            }
            Err(error) => Self::notify_error(window, error, cx),
        }
    }

    // Rendering -------------------------------------------------------------

    fn render_header(&self, patient: &Patient, cx: &mut Context<Self>) -> impl IntoElement {
        let today = dates::today();
        let mut subtitle = Vec::new();
        if let Some(birth) = dates::parse_stored(&patient.birth_date) {
            subtitle.push(format!("{} tahun", dates::age_years(birth, today)));
        }
        if !patient.head_family_name.trim().is_empty() {
            subtitle.push(format!("KK {}", patient.head_family_name.trim()));
        }
        h_flex()
            .w_full()
            .flex_none()
            .gap_3()
            .px_5()
            .py_3()
            .border_b_1()
            .border_color(cx.theme().border)
            .child(Avatar::new().name(patient.name.clone()))
            .child(
                v_flex()
                    .flex_1()
                    .min_w(px(0.))
                    .child(
                        div()
                            .text_lg()
                            .font_weight(FontWeight::SEMIBOLD)
                            .truncate()
                            .child(patient.name.clone()),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .truncate()
                            .child(subtitle.join(" · ")),
                    ),
            )
            .child(
                Button::new("new-entry")
                    .primary()
                    .small()
                    .icon(IconName::Plus)
                    .label("New Entry")
                    .disabled(self.composer.is_some())
                    .on_click(cx.listener(|this, _, window, cx| this.open_composer(window, cx))),
            )
            .child(
                Button::new("edit-profile")
                    .ghost()
                    .small()
                    .icon(IconName::Pencil)
                    .tooltip("Edit patient")
                    .on_click(cx.listener(|this, _, window, cx| this.edit_profile(window, cx))),
            )
            .child(
                Button::new("delete-patient")
                    .ghost()
                    .small()
                    .icon(IconName::Trash)
                    .tooltip("Delete patient")
                    .on_click(cx.listener(|this, _, window, cx| this.confirm_delete_patient(window, cx))),
            )
    }

    fn render_profile(&self, patient: &Patient) -> impl IntoElement {
        let today = dates::today();
        let or_dash = |value: &str| {
            let value = value.trim();
            if value.is_empty() { "-".to_string() } else { value.to_string() }
        };
        DescriptionList::horizontal()
            .columns(2)
            .label_width(px(140.))
            .bordered(true)
            .item("Nama", patient.name.clone(), 1)
            .item("Nama KK", or_dash(&patient.head_family_name), 1)
            .item("Tanggal Lahir", dates::display_with_age(&patient.birth_date, today), 1)
            .item(
                "Kunjungan Terakhir",
                dates::display_with_relative(&patient.last_visit, today),
                1,
            )
            .item("Alamat", or_dash(&patient.address), 2)
    }

    fn render_entry(&self, entry: &Entry, cx: &mut Context<Self>) -> AnyElement {
        let visit = &entry.visit;
        let separator = cx.theme().border;
        let body = match &entry.editor {
            Some(editor) => div().w_full().child(editor.clone()).into_any_element(),
            None => {
                let (print_id, edit_id, delete_id) =
                    (visit.id.clone(), visit.id.clone(), visit.id.clone());
                let action = |name: &str, icon: IconName, tooltip: &'static str| {
                    Button::new(SharedString::from(format!("{name}-{}", visit.id)))
                        .icon(icon)
                        .ghost()
                        .xsmall()
                        .tooltip(tooltip)
                };
                let actions = [
                    action("print", IconName::Printer, "Print")
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.print_entry(&print_id, window, cx)
                        }))
                        .into_any_element(),
                    action("edit", IconName::Pencil, "Edit")
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.edit_entry(&edit_id, window, cx)
                        }))
                        .into_any_element(),
                    action("delete", IconName::Trash, "Delete")
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.confirm_delete_entry(delete_id.clone(), window, cx)
                        }))
                        .into_any_element(),
                ];
                entry_view("patient", visit, actions, cx).into_any_element()
            }
        };
        div()
            .w_full()
            .pb_4()
            .border_b_1()
            .border_color(separator)
            .child(body)
            .into_any_element()
    }
}

impl Render for PatientDetail {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(patient) = self.patient.clone() else {
            return widgets::empty_state(
                IconName::User,
                "Patient not found",
                "The patient may have been deleted.",
                cx,
            );
        };

        let entries: Vec<AnyElement> = self
            .entries
            .iter()
            .map(|entry| self.render_entry(entry, cx))
            .collect();
        let composer = self.composer.clone().map(|composer| {
            v_flex()
                .w_full()
                .gap_2()
                .p_3()
                .border_1()
                .border_color(cx.theme().border)
                .rounded(cx.theme().radius)
                .bg(cx.theme().secondary)
                .child(div().font_weight(FontWeight::SEMIBOLD).child("New Entry"))
                .child(composer)
        });

        v_flex()
            .size_full()
            .child(self.render_header(&patient, cx))
            .child(
                div()
                    .id(SharedString::from(format!("patient-scroll-{}", self.patient_id)))
                    .flex_1()
                    .min_h(px(0.))
                    .w_full()
                    .overflow_y_scrollbar()
                    .child(
                        v_flex()
                            .w_full()
                            .gap_5()
                            .px_5()
                            .py_4()
                            .child(self.render_profile(&patient))
                            .children(composer)
                            .child(
                                h_flex()
                                    .gap_2()
                                    .child(div().font_weight(FontWeight::SEMIBOLD).child("History"))
                                    .child(widgets::muted(widgets::format_count(self.visit_count), cx).text_sm()),
                            )
                            .when(entries.is_empty(), |page| {
                                page.child(widgets::muted("No visits yet.", cx).text_sm())
                            })
                            .children(entries)
                            .when(self.next.is_some(), |page| {
                                page.child(
                                    h_flex().w_full().justify_center().child(
                                        Button::new("more-visits")
                                            .ghost()
                                            .small()
                                            .label("Load more")
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.load_entries(window, cx)
                                            })),
                                    ),
                                )
                            }),
                    ),
            )
            .into_any_element()
    }
}
