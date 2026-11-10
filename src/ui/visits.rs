//! The Visits section: every visit of every patient, newest first.

use std::rc::Rc;

use chrono::{Datelike as _, Days, NaiveDate};
use gpui_kit::assets::IconName;
use gpui_kit::component::avatar::Avatar;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::date_picker::{DatePicker, DatePickerEvent, DatePickerState, DateRangePreset};
use gpui_kit::component::list::{List, ListDelegate, ListEvent, ListItem, ListState};
use gpui_kit::component::notification::Notification;
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::{
    ActiveTheme as _, IndexPath, Sizable as _, WindowExt as _, h_flex, h_resizable,
    resizable_panel, v_flex,
};
use gpui_kit::prelude::*;
use gpui_kit::{
    App, Context, Entity, EventEmitter, FontWeight, SharedString, Subscription, Task, Window, div,
    px,
};

use crate::dates;
use crate::model::{Patient, Visit, VisitSummary};
use crate::print;
use crate::store::Store;
use crate::ui::entry::entry_view;
use crate::ui::widgets;

pub enum VisitsEvent {
    OpenPatient(String),
}

/// The visits of one day.
struct Day {
    date: String,
    visits: Vec<VisitSummary>,
}

/// Every visit, grouped by day, filtered by patient name and dates.
pub struct VisitList {
    store: Rc<Store>,
    days: Vec<Day>,
    total: usize,
    query: String,
    from: Option<NaiveDate>,
    to: Option<NaiveDate>,
    error: Option<String>,
}

