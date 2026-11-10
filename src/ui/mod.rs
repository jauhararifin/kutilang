//! The GPUI desktop interface.

mod app;
mod assets;
mod entry;
mod launcher;
mod patient_detail;
mod patients;
mod profile_form;
mod settings;
mod shell;
mod visit_editor;
mod visits;
mod widgets;

use std::path::PathBuf;

use gpui_kit::component::{Theme, TitleBar};
use gpui_kit::prelude::*;
use gpui_kit::{
    App, Bounds, KeyBinding, Menu, MenuItem, TitlebarOptions, WindowAppearance, WindowBounds,
    WindowOptions, px, size,
};

pub use app::{BRAND, KutilangApp, Section};
pub use assets::Assets;
pub use patient_detail::PatientDetail;
pub use patients::{PatientList, PatientsView};
pub use profile_form::ProfileForm;
pub use launcher::Launcher;
pub use settings::{SettingsEvent, SettingsView};
pub use shell::Shell;
pub use visit_editor::VisitEditor;
pub use visits::{VisitList, VisitsView};

use crate::recent::RecentDatabases;

gpui_kit::actions!(
    kutilang,
    [
        Quit,
        ShowPatients,
        ShowVisits,
        ShowSettings,
        NewPatient,
        BackUp,
        RestoreBackup,
        ExportToExcel,
        OpenDatabase,
        CloseDatabase
    ]
);

/// Base text size. The component library's default (16 px) is sized for web
/// pages; desktop applications use a smaller one.
const FONT_SIZE: f32 = 14.;

pub fn window_options(cx: &App) -> WindowOptions {
    WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
            None,
            size(px(1240.), px(820.)),
            cx,
        ))),
        titlebar: Some(TitlebarOptions {
            title: Some("Kutilang".into()),
            ..TitleBar::title_bar_options()
        }),
        window_min_size: Some(size(px(900.), px(560.))),
        ..TitleBar::window_options()
    }
}

/// Applies the light or dark theme, with the application's text size.
pub fn apply_theme(appearance: WindowAppearance, cx: &mut App) {
    Theme::change(appearance, None, cx);
    // Changing the theme reloads its sizes too.
    Theme::update(cx, |theme| theme.font_size = px(FONT_SIZE));
}

/// Initializes the components, the theme, the application menu and quitting.
pub fn init(cx: &mut App) {
    gpui_kit::init(cx);
    // Always light, whatever the system appearance.
    apply_theme(WindowAppearance::Light, cx);
    cx.on_action(|_: &Quit, cx| cx.quit());
    // Cmd on macOS, Ctrl elsewhere.
    macro_rules! shortcuts {
        ($modifier:literal) => {
            [
                KeyBinding::new(concat!($modifier, "-q"), Quit, None),
                KeyBinding::new(concat!($modifier, "-1"), ShowPatients, None),
                KeyBinding::new(concat!($modifier, "-2"), ShowVisits, None),
                KeyBinding::new(concat!($modifier, "-,"), ShowSettings, None),
                KeyBinding::new(concat!($modifier, "-n"), NewPatient, None),
                KeyBinding::new(concat!($modifier, "-o"), OpenDatabase, None),
            ]
        };
    }
    #[cfg(target_os = "macos")]
    cx.bind_keys(shortcuts!("cmd"));
    #[cfg(not(target_os = "macos"))]
    cx.bind_keys(shortcuts!("ctrl"));
    cx.set_menus([
        Menu::new("Kutilang").items([
            MenuItem::action("Settings…", ShowSettings),
            MenuItem::separator(),
            MenuItem::action("Quit Kutilang", Quit),
        ]),
        Menu::new("File").items([
            MenuItem::action("Open Database…", OpenDatabase),
            MenuItem::action("Close Database", CloseDatabase),
            MenuItem::separator(),
            MenuItem::action("New Patient…", NewPatient),
            MenuItem::separator(),
            MenuItem::action("Back Up…", BackUp),
            MenuItem::action("Restore…", RestoreBackup),
            MenuItem::action("Export to Excel…", ExportToExcel),
        ]),
        Menu::new("View").items([
            MenuItem::action("Patients", ShowPatients),
            MenuItem::action("Visits", ShowVisits),
        ]),
    ]);
    cx.on_window_closed(|cx, _| {
        if cx.windows().is_empty() {
            cx.quit();
        }
    })
    .detach();
}

/// Runs the application until its window is closed: opens `database`, or
/// shows the database picker.
pub fn run(database: Option<PathBuf>) {
    gpui_kit::application().with_assets(Assets).run(move |cx| {
        init(cx);
        let options = window_options(cx);
        let recent = RecentDatabases::load_default();
        let opened = gpui_kit::open_window(options, cx, |window, cx| {
            cx.new(|cx| Shell::new(recent, database, window, cx))
        });
        if let Err(error) = opened {
            eprintln!("kutilang: cannot open the window: {error:#}");
            cx.quit();
        }
        cx.activate(true);
    });
}
