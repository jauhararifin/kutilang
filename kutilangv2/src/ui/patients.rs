//! The Patients section: the list of all patients and the selected one.

use std::cell::Cell;
use std::rc::Rc;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::dialog::DialogFooter;
use gpui_kit::component::list::{List, ListDelegate, ListEvent, ListItem, ListState};
use gpui_kit::component::notification::Notification;
use gpui_kit::component::{
    ActiveTheme as _, IndexPath, Selectable as _, Sizable as _, WindowExt as _, h_flex,
    h_resizable, resizable_panel, v_flex,
};
use gpui_kit::prelude::*;
use gpui_kit::{
    App, Context, Entity, EventEmitter, FontWeight, Subscription, Task, Window, div, px,
};

use crate::dates;
use crate::model::{Patient, PatientOrder};
use crate::store::Store;
use crate::ui::patient_detail::{PatientDetail, PatientDetailEvent};
use crate::ui::profile_form::{ProfileForm, ProfileFormEvent};
use crate::ui::widgets;

pub enum PatientsEvent {
    /// Patients or visits changed.
    Changed,
}

/// The list of patients, filtered by the search field.
pub struct PatientList {
    store: Rc<Store>,
    patients: Vec<Patient>,
    query: String,
    order: PatientOrder,
    error: Option<String>,
}

impl PatientList {
    fn reload(&mut self) {
        match self.store.list_patients(&self.query, self.order) {
            Ok(patients) => {
                self.patients = patients;
                self.error = None;
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    fn position(&self, id: &str) -> Option<usize> {
        self.patients.iter().position(|p| p.id == id)
    }

    pub fn patients(&self) -> &[Patient] {
        &self.patients
    }
}

impl ListDelegate for PatientList {
    type Item = ListItem;

    fn perform_search(
        &mut self,
        query: &str,
        _: &mut Window,
        cx: &mut Context<ListState<Self>>,
    ) -> Task<()> {
        self.query = query.to_string();
        self.reload();
        cx.notify();
        Task::ready(())
    }

    fn items_count(&self, _: usize, _: &App) -> usize {
        self.patients.len()
    }

    fn render_item(
        &mut self,
        ix: IndexPath,
        _: &mut Window,
        cx: &mut Context<ListState<Self>>,
    ) -> Option<ListItem> {
        let patient = self.patients.get(ix.row)?;
        let today = dates::today();
        let mut details = Vec::new();
        if let Some(birth) = dates::parse_stored(&patient.birth_date) {
            details.push(format!("{} th", dates::age_years(birth, today)));
        }
        if !patient.head_family_name.trim().is_empty() {
            details.push(format!("KK {}", patient.head_family_name.trim()));
        }
        if !patient.address.trim().is_empty() {
            details.push(patient.address.trim().to_string());
        }
        let last_visit = dates::parse_stored(&patient.last_visit)
            .map(dates::format_short)
            .unwrap_or_default();
        let muted = cx.theme().muted_foreground;

        Some(
            ListItem::new(("patient", ix.row)).child(
                v_flex()
                    .w_full()
                    .min_w(px(0.))
                    .child(
                        h_flex()
                            .w_full()
                            .gap_2()
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(0.))
                                    .truncate()
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(patient.name.clone()),
                            )
                            .child(div().flex_none().text_xs().text_color(muted).child(last_visit)),
                    )
                    .child(
                        div()
                            .w_full()
                            .truncate()
                            .text_xs()
                            .text_color(muted)
                            .child(if details.is_empty() { " ".to_string() } else { details.join(" · ") }),
                    ),
            ),
        )
    }

    fn set_selected_index(&mut self, _: Option<IndexPath>, _: &mut Window, _: &mut Context<ListState<Self>>) {}

    fn render_empty(&mut self, _: &mut Window, cx: &mut Context<ListState<Self>>) -> impl IntoElement {
        let (title, hint) = match &self.error {
            Some(error) => ("Cannot list the patients", error.clone()),
            None => ("No patients found", "Try another name.".to_string()),
        };
        widgets::empty_state(IconName::Users, title, hint, cx)
    }
}

pub struct PatientsView {
    store: Rc<Store>,
    list: Entity<ListState<PatientList>>,
    detail: Option<Entity<PatientDetail>>,
    detail_subscription: Option<Subscription>,
    form_subscription: Option<Subscription>,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<PatientsEvent> for PatientsView {}

impl PatientsView {
    pub fn new(store: Rc<Store>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut delegate = PatientList {
            store: store.clone(),
            patients: Vec::new(),
            query: String::new(),
            order: PatientOrder::default(),
            error: None,
        };
        delegate.reload();
        let list = cx.new(|cx| ListState::new(delegate, window, cx).searchable(true));
        let subscription = cx.subscribe_in(&list, window, |this, list, event, window, cx| match event {
            ListEvent::Select(ix) | ListEvent::Confirm(ix) => {
                let id = list.read(cx).delegate().patients.get(ix.row).map(|p| p.id.clone());
                if let Some(id) = id {
                    this.show_patient(id, window, cx);
                }
            }
            ListEvent::Cancel => {}
        });
        Self {
            store,
            list,
            detail: None,
            detail_subscription: None,
            form_subscription: None,
            _subscriptions: vec![subscription],
        }
    }

    pub fn list(&self) -> &Entity<ListState<PatientList>> {
        &self.list
    }

    pub fn detail(&self) -> Option<&Entity<PatientDetail>> {
        self.detail.as_ref()
    }

