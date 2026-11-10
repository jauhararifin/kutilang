//! The GPUI desktop interface.

mod app;
mod assets;
mod entry;
mod patient_detail;
mod patients;
mod profile_form;
mod settings;
mod visit_editor;
mod visits;
mod widgets;

use std::path::PathBuf;
use std::rc::Rc;

use gpui_kit::component::{Theme, TitleBar};
use gpui_kit::prelude::*;
use gpui_kit::{
    App, Bounds, KeyBinding, Menu, MenuItem, TitlebarOptions, WindowAppearance, WindowBounds,
    WindowOptions, px, size,
};

pub use app::{BRAND, ErrorView, KutilangApp, Section};
pub use assets::Assets;
pub use patient_detail::PatientDetail;
pub use patients::{PatientList, PatientsView};
pub use profile_form::ProfileForm;
pub use settings::SettingsView;
pub use visit_editor::VisitEditor;
pub use visits::{VisitList, VisitsView};

use crate::store::Store;

gpui_kit::actions!(
    kutilang,
    [Quit, ShowPatients, ShowVisits, ShowSettings, NewPatient]
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
        Menu::new("File").items([MenuItem::action("New Patient…", NewPatient)]),
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

/// Opens the database at `db_path` (upgrading it if needed) and runs the
/// application until its window is closed.
pub fn run(db_path: PathBuf) {
    gpui_kit::application().with_assets(Assets).run(move |cx| {
        init(cx);
        let options = window_options(cx);
        let opened = match Store::open(&db_path) {
            Ok((store, report)) => {
                if let Some(report) = &report {
                    eprintln!("kutilang: upgraded {} ({report:?})", db_path.display());
                }
                let path = db_path.clone();
                gpui_kit::open_window(options, cx, |window, cx| {
                    cx.new(|cx| KutilangApp::new(Rc::new(store), report, path, window, cx))
                })
                .map(|_| ())
            }
            Err(error) => {
                eprintln!("kutilang: cannot open {}: {error:#}", db_path.display());
                let view = ErrorView {
                    path: db_path.display().to_string(),
                    message: format!("{error:#}"),
                };
                gpui_kit::open_window(options, cx, |_, cx| cx.new(|_| view)).map(|_| ())
            }
        };
        if let Err(error) = opened {
            eprintln!("kutilang: cannot open the window: {error:#}");
            cx.quit();
        }
        cx.activate(true);
    });
}
