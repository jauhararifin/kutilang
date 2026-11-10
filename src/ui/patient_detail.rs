//! The detail pane of a patient: the profile, the list of their visits, and
//! the selected visit (or the editor of a new one).

use std::cell::Cell;
use std::rc::Rc;

use chrono::NaiveDate;
use gpui_kit::assets::IconName;
use gpui_kit::component::avatar::Avatar;
use gpui_kit::component::button::{Button, ButtonVariant, ButtonVariants as _};
use gpui_kit::component::description_list::DescriptionList;
use gpui_kit::component::dialog::DialogFooter;
use gpui_kit::component::list::{List, ListDelegate, ListEvent, ListItem, ListState};
use gpui_kit::component::notification::Notification;
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, IndexPath, Sizable as _, WindowExt as _, h_flex,
    h_resizable, resizable_panel, v_flex,
};
use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, App, Context, Entity, EventEmitter, FontWeight, SharedString, Subscription, Task,
    Window, div, px,
};

use crate::dates;
use crate::diagnosis;
use crate::i18n::{pick, t};
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

struct VisitRow {
    visit: Visit,
    summary: String,
}

/// The visits of one patient, newest first, loaded a page at a time while
/// the list is scrolled.
pub struct PatientVisitList {
    store: Rc<Store>,
    patient_id: String,
    rows: Vec<VisitRow>,
    next: Option<Cursor>,
    error: Option<String>,
}

impl PatientVisitList {
    fn rows_from(visits: Vec<Visit>) -> impl Iterator<Item = VisitRow> {
        visits.into_iter().map(|visit| VisitRow {
            summary: diagnosis::summary(&visit.diagnosis, visit.format, 120),
            visit,
        })
    }