    pub fn focus_search(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.list.update(cx, |list, cx| list.focus(window, cx));
    }

    /// Filters the list as if `query` was typed in the search field.
    pub fn search(&mut self, query: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.list.update(cx, |list, cx| list.set_query(query, window, cx));
    }

    fn show_patient(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
        if self.detail.as_ref().is_some_and(|d| d.read(cx).patient_id() == id) {
            return;
        }
        let store = self.store.clone();
        let detail = cx.new(|cx| PatientDetail::new(store, id, window, cx));
        self.detail_subscription = Some(cx.subscribe_in(&detail, window, |this, _, event, window, cx| {
            match event {
                PatientDetailEvent::Changed => this.reload_list(window, cx),
                PatientDetailEvent::Deleted => {
                    this.detail = None;
                    this.reload_list(window, cx);
                    this.focus_search(window, cx);
                }
            }
            cx.emit(PatientsEvent::Changed);
        }));
        self.detail = Some(detail);
        cx.notify();
    }

    /// Selects a patient in the list (clearing the search if it hides the
    /// patient) and shows it.
    pub fn select_patient(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        if self.list.read(cx).delegate().position(id).is_none() {
            self.list.update(cx, |list, cx| list.set_query("", window, cx));
        }
        self.reselect(id, window, cx);
        self.show_patient(id.to_string(), window, cx);
    }

    fn reselect(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.list.update(cx, |list, cx| {
            let row = list.delegate().position(id);
            list.set_selected_index(row.map(IndexPath::new), window, cx);
            if row.is_some() {
                list.scroll_to_selected_item(window, cx);
            }
        });
    }

    /// Reloads the list after a change, keeping the selected patient.
    fn reload_list(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.list.update(cx, |list, cx| {
            list.delegate_mut().reload();
            cx.notify();
        });
        let selected = self.detail.as_ref().map(|d| d.read(cx).patient_id().to_string());
        match selected {
            Some(id) => self.reselect(&id, window, cx),
            None => self.list.update(cx, |list, cx| list.set_selected_index(None, window, cx)),
        }
        cx.notify();
    }

    fn set_order(&mut self, order: PatientOrder, window: &mut Window, cx: &mut Context<Self>) {
        self.list.update(cx, |list, _| list.delegate_mut().order = order);
        self.reload_list(window, cx);
    }

    pub fn new_patient(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let form = cx.new(|cx| ProfileForm::new(None, window, cx));
        self.form_subscription = Some(cx.subscribe_in(&form, window, |this, _, event, window, cx| {
            let ProfileFormEvent::Submit(profile) = event;
            match this.store.create_patient(profile, dates::today()) {
                Ok(patient) => {
                    window.close_dialog(cx);
                    window.push_notification(Notification::success("Patient added"), cx);
                    this.reload_list(window, cx);
                    this.select_patient(&patient.id, window, cx);
                    cx.emit(PatientsEvent::Changed);
                }
                Err(error) => window.push_notification(Notification::error(error.to_string()), cx),
            }
        }));

        let first_render = Cell::new(true);
        window.open_dialog(cx, move |dialog, window, cx| {
            if first_render.replace(false) {
                form.update(cx, |form, cx| form.focus(window, cx));
            }
            let save = form.clone();
            dialog
                .title("New Patient")
                .w(px(520.))
                .child(form.clone())
                .footer(
                    DialogFooter::new()
                        .child(
                            Button::new("new-patient-cancel")
                                .label("Cancel")
                                .on_click(|_, window, cx| window.close_dialog(cx)),
                        )
                        .child(Button::new("new-patient-save").primary().label("Save").on_click(
                            move |_, window, cx| save.update(cx, |form, cx| form.submit(window, cx)),
                        )),
                )
        });
    }

    fn render_list_pane(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let delegate = self.list.read(cx).delegate();
        let (count, order) = (delegate.patients.len(), delegate.order);
        let sort = |id: &'static str, icon: IconName, tooltip: &'static str, value: PatientOrder| {
            Button::new(id)
                .icon(icon)
                .ghost()
                .xsmall()
                .tooltip(tooltip)
                .selected(order == value)
        };
        v_flex()
            .size_full()
            .child(
                widgets::pane_header("Patients", Some(count), cx)
                    .child(
                        sort("sort-recent", IconName::Clock, "Most recent visit first", PatientOrder::RecentVisit)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.set_order(PatientOrder::RecentVisit, window, cx)
                            })),
                    )
                    .child(
                        sort("sort-name", IconName::ArrowDownAZ, "By name", PatientOrder::Name)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.set_order(PatientOrder::Name, window, cx)
                            })),
                    )
                    .child(
                        Button::new("new-patient")
                            .icon(IconName::UserPlus)
                            .ghost()
                            .xsmall()
                            .tooltip("New patient")
                            .on_click(cx.listener(|this, _, window, cx| this.new_patient(window, cx))),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_h(px(0.))
                    .child(List::new(&self.list).search_placeholder("Search patients")),
            )
    }
}

impl Render for PatientsView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let detail = match &self.detail {
            Some(detail) => detail.clone().into_any_element(),
            None => widgets::empty_state(
                IconName::Users,
                "No patient selected",
                "Choose a patient from the list, or add a new one.",
                cx,
            ),
        };
        h_resizable("patients-split")
            .child(
                resizable_panel()
                    .size(px(320.))
                    .size_range(px(240.)..px(560.))
                    .child(self.render_list_pane(cx)),
            )
            .child(resizable_panel().child(detail))
    }
}
