//! The main window: title bar, sidebar and the current section.

use std::path::PathBuf;
use std::rc::Rc;

use gpui_kit::assets::IconName;
use gpui_kit::component::notification::Notification;
use gpui_kit::component::sidebar::{Sidebar, SidebarGroup, SidebarMenu, SidebarMenuItem};
use gpui_kit::component::{ActiveTheme as _, Icon, TitleBar, WindowExt as _, h_flex, v_flex};
use gpui_kit::prelude::*;
use gpui_kit::{Context, Entity, FontWeight, Subscription, Window, div, px};

use crate::i18n::{pick, t};
use crate::store::{MigrationReport, Store};
use crate::ui::patients::{PatientsEvent, PatientsView};
use crate::ui::settings::{SettingsEvent, SettingsView};
use crate::ui::visits::{VisitsEvent, VisitsView};
use crate::ui::widgets;
use crate::ui::{
    BackUp, ExportToExcel, NewPatient, RestoreBackup, ShowPatients, ShowSettings, ShowVisits,
};

/// Shown in the title bar, as in the original application.
pub const BRAND: &str = "MR Pasien dr Syamsul Hadi, SpPD";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Section {
    Patients,
    Visits,
    Settings,
}

pub enum AppEvent {
    /// Texts built when the views were made are in the old language.
    LanguageChanged,
}

impl gpui_kit::EventEmitter<AppEvent> for KutilangApp {}

pub struct KutilangApp {
    store: Rc<Store>,
    db_path: PathBuf,
    section: Section,
    patients: Entity<PatientsView>,
    visits: Entity<VisitsView>,
    settings: Entity<SettingsView>,
    patient_count: usize,
    visit_count: usize,
    /// Visits changed since the Visits section was last shown.
    visits_stale: bool,
    /// Subscriptions to the Patients and Visits views.
    record_subscriptions: Vec<Subscription>,
    _subscriptions: Vec<Subscription>,
}

/// The Patients and Visits views, and the app's subscriptions to them.
fn record_views(
    store: &Rc<Store>,
    window: &mut Window,
    cx: &mut Context<KutilangApp>,
) -> (Entity<PatientsView>, Entity<VisitsView>, Vec<Subscription>) {
    let patients = cx.new(|cx| PatientsView::new(store.clone(), window, cx));
    let visits = cx.new(|cx| VisitsView::new(store.clone(), window, cx));
    let subscriptions = vec![
        cx.subscribe_in(&patients, window, |this, _, event, _, cx| match event {
            PatientsEvent::Changed => {
                this.visits_stale = true;
                this.refresh_counts();
                cx.notify();
            }
        }),
        cx.subscribe_in(&visits, window, |this, _, event, window, cx| match event {
            VisitsEvent::OpenPatient(id) => this.open_patient(id, window, cx),
        }),
    ];
    (patients, visits, subscriptions)
}

impl KutilangApp {
    pub fn new(
        store: Rc<Store>,
        migration: Option<MigrationReport>,
        db_path: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let (patients, visits, record_subscriptions) = record_views(&store, window, cx);
        let settings = cx.new(|cx| SettingsView::new(store.clone(), db_path.clone(), window, cx));
        let subscriptions = vec![cx.subscribe_in(
            &settings,
            window,
            |this, _, event, window, cx| match event {
                SettingsEvent::DataReplaced => this.reload_records(window, cx),
                SettingsEvent::LanguageChanged => cx.emit(AppEvent::LanguageChanged),
            },
        )];

        if let Some(report) = migration {
            cx.defer_in(window, move |_, window, cx| {
                window.push_notification(migration_notice(&report), cx);
            });
        }
        patients.update(cx, |patients, cx| patients.focus_search(window, cx));

        let mut app = Self {
            store,
            db_path,
            section: Section::Patients,
            patients,
            visits,
            settings,
            patient_count: 0,
            visit_count: 0,
            visits_stale: false,
            record_subscriptions,
            _subscriptions: subscriptions,
        };
        app.refresh_counts();
        app
    }

    /// The open database file.
    pub fn db_path(&self) -> &PathBuf {
        &self.db_path
    }

    pub fn store(&self) -> &Rc<Store> {
        &self.store
    }

    pub fn section(&self) -> Section {
        self.section
    }

    pub fn patients(&self) -> &Entity<PatientsView> {
        &self.patients
    }

    pub fn visits(&self) -> &Entity<VisitsView> {
        &self.visits
    }

    pub fn counts(&self) -> (usize, usize) {
        (self.patient_count, self.visit_count)
    }

    pub fn settings(&self) -> &Entity<SettingsView> {
        &self.settings
    }

    /// Builds the Patients and Visits views again, after every record was
    /// replaced.
    fn reload_records(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (patients, visits, subscriptions) = record_views(&self.store, window, cx);
        self.patients = patients;
        self.visits = visits;
        self.record_subscriptions = subscriptions;
        self.visits_stale = false;
        self.refresh_counts();
        cx.notify();
    }

    fn refresh_counts(&mut self) {
        self.patient_count = self.store.count_patients().unwrap_or(0);
        self.visit_count = self.store.count_visits().unwrap_or(0);
    }