    /// Loads the first visits again, at least as many as are shown.
    fn reload(&mut self) {
        let limit = self.rows.len().max(PAGE_SIZE);
        match self.store.list_visits(&self.patient_id, None, limit) {
            Ok(page) => {
                self.rows = Self::rows_from(page.items).collect();
                self.next = page.next;
                self.error = None;
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    fn load_next_page(&mut self) {
        let Some(cursor) = self.next.take() else {
            return;
        };
        match self
            .store
            .list_visits(&self.patient_id, Some(&cursor), PAGE_SIZE)
        {
            Ok(page) => {
                self.rows.extend(Self::rows_from(page.items));
                self.next = page.next;
            }
            Err(error) => {
                self.next = Some(cursor);
                self.error = Some(error.to_string());
            }
        }
    }

    fn position(&self, id: &str) -> Option<usize> {
        self.rows.iter().position(|row| row.visit.id == id)
    }

    fn visit(&self, id: &str) -> Option<&Visit> {
        self.rows
            .iter()
            .map(|row| &row.visit)
            .find(|visit| visit.id == id)
    }
}

impl ListDelegate for PatientVisitList {
    type Item = ListItem;

    fn items_count(&self, _: usize, _: &App) -> usize {
        self.rows.len()
    }

    fn render_item(
        &mut self,
        ix: IndexPath,
        _: &mut Window,
        cx: &mut Context<ListState<Self>>,
    ) -> Option<ListItem> {
        let row = self.rows.get(ix.row)?;
        let today = dates::today();
        let (date, relative) = match dates::parse_stored(&row.visit.date) {
            Some(date) => (
                dates::format_short(date),
                dates::format_relative(date, today),
            ),
            None => (row.visit.date.clone(), String::new()),
        };
        let muted = cx.theme().muted_foreground;
        let summary = if row.summary.is_empty() {
            " ".to_string()
        } else {
            row.summary.clone()
        };
        Some(
            ListItem::new(SharedString::from(format!(
                "patient-visit-{}",
                row.visit.id
            )))
            .child(
                v_flex()
                    .w_full()
                    .min_w(px(0.))
                    .child(
                        h_flex()
                            .w_full()
                            .gap_2()
                            .child(
                                div()
                                    .flex_none()
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(date),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(0.))
                                    .truncate()
                                    .text_xs()
                                    .text_color(muted)
                                    .child(relative),
                            ),
                    )
                    .child(
                        div()
                            .w_full()
                            .truncate()
                            .text_xs()
                            .text_color(muted)
                            .child(summary),
                    ),
            ),
        )
    }

    fn set_selected_index(
        &mut self,
        _: Option<IndexPath>,
        _: &mut Window,
        _: &mut Context<ListState<Self>>,
    ) {
    }

    fn has_more(&self, _: &App) -> bool {
        self.next.is_some()
    }

    fn load_more_threshold(&self) -> usize {
        10
    }

    fn load_more(&mut self, _: &mut Window, cx: &mut Context<ListState<Self>>) {
        self.load_next_page();
        cx.notify();
    }

    fn render_empty(
        &mut self,
        _: &mut Window,
        cx: &mut Context<ListState<Self>>,
    ) -> impl IntoElement {
        let (title, hint) = match &self.error {
            Some(error) => (
                t(
                    "Kunjungan tidak dapat ditampilkan",
                    "Cannot list the visits",
                ),
                error.clone(),
            ),
            None => (
                t("Belum ada kunjungan", "No visits yet"),
                t(
                    "Tulis yang pertama dengan Kunjungan Baru.",
                    "Write the first one with New Visit.",
                )
                .to_string(),
            ),
        };
        widgets::empty_state(IconName::Calendar, title, hint, cx)
    }

    fn perform_search(
        &mut self,
        _: &str,
        _: &mut Window,
        _: &mut Context<ListState<Self>>,
    ) -> Task<()> {
        Task::ready(())
    }
}

pub struct PatientDetail {
    store: Rc<Store>,
    patient_id: String,
    patient: Option<Patient>,
    visit_count: usize,
    visits: Entity<ListState<PatientVisitList>>,
    /// The visit shown on the right.
    selected: Option<String>,
    /// The editor of a new entry, while one is being written.
    composer: Option<(Entity<VisitEditor>, Subscription)>,
    /// The editor of the selected visit, while it is being edited.
    editor: Option<(String, Entity<VisitEditor>, Subscription)>,
    profile_subscription: Option<Subscription>,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<PatientDetailEvent> for PatientDetail {}

impl PatientDetail {
    pub fn new(
        store: Rc<Store>,
        patient_id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut delegate = PatientVisitList {
            store: store.clone(),
            patient_id: patient_id.clone(),
            rows: Vec::new(),
            next: None,
            error: None,
        };
        delegate.reload();
        let selected = delegate.rows.first().map(|row| row.visit.id.clone());
        let visits = cx.new(|cx| {
            let mut list = ListState::new(delegate, window, cx);
            if selected.is_some() {
                list.set_selected_index(Some(IndexPath::new(0)), window, cx);
            }
            list
        });
        let subscription =
            cx.subscribe_in(
                &visits,
                window,
                |this, visits, event, window, cx| match event {
                    ListEvent::Select(ix) | ListEvent::Confirm(ix) => {
                        let id = visits
                            .read(cx)
                            .delegate()
                            .rows
                            .get(ix.row)
                            .map(|r| r.visit.id.clone());
                        if let Some(id) = id {
                            this.select_visit(&id, window, cx);
                        }
                    }
                    ListEvent::Cancel => {}
                },
            );

        let mut detail = Self {
            store,
            patient_id,
            patient: None,
            visit_count: 0,
            visits,
            selected,
            composer: None,
            editor: None,
            profile_subscription: None,
            _subscriptions: vec![subscription],
        };
        detail.reload_patient(window, cx);
        detail
    }

    pub fn patient_id(&self) -> &str {
        &self.patient_id
    }

    /// Ids of the visits loaded in the list, newest first.
    pub fn entry_ids(&self, cx: &App) -> Vec<String> {
        self.visits
            .read(cx)
            .delegate()
            .rows
            .iter()
            .map(|r| r.visit.id.clone())
            .collect()
    }

    /// The visit shown on the right.
    pub fn selected_visit(&self) -> Option<&str> {
        self.selected.as_deref()
    }

    /// The open editor of a visit.
    pub fn entry_editor(&self, id: &str) -> Option<Entity<VisitEditor>> {
        self.editor
            .as_ref()
            .filter(|(editing, _, _)| editing == id)
            .map(|(_, editor, _)| editor.clone())
    }

    /// The editor of the new entry, when open.
    pub fn composer(&self) -> Option<&Entity<VisitEditor>> {
        self.composer.as_ref().map(|(composer, _)| composer)
    }

    fn notify_error(window: &mut Window, error: impl ToString, cx: &mut Context<Self>) {
        window.push_notification(Notification::error(error.to_string()), cx);
    }

    fn reload_patient(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self.store.get_patient(&self.patient_id) {
            Ok(patient) => self.patient = patient,
            Err(error) => Self::notify_error(window, error, cx),
        }
        self.visit_count = self
            .store
            .count_patient_visits(&self.patient_id)
            .unwrap_or(0);
        cx.notify();
    }

    /// Reloads the visits after a change and shows `select` (or the newest).
    fn reload_visits(&mut self, select: Option<&str>, window: &mut Window, cx: &mut Context<Self>) {
        let selected = self.visits.update(cx, |list, cx| {
            list.delegate_mut().reload();
            let delegate = list.delegate();
            let row = select
                .and_then(|id| delegate.position(id))
                .or_else(|| (!delegate.rows.is_empty()).then_some(0));
            let id = row.map(|row| delegate.rows[row].visit.id.clone());
            list.set_selected_index(row.map(IndexPath::new), window, cx);
            if row.is_some() {
                list.scroll_to_selected_item(window, cx);
            }
            cx.notify();
            id
        });
        self.selected = selected;
        self.reload_patient(window, cx);
    }

    /// Shows a visit on the right, closing the editors.
    pub fn select_visit(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.composer = None;
        if self
            .editor
            .as_ref()
            .is_some_and(|(editing, _, _)| editing != id)
        {
            self.editor = None;
        }
        self.selected = Some(id.to_string());
        self.visits.update(cx, |list, cx| {
            let row = list.delegate().position(id);
            list.set_selected_index(row.map(IndexPath::new), window, cx);
        });
        cx.notify();
    }

    // New entry -------------------------------------------------------------

    pub fn open_composer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.composer.is_some() {
            return;
        }
        self.editor = None;
        let template = self.store.diagnosis_template();
        let composer = cx.new(|cx| VisitEditor::new_entry(template, window, cx));
        let subscription =
            cx.subscribe_in(
                &composer,
                window,
                |this, _, event, window, cx| match event {
                    VisitEditorEvent::Submit {
                        date,
                        diagnosis: Some(diagnosis),
                    } => this.add_entry(*date, diagnosis, window, cx),
                    VisitEditorEvent::Submit {
                        diagnosis: None, ..
                    } => {}
                    VisitEditorEvent::Cancel => {
                        this.composer = None;
                        cx.notify();
                    }
                },
            );
        composer.update(cx, |composer, cx| composer.focus(window, cx));
        self.composer = Some((composer, subscription));
        cx.notify();
    }

    fn add_entry(
        &mut self,
        date: NaiveDate,
        diagnosis: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.store.add_visit(&self.patient_id, date, diagnosis) {
            Ok(visit) => {
                self.composer = None;
                self.reload_visits(Some(&visit.id), window, cx);
                window.push_notification(
                    Notification::success(t("Kunjungan ditambahkan", "Visit added")),
                    cx,
                );
                cx.emit(PatientDetailEvent::Changed);
            }
            Err(error) => Self::notify_error(window, error, cx),
        }
        cx.notify();
    }

    // Existing entries ------------------------------------------------------

    pub fn edit_entry(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(visit) = self.visits.read(cx).delegate().visit(id).cloned() else {
            return;
        };
        self.select_visit(id, window, cx);
        let editor = cx.new(|cx| VisitEditor::edit(&visit, window, cx));
        let visit_id = visit.id.clone();
        let subscription =
            cx.subscribe_in(
                &editor,
                window,
                move |this, _, event, window, cx| match event {
                    VisitEditorEvent::Submit { date, diagnosis } => {
                        this.update_entry(&visit_id, *date, diagnosis.as_deref(), window, cx)
                    }
                    VisitEditorEvent::Cancel => {
                        this.editor = None;
                        cx.notify();
                    }
                },
            );
        editor.update(cx, |editor, cx| editor.focus(window, cx));
        self.editor = Some((visit.id, editor, subscription));
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
            Ok(_) => {
                self.editor = None;
                self.reload_visits(Some(id), window, cx);
                window.push_notification(
                    Notification::success(t("Kunjungan diperbarui", "Visit updated")),
                    cx,
                );
                cx.emit(PatientDetailEvent::Changed);
            }
            Err(error) => Self::notify_error(window, error, cx),
        }
        cx.notify();
    }

    pub fn confirm_delete_entry(
        &mut self,
        id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let this = cx.weak_entity();
        window.open_alert_dialog(cx, move |alert, _, _| {
            let this = this.clone();
            let id = id.clone();
            alert
                .title(t("Hapus kunjungan ini?", "Delete this visit?"))
                .description(t(
                    "Kunjungan akan dihapus dari riwayat.",
                    "The visit will be removed from the history.",
                ))
                .show_cancel(true)
                .ok_text("Delete")
                .ok_variant(ButtonVariant::Danger)
                .on_ok(move |_, window, cx| {
                    this.update(cx, |detail, cx| detail.delete_entry(&id, window, cx))
                        .ok();
                    true
                })
        });
    }

    fn delete_entry(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        // Show the next older visit afterwards, or the newer one.
        let neighbour = {
            let delegate = self.visits.read(cx).delegate();
            delegate.position(id).and_then(|row| {
                delegate
                    .rows
                    .get(row + 1)
                    .or_else(|| row.checked_sub(1).and_then(|r| delegate.rows.get(r)))
                    .map(|r| r.visit.id.clone())
            })
        };
        match self.store.delete_visit(id) {
            Ok(()) => {
                self.editor = None;
                self.reload_visits(neighbour.as_deref(), window, cx);
                window.push_notification(
                    Notification::success(t("Kunjungan dihapus", "Visit deleted")),
                    cx,
                );
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
        let Some(visit) = self.visits.read(cx).delegate().visit(id) else {
            return;
        };
        match print::write_visit_page(patient, visit) {
            Ok(path) => cx.open_url(&print::file_url(&path)),
            Err(error) => Self::notify_error(
                window,
                pick(
                    format!("Gagal mencetak: {error}"),
                    format!("Cannot print: {error}"),
                ),
                cx,
            ),
        }
    }

    // Profile ---------------------------------------------------------------

    pub fn edit_profile(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(patient) = self.patient.clone() else {
            return;
        };
        let form = cx.new(|cx| ProfileForm::new(Some(&patient), window, cx));
        self.profile_subscription =
            Some(
                cx.subscribe_in(&form, window, |this, _, event, window, cx| {
                    let ProfileFormEvent::Submit(profile) = event;
                    this.save_profile(profile, window, cx);
                }),
            );

        let first_render = Cell::new(true);
        window.open_dialog(cx, move |dialog, window, cx| {
            if first_render.replace(false) {
                form.update(cx, |form, cx| form.focus(window, cx));
            }
            let save = form.clone();
            dialog
                .title(t("Ubah Pasien", "Edit Patient"))
                .w(px(520.))
                .child(form.clone())
                .footer(
                    DialogFooter::new()
                        .child(
                            Button::new("profile-cancel")
                                .label(t("Batal", "Cancel"))
                                .on_click(|_, window, cx| window.close_dialog(cx)),
                        )
                        .child(
                            Button::new("profile-save")
                                .primary()
                                .label(t("Simpan", "Save"))
                                .on_click(move |_, window, cx| {
                                    save.update(cx, |form, cx| form.submit(window, cx))
                                }),
                        ),
                )
        });
    }

    fn save_profile(
        &mut self,
        profile: &PatientProfile,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.store.update_patient(&self.patient_id, profile) {
            Ok(patient) => {
                self.patient = Some(patient);
                self.profile_subscription = None;
                window.close_dialog(cx);
                window.push_notification(
                    Notification::success(t("Pasien diperbarui", "Patient updated")),
                    cx,
                );
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
                .title(t("Hapus pasien ini?", "Delete this patient?"))
                .description(t(
                    "Pasien tidak akan muncul lagi di daftar.",
                    "The patient will no longer appear in the lists.",
                ))
                .show_cancel(true)
                .ok_text("Delete")
                .ok_variant(ButtonVariant::Danger)
                .on_ok(move |_, window, cx| {
                    this.update(cx, |detail, cx| detail.delete_patient(window, cx))
                        .ok();
                    true
                })
        });
    }

    fn delete_patient(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self.store.delete_patient(&self.patient_id) {
            Ok(()) => {
                window.push_notification(
                    Notification::success(t("Pasien dihapus", "Patient deleted")),
                    cx,
                );
                cx.emit(PatientDetailEvent::Deleted);
            }
            Err(error) => Self::notify_error(window, error, cx),
        }
    }

    // Rendering -------------------------------------------------------------

    /// The left column: who the patient is, then the list of their visits.
    fn render_patient_column(&self, patient: &Patient, cx: &mut Context<Self>) -> impl IntoElement {
        let today = dates::today();
        let or_dash = |value: &str| {
            let value = value.trim();
            if value.is_empty() {
                "-".to_string()
            } else {
                value.to_string()
            }
        };
        let identity = h_flex()
            .w_full()
            .gap_3()
            .child(Avatar::new().name(patient.name.clone()))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .text_lg()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(patient.name.clone()),
            )
            .child(
                Button::new("edit-profile")
                    .ghost()
                    .small()
                    .icon(IconName::Pencil)
                    .tooltip(t("Ubah pasien", "Edit patient"))
                    .on_click(cx.listener(|this, _, window, cx| this.edit_profile(window, cx))),
            )
            .child(
                Button::new("delete-patient")
                    .ghost()
                    .small()
                    .icon(IconName::Trash)
                    .tooltip(t("Hapus pasien", "Delete patient"))
                    .on_click(
                        cx.listener(|this, _, window, cx| this.confirm_delete_patient(window, cx)),
                    ),
            );
        let profile = DescriptionList::horizontal()
            .columns(1)
            .label_width(px(110.))
            .item(
                t("Nama KK", "Head of Family"),
                or_dash(&patient.head_family_name),
                1,
            )
            .item(
                t("Tanggal Lahir", "Date of Birth"),
                dates::display_with_age(&patient.birth_date, today),
                1,
            )
            .item(t("Alamat", "Address"), or_dash(&patient.address), 1)
            .item(
                t("Kunjungan Terakhir", "Last Visit"),
                dates::display_with_relative(&patient.last_visit, today),
                1,
            );

        v_flex()
            .size_full()
            .child(
                v_flex()
                    .w_full()
                    .flex_none()
                    .gap_3()
                    .px_4()
                    .py_3()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .child(identity)
                    .child(div().text_sm().child(profile)),
            )
            .child(
                widgets::pane_header(t("Kunjungan", "Visits"), Some(self.visit_count), cx).child(
                    Button::new("new-entry")
                        .primary()
                        .xsmall()
                        .icon(IconName::Plus)
                        .label(t("Kunjungan Baru", "New Visit"))
                        .disabled(self.composer.is_some())
                        .on_click(
                            cx.listener(|this, _, window, cx| this.open_composer(window, cx)),
                        ),
                ),
            )
            .child(div().flex_1().min_h(px(0.)).child(List::new(&self.visits)))
    }

    fn render_selected(&self, cx: &mut Context<Self>) -> AnyElement {
        let (scroll_id, content): (SharedString, AnyElement) =
            if let Some((composer, _)) = &self.composer {
                (
                    "patient-new-entry".into(),
                    v_flex()
                        .w_full()
                        .gap_2()
                        .child(
                            div()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(t("Kunjungan Baru", "New Visit")),
                        )
                        .child(composer.clone())
                        .into_any_element(),
                )
            } else if let Some((id, editor, _)) = &self.editor {
                (
                    format!("patient-edit-{id}").into(),
                    editor.clone().into_any_element(),
                )
            } else if let Some(visit) = self
                .selected
                .as_deref()
                .and_then(|id| self.visits.read(cx).delegate().visit(id))
                .cloned()
            {
                let action = |name: &str, icon: IconName, tooltip: &'static str| {
                    Button::new(SharedString::from(format!("{name}-{}", visit.id)))
                        .icon(icon)
                        .ghost()
                        .small()
                        .tooltip(tooltip)
                };
                let (print_id, edit_id, delete_id) =
                    (visit.id.clone(), visit.id.clone(), visit.id.clone());
                let actions = [
                    action("print", IconName::Printer, t("Cetak", "Print"))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.print_entry(&print_id, window, cx)
                        }))
                        .into_any_element(),
                    action("edit", IconName::Pencil, t("Ubah", "Edit"))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.edit_entry(&edit_id, window, cx)
                        }))
                        .into_any_element(),
                    action("delete", IconName::Trash, t("Hapus", "Delete"))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.confirm_delete_entry(delete_id.clone(), window, cx)
                        }))
                        .into_any_element(),
                ];
                (
                    format!("patient-visit-scroll-{}", visit.id).into(),
                    entry_view("patient", &visit, actions, cx).into_any_element(),
                )
            } else {
                return widgets::empty_state(
                    IconName::Calendar,
                    t("Belum ada kunjungan yang dipilih", "No visit selected"),
                    t(
                        "Pilih kunjungan dari daftar, atau tulis kunjungan baru.",
                        "Choose a visit from the list, or write a new one.",
                    ),
                    cx,
                );
            };

        div()
            .id(scroll_id)
            .size_full()
            .overflow_y_scrollbar()
            .child(div().w_full().px_5().py_4().child(content))
            .into_any_element()
    }
}

impl Render for PatientDetail {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(patient) = self.patient.clone() else {
            return widgets::empty_state(
                IconName::User,
                t("Pasien tidak ditemukan", "Patient not found"),
                t(
                    "Pasien mungkin sudah dihapus.",
                    "The patient may have been deleted.",
                ),
                cx,
            );
        };

        h_resizable(SharedString::from(format!(
            "patient-split-{}",
            self.patient_id
        )))
        .child(
            resizable_panel()
                .size(px(330.))
                .size_range(px(260.)..px(480.))
                .child(self.render_patient_column(&patient, cx)),
        )
        .child(resizable_panel().child(self.render_selected(cx)))
        .into_any_element()
    }
}