impl VisitList {
    fn reload(&mut self) {
        match self.store.list_visit_summaries(&self.query, self.from, self.to) {
            Ok(visits) => {
                self.total = visits.len();
                self.days.clear();
                for visit in visits {
                    match self.days.last_mut() {
                        Some(day) if day.date == visit.date => day.visits.push(visit),
                        _ => self.days.push(Day {
                            date: visit.date.clone(),
                            visits: vec![visit],
                        }),
                    }
                }
                self.error = None;
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    fn visit(&self, ix: IndexPath) -> Option<&VisitSummary> {
        self.days.get(ix.section)?.visits.get(ix.row)
    }

    fn position(&self, visit_id: &str) -> Option<IndexPath> {
        self.days.iter().enumerate().find_map(|(section, day)| {
            day.visits
                .iter()
                .position(|v| v.id == visit_id)
                .map(|row| IndexPath::new(row).section(section))
        })
    }

    pub fn total(&self) -> usize {
        self.total
    }

    /// The visits shown, newest first.
    pub fn visits(&self) -> impl Iterator<Item = &VisitSummary> {
        self.days.iter().flat_map(|day| day.visits.iter())
    }
}

impl ListDelegate for VisitList {
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

    fn sections_count(&self, _: &App) -> usize {
        self.days.len().max(1)
    }

    fn items_count(&self, section: usize, _: &App) -> usize {
        self.days.get(section).map_or(0, |day| day.visits.len())
    }

    fn render_section_header(
        &mut self,
        section: usize,
        _: &mut Window,
        cx: &mut Context<ListState<Self>>,
    ) -> Option<impl IntoElement> {
        let day = self.days.get(section)?;
        let title = dates::parse_stored(&day.date)
            .map(dates::format_with_weekday)
            .unwrap_or_else(|| day.date.clone());
        Some(
            h_flex()
                .w_full()
                .px_3()
                .pt_3()
                .pb_1()
                .gap_2()
                .text_xs()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(cx.theme().muted_foreground)
                .child(title)
                .child(div().flex_1())
                .child(widgets::format_count(day.visits.len())),
        )
    }

    fn render_item(
        &mut self,
        ix: IndexPath,
        _: &mut Window,
        cx: &mut Context<ListState<Self>>,
    ) -> Option<ListItem> {
        let visit = self.visit(ix)?;
        let name = if visit.head_family_name.trim().is_empty() {
            visit.patient_name.clone()
        } else {
            format!("{} (KK {})", visit.patient_name, visit.head_family_name.trim())
        };
        let summary = if visit.summary.is_empty() { " ".to_string() } else { visit.summary.clone() };
        Some(
            ListItem::new(SharedString::from(format!("visit-{}", visit.id))).child(
                v_flex()
                    .w_full()
                    .min_w(px(0.))
                    .child(div().w_full().truncate().font_weight(FontWeight::MEDIUM).child(name))
                    .child(
                        div()
                            .w_full()
                            .truncate()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(summary),
                    ),
            ),
        )
    }

    fn set_selected_index(&mut self, _: Option<IndexPath>, _: &mut Window, _: &mut Context<ListState<Self>>) {}

    fn render_empty(&mut self, _: &mut Window, cx: &mut Context<ListState<Self>>) -> impl IntoElement {
        let (title, hint) = match &self.error {
            Some(error) => ("Cannot list the visits", error.clone()),
            None => ("No visits", "Nothing matches the name and dates.".to_string()),
        };
        widgets::empty_state(IconName::Calendar, title, hint, cx)
    }
}

/// The selected visit, with its patient.
struct Selected {
    visit: Visit,
    patient: Patient,
}

pub struct VisitsView {
    store: Rc<Store>,
    list: Entity<ListState<VisitList>>,
    range: Entity<DatePickerState>,
    selected: Option<Selected>,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<VisitsEvent> for VisitsView {}

impl VisitsView {
    pub fn new(store: Rc<Store>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut delegate = VisitList {
            store: store.clone(),
            days: Vec::new(),
            total: 0,
            query: String::new(),
            from: None,
            to: None,
            error: None,
        };
        delegate.reload();
        let list = cx.new(|cx| ListState::new(delegate, window, cx).searchable(true));
        let range = cx.new(|cx| DatePickerState::range(window, cx).date_format("%d/%m/%Y"));

        let subscriptions = vec![
            cx.subscribe_in(&list, window, |this, list, event, window, cx| match event {
                ListEvent::Select(ix) | ListEvent::Confirm(ix) => {
                    let id = list.read(cx).delegate().visit(*ix).map(|v| v.id.clone());
                    if let Some(id) = id {
                        this.show_visit(&id, window, cx);
                    }
                }
                ListEvent::Cancel => {}
            }),
            cx.subscribe_in(&range, window, |this, _, event, window, cx| {
                let DatePickerEvent::Change(value) = event;
                let from = value.start().map(|d| d.date());
                let to = value.end().map(|d| d.date()).or(from);
                this.set_dates(from, to, window, cx);
            }),
        ];

        Self {
            store,
            list,
            range,
            selected: None,
            _subscriptions: subscriptions,
        }
    }

    pub fn list(&self) -> &Entity<ListState<VisitList>> {
        &self.list
    }

    pub fn focus_search(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.list.update(cx, |list, cx| list.focus(window, cx));
    }

    /// Reloads the list, after visits changed elsewhere.
    pub fn reload(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.list.update(cx, |list, cx| {
            list.delegate_mut().reload();
            cx.notify();
        });
        // The selected visit may have changed or been deleted.
        if let Some(id) = self.selected.as_ref().map(|s| s.visit.id.clone()) {
            self.selected = None;
            self.show_visit(&id, window, cx);
        }
        self.reselect(window, cx);
        cx.notify();
    }

    /// Shows the visits between `from` and `to` (inclusive), or all visits.
    pub fn set_dates(
        &mut self,
        from: Option<NaiveDate>,
        to: Option<NaiveDate>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.list.update(cx, |list, cx| {
            let delegate = list.delegate_mut();
            delegate.from = from;
            delegate.to = to;
            delegate.reload();
            cx.notify();
        });
        self.reselect(window, cx);
        cx.notify();
    }

    pub fn show_visit(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let loaded = self.store.get_visit(id).and_then(|visit| {
            let patient = match &visit {
                Some(visit) => self.store.get_patient(&visit.patient_id)?,
                None => None,
            };
            Ok(visit.zip(patient))
        });
        match loaded {
            Ok(selected) => {
                self.selected = selected.map(|(visit, patient)| Selected { visit, patient });
            }
            Err(error) => window.push_notification(Notification::error(error.to_string()), cx),
        }
        // Keep the list's highlight on the visit shown.
        self.reselect(window, cx);
        cx.notify();
    }

    fn reselect(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let selected = self.selected.as_ref().map(|s| s.visit.id.clone());
        self.list.update(cx, |list, cx| {
            let ix = selected.and_then(|id| list.delegate().position(&id));
            list.set_selected_index(ix, window, cx);
        });
    }

    fn print(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(selected) = &self.selected else {
            return;
        };
        match print::write_visit_page(&selected.patient, &selected.visit) {
            Ok(path) => cx.open_url(&print::file_url(&path)),
            Err(error) => {
                window.push_notification(Notification::error(format!("Cannot print: {error}")), cx)
            }
        }
    }

    fn render_list_pane(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let total = self.list.read(cx).delegate().total;
        let today = dates::today();
        let days_ago = |days: u64| today.checked_sub_days(Days::new(days)).unwrap_or(today);
        let month_start = today.with_day(1).unwrap_or(today);
        let year_start = today.with_ordinal(1).unwrap_or(today);
        let presets = vec![
            DateRangePreset::range("Hari ini", today, today),
            DateRangePreset::range("7 hari terakhir", days_ago(6), today),
            DateRangePreset::range("30 hari terakhir", days_ago(29), today),
            DateRangePreset::range("Bulan ini", month_start, today),
            DateRangePreset::range("Tahun ini", year_start, today),
        ];
        v_flex()
            .size_full()
            .child(widgets::pane_header("Visits", Some(total), cx))
            .child(
                div().w_full().flex_none().px_3().pt_2().child(
                    DatePicker::new(&self.range)
                        .small()
                        .cleanable(true)
                        .placeholder("All dates")
                        .presets(presets),
                ),
            )
            .child(
                div()
                    .flex_1()
                    .min_h(px(0.))
                    .child(List::new(&self.list).search_placeholder("Filter by patient")),
            )
    }

    fn render_detail(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(selected) = &self.selected else {
            return widgets::empty_state(
                IconName::Calendar,
                "No visit selected",
                "Choose a visit from the list.",
                cx,
            );
        };
        let patient = &selected.patient;
        let today = dates::today();
        let mut subtitle = Vec::new();
        if let Some(birth) = dates::parse_stored(&patient.birth_date) {
            subtitle.push(format!("{} tahun", dates::age_years(birth, today)));
        }
        if !patient.head_family_name.trim().is_empty() {
            subtitle.push(format!("KK {}", patient.head_family_name.trim()));
        }
        if !patient.address.trim().is_empty() {
            subtitle.push(patient.address.trim().to_string());
        }
        let patient_id = patient.id.clone();

        v_flex()
            .size_full()
            .child(
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
                        Button::new("print-visit")
                            .ghost()
                            .small()
                            .icon(IconName::Printer)
                            .tooltip("Print")
                            .on_click(cx.listener(|this, _, window, cx| this.print(window, cx))),
                    )
                    .child(
                        Button::new("open-patient")
                            .small()
                            .icon(IconName::User)
                            .label("Open Patient")
                            .on_click(cx.listener(move |_, _, _, cx| {
                                cx.emit(VisitsEvent::OpenPatient(patient_id.clone()))
                            })),
                    ),
            )
            .child(
                div()
                    .id(SharedString::from(format!("visit-scroll-{}", selected.visit.id)))
                    .flex_1()
                    .min_h(px(0.))
                    .w_full()
                    .overflow_y_scrollbar()
                    .child(
                        div()
                            .px_5()
                            .py_4()
                            .child(entry_view("visits", &selected.visit, [], cx)),
                    ),
            )
            .into_any_element()
    }
}

impl Render for VisitsView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        h_resizable("visits-split")
            .child(
                resizable_panel()
                    .size(px(360.))
                    .size_range(px(260.)..px(600.))
                    .child(self.render_list_pane(cx)),
            )
            .child(resizable_panel().child(self.render_detail(cx)))
    }
}