    pub fn show(&mut self, section: Section, window: &mut Window, cx: &mut Context<Self>) {
        self.section = section;
        match section {
            Section::Patients => self.patients.update(cx, |p, cx| p.focus_search(window, cx)),
            Section::Visits => {
                let stale = std::mem::take(&mut self.visits_stale);
                self.visits.update(cx, |visits, cx| {
                    if stale {
                        visits.reload(window, cx);
                    }
                    visits.focus_search(window, cx);
                });
            }
            Section::Settings => {}
        }
        cx.notify();
    }

    /// Shows a patient in the Patients section.
    pub fn open_patient(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.section = Section::Patients;
        self.patients
            .update(cx, |patients, cx| patients.select_patient(id, window, cx));
        cx.notify();
    }

    fn menu_item(
        &self,
        label: &'static str,
        icon: IconName,
        section: Section,
        count: Option<usize>,
        cx: &mut Context<Self>,
    ) -> SidebarMenuItem {
        let item = SidebarMenuItem::new(label)
            .icon(icon)
            .active(self.section == section)
            .on_click(cx.listener(move |this, _, window, cx| this.show(section, window, cx)));
        match count {
            Some(count) => item.suffix(move |_, cx| {
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(widgets::format_count(count))
            }),
            None => item,
        }
    }

    fn render_sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        Sidebar::new("sidebar")
            .w(px(210.))
            .child(
                SidebarGroup::new(t("Rekam Medis", "Medical Records")).child(
                    SidebarMenu::new()
                        .child(self.menu_item(
                            t("Pasien", "Patients"),
                            IconName::Users,
                            Section::Patients,
                            Some(self.patient_count),
                            cx,
                        ))
                        .child(self.menu_item(
                            t("Kunjungan", "Visits"),
                            IconName::Calendar,
                            Section::Visits,
                            Some(self.visit_count),
                            cx,
                        )),
                ),
            )
            .child(
                SidebarGroup::new(t("Aplikasi", "Application")).child(SidebarMenu::new().child(
                    self.menu_item(
                        t("Pengaturan", "Settings"),
                        IconName::Settings,
                        Section::Settings,
                        None,
                        cx,
                    ),
                )),
            )
    }

    fn render_title(&self, cx: &mut Context<Self>) -> impl IntoElement {
        h_flex()
            .gap_2()
            .text_sm()
            .child(Icon::new(IconName::Stethoscope).text_color(cx.theme().primary))
            .child(div().font_weight(FontWeight::SEMIBOLD).child("Kutilang"))
            .child(div().text_color(cx.theme().muted_foreground).child(BRAND))
            .child(div().text_color(cx.theme().muted_foreground).child("·"))
            .child(
                div().text_color(cx.theme().muted_foreground).child(
                    self.db_path
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default(),
                ),
            )
    }
}

fn migration_notice(report: &MigrationReport) -> Notification {
    let backup = report
        .backup_path
        .as_ref()
        .map(|path| {
            let path = path.display();
            pick(
                format!(" Salinan database lama disimpan sebagai {path}."),
                format!(" A copy of the old database was saved as {path}."),
            )
        })
        .unwrap_or_default();
    let (patients, visits) = (
        widgets::format_count(report.patients),
        widgets::format_count(report.visits),
    );
    Notification::info(pick(
        format!("Database diperbarui ke format baru ({patients} pasien, {visits} kunjungan).{backup}"),
        format!("The database was upgraded to the new format ({patients} patients, {visits} visits).{backup}"),
    ))
    .title(t("Database diperbarui", "Database upgraded"))
    .autohide(false)
}

impl Render for KutilangApp {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let content = match self.section {
            Section::Patients => self.patients.clone().into_any_element(),
            Section::Visits => self.visits.clone().into_any_element(),
            Section::Settings => self.settings.clone().into_any_element(),
        };
        v_flex()
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .on_action(cx.listener(|this, _: &ShowPatients, window, cx| {
                this.show(Section::Patients, window, cx)
            }))
            .on_action(cx.listener(|this, _: &ShowVisits, window, cx| {
                this.show(Section::Visits, window, cx)
            }))
            .on_action(cx.listener(|this, _: &ShowSettings, window, cx| {
                this.show(Section::Settings, window, cx)
            }))
            .on_action(cx.listener(|this, _: &BackUp, window, cx| {
                this.settings
                    .update(cx, |settings, cx| settings.back_up(window, cx))
            }))
            .on_action(cx.listener(|this, _: &RestoreBackup, window, cx| {
                this.settings
                    .update(cx, |settings, cx| settings.restore(window, cx))
            }))
            .on_action(cx.listener(|this, _: &ExportToExcel, window, cx| {
                this.settings
                    .update(cx, |settings, cx| settings.export(window, cx))
            }))
            .on_action(cx.listener(|this, _: &NewPatient, window, cx| {
                this.show(Section::Patients, window, cx);
                this.patients
                    .update(cx, |patients, cx| patients.new_patient(window, cx));
            }))
            .child(TitleBar::new().child(self.render_title(cx)))
            .child(
                h_flex()
                    .flex_1()
                    .min_h(px(0.))
                    .w_full()
                    .child(self.render_sidebar(cx))
                    .child(div().flex_1().min_w(px(0.)).h_full().child(content)),
            )
    }
}
