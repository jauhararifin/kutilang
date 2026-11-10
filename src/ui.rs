//! The GPUI desktop interface: the window's shell and database picker, the
//! Patients, Visits and Settings sections, the visit editor and the shared
//! widgets.

use crate::diagnosis::TEMPLATE;
use crate::store::{Backup, MigrationReport, SCHEMA_VERSION, Store, timestamped_path};
use crate::{
    Cursor, DiagnosisFormat, Language, Patient, PatientErrors, PatientInput, PatientOrder,
    PatientProfile, RecentDatabases, Visit, VisitSummary, age_years, app_dir, default_database,
    diagnosis, display_with_age, display_with_relative, format_age, format_age_short,
    format_relative, format_short, format_with_weekday, language, load_preference, parse_stored,
    pick, save_preference, set_language, store, t, to_stored, today,
};
use chrono::{Datelike as _, Days, NaiveDate};
use gpui_kit::assets::IconName;
use gpui_kit::component::avatar::Avatar;
use gpui_kit::component::button::{Button, ButtonVariant, ButtonVariants as _};
use gpui_kit::component::calendar::Date;
use gpui_kit::component::date_picker::{
    DatePicker, DatePickerEvent, DatePickerState, DateRangePreset,
};
use gpui_kit::component::description_list::DescriptionList;
use gpui_kit::component::dialog::DialogFooter;
use gpui_kit::component::form::{field, v_form};
use gpui_kit::component::group_box::{GroupBox, GroupBoxVariants as _};
use gpui_kit::component::input::{Input, InputEvent, InputState, Textarea, TextareaState};
use gpui_kit::component::list::{List, ListDelegate, ListEvent, ListItem, ListState};
use gpui_kit::component::notification::Notification;
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::sidebar::{Sidebar, SidebarGroup, SidebarMenu, SidebarMenuItem};
use gpui_kit::component::text::TextView;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Icon, IndexPath, Selectable as _, Sizable as _, Theme,
    TitleBar, WindowExt as _, h_flex, h_resizable, resizable_panel, v_flex,
};
use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, App, AssetSource, Bounds, Context, Div, Entity, EventEmitter, FocusHandle,
    FontWeight, KeyBinding, Menu, MenuItem, PathPromptOptions, Result, SharedString, Subscription,
    Task, TitlebarOptions, WeakEntity, Window, WindowAppearance, WindowBounds, WindowOptions, div,
    px, size,
};
use std::borrow::Cow;
use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;

// ============================================================================
// Actions, theme, menus and starting the application.
// ============================================================================

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
    set_language(load_preference());
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
    set_menus(cx);
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
    diagnosis::remove_old_print_pages();
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

/// The application menu, in the current language.
pub fn set_menus(cx: &mut App) {
    cx.set_menus([
        Menu::new("Kutilang").items([
            MenuItem::action(t("Pengaturan…", "Settings…"), ShowSettings),
            MenuItem::separator(),
            MenuItem::action(t("Keluar dari Kutilang", "Quit Kutilang"), Quit),
        ]),
        Menu::new(t("Berkas", "File")).items([
            MenuItem::action(t("Buka Database…", "Open Database…"), OpenDatabase),
            MenuItem::action(t("Tutup Database", "Close Database"), CloseDatabase),
            MenuItem::separator(),
            MenuItem::action(t("Pasien Baru…", "New Patient…"), NewPatient),
            MenuItem::separator(),
            MenuItem::action(t("Cadangkan…", "Back Up…"), BackUp),
            MenuItem::action(t("Pulihkan…", "Restore…"), RestoreBackup),
            MenuItem::action(t("Ekspor ke Excel…", "Export to Excel…"), ExportToExcel),
        ]),
        Menu::new(t("Tampilan", "View")).items([
            MenuItem::action(t("Pasien", "Patients"), ShowPatients),
            MenuItem::action(t("Kunjungan", "Visits"), ShowVisits),
        ]),
    ]);
}

// ============================================================================
// The window's root: the database picker, or the open database.
// ============================================================================

enum Content {
    Launcher {
        view: Entity<Launcher>,
        _subscription: Subscription,
    },
    App(Entity<KutilangApp>),
}

pub struct Shell {
    recent: Rc<RefCell<RecentDatabases>>,
    content: Content,
    /// Focused when no field is, so that the shortcuts and menus keep
    /// working: actions only reach the views around the focused element.
    focus: FocusHandle,
    _focus_lost: Subscription,
}

impl Shell {
    /// Opens `database` straight away, or shows the database picker.
    pub fn new(
        recent: RecentDatabases,
        database: Option<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let recent = Rc::new(RefCell::new(recent));
        let content = Self::launcher(&recent, window, cx);
        // A closed editor or a deleted patient takes the focus with it.
        let focus_lost =
            cx.on_focus_lost(window, |shell, window, cx| shell.restore_focus(window, cx));
        // Closing the window asks first when there are unsaved changes.
        let this = cx.weak_entity();
        window.on_window_should_close(cx, move |window, cx| {
            let unsaved = this
                .read_with(cx, |shell, cx| shell.has_unsaved_changes(cx))
                .unwrap_or(false);
            if unsaved {
                confirm_discard(window, cx, |window, _| window.remove_window());
            }
            !unsaved
        });
        // Notifications need the window to be set up.
        cx.defer_in(window, move |shell, window, cx| match database {
            Some(path) => shell.open(&path, window, cx),
            None => shell.restore_focus(window, cx),
        });
        Self {
            recent,
            content,
            focus: cx.focus_handle(),
            _focus_lost: focus_lost,
        }
    }

    fn launcher(
        recent: &Rc<RefCell<RecentDatabases>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Content {
        let launcher = cx.new(|_| Launcher::new(recent.clone()));
        let subscription =
            cx.subscribe_in(
                &launcher,
                window,
                |shell, _, event, window, cx| match event {
                    LauncherEvent::Open(path) => shell.open(path, window, cx),
                },
            );
        Content::Launcher {
            view: launcher,
            _subscription: subscription,
        }
    }

    /// Opens a database (creating it if needed), replacing the one that is
    /// open. On failure nothing changes, and the reason is shown.
    pub fn open(&mut self, path: &Path, window: &mut Window, cx: &mut Context<Self>) {
        match Store::open(path) {
            Ok((store, report)) => {
                if let Some(report) = &report {
                    eprintln!("kutilang: upgraded {} ({report:?})", path.display());
                }
                self.recent.borrow_mut().add(path).ok();
                let path = path.to_path_buf();
                let app = cx.new(|cx| KutilangApp::new(Rc::new(store), report, path, window, cx));
                self.content = Content::App(app);
            }
            Err(error) => {
                window.push_notification(
                    Notification::error({
                        let path = path.display();
                        pick(
                            format!("{path} tidak dapat dibuka: {error:#}"),
                            format!("Cannot open {path}: {error:#}"),
                        )
                    })
                    .title(t("Tidak ada yang diubah", "Nothing was changed"))
                    .autohide(false),
                    cx,
                );
            }
        }
        cx.notify();
    }

    /// Asks for a database file and opens it.
    pub fn choose_database(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let chosen = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(t("Buka", "Open").into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(paths))) = chosen.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            this.update_in(cx, |shell, window, cx| {
                shell.after_confirming(window, cx, move |shell, window, cx| {
                    shell.open(&path, window, cx)
                })
            })
            .ok();
        })
        .detach();
    }

    /// Closes the open database and shows the picker.
    pub fn show_launcher(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Content::App(_) = self.content {
            self.content = Self::launcher(&self.recent, window, cx);
            self.focus.focus(window, cx);
            cx.notify();
        }
    }

    pub fn app(&self) -> Option<&Entity<KutilangApp>> {
        match &self.content {
            Content::App(app) => Some(app),
            Content::Launcher { .. } => None,
        }
    }

    pub fn launcher_view(&self) -> Option<&Entity<Launcher>> {
        match &self.content {
            Content::Launcher { view, .. } => Some(view),
            Content::App(_) => None,
        }
    }

    /// Whether text was written that is not saved yet.
    pub fn has_unsaved_changes(&self, cx: &App) -> bool {
        self.app()
            .is_some_and(|app| app.read(cx).has_unsaved_changes(cx))
    }

    /// Runs `action` now, or after the user agreed to lose unsaved changes.
    fn after_confirming(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        action: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
    ) {
        if self.has_unsaved_changes(cx) {
            confirm_then(cx.weak_entity(), window, cx, action);
        } else {
            action(self, window, cx);
        }
    }

    fn restore_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match &self.content {
            Content::App(app) => app.read(cx).focus_handle.clone().focus(window, cx),
            Content::Launcher { .. } => self.focus.focus(window, cx),
        }
    }

    fn update_app(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        f: impl FnOnce(&mut KutilangApp, &mut Window, &mut Context<KutilangApp>),
    ) {
        if let Some(app) = self.app().cloned() {
            app.update(cx, |app, cx| f(app, window, cx));
        }
    }
}

impl Render for Shell {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (content, has_app) = match &self.content {
            Content::Launcher { view, .. } => (view.clone().into_any_element(), false),
            Content::App(app) => (app.clone().into_any_element(), true),
        };
        // The actions are handled here, around everything, so that they work
        // whatever is focused; those of an open database only while it is.
        div()
            .size_full()
            .track_focus(&self.focus)
            .on_action(cx.listener(|shell, _: &Quit, window, cx| {
                shell.after_confirming(window, cx, |_, _, cx| cx.quit())
            }))
            .on_action(
                cx.listener(|shell, _: &OpenDatabase, window, cx| {
                    shell.choose_database(window, cx)
                }),
            )
            .when(has_app, |this| {
                this.on_action(cx.listener(|shell, _: &CloseDatabase, window, cx| {
                    shell.after_confirming(window, cx, |shell, window, cx| {
                        shell.show_launcher(window, cx)
                    })
                }))
                .on_action(cx.listener(|shell, _: &ShowPatients, window, cx| {
                    shell.update_app(window, cx, |app, window, cx| {
                        app.show(Section::Patients, window, cx)
                    })
                }))
                .on_action(cx.listener(|shell, _: &ShowVisits, window, cx| {
                    shell.update_app(window, cx, |app, window, cx| {
                        app.show(Section::Visits, window, cx)
                    })
                }))
                .on_action(cx.listener(|shell, _: &ShowSettings, window, cx| {
                    shell.update_app(window, cx, |app, window, cx| {
                        app.show(Section::Settings, window, cx)
                    })
                }))
                .on_action(cx.listener(|shell, _: &NewPatient, window, cx| {
                    shell.update_app(window, cx, |app, window, cx| app.new_patient(window, cx))
                }))
                .on_action(cx.listener(|shell, _: &BackUp, window, cx| {
                    shell.update_app(window, cx, |app, window, cx| {
                        app.settings
                            .update(cx, |settings, cx| settings.back_up(window, cx))
                    })
                }))
                .on_action(cx.listener(|shell, _: &RestoreBackup, window, cx| {
                    shell.update_app(window, cx, |app, window, cx| {
                        app.settings
                            .update(cx, |settings, cx| settings.restore(window, cx))
                    })
                }))
                .on_action(cx.listener(
                    |shell, _: &ExportToExcel, window, cx| {
                        shell.update_app(window, cx, |app, window, cx| {
                            app.settings
                                .update(cx, |settings, cx| settings.export(window, cx))
                        })
                    },
                ))
            })
            .child(content)
    }
}

/// Asks before losing changes that were not saved; runs `discard` when the
/// user agrees.
fn confirm_discard(
    window: &mut Window,
    cx: &mut App,
    discard: impl Fn(&mut Window, &mut App) + 'static,
) {
    let discard = Rc::new(discard);
    window.open_alert_dialog(cx, move |alert, _, _| {
        let discard = discard.clone();
        alert
            .title(t("Buang perubahan?", "Discard changes?"))
            .description(t(
                "Ada tulisan yang belum disimpan. Tulisan itu akan hilang.",
                "Some text was not saved. It will be lost.",
            ))
            .show_cancel(true)
            .cancel_text(t("Batal", "Cancel"))
            .ok_text(t("Buang", "Discard"))
            .ok_variant(ButtonVariant::Danger)
            .on_ok(move |_, window, cx| {
                discard(window, cx);
                true
            })
    });
}

/// [`confirm_discard`], then `action` on the view.
fn confirm_then<T: 'static>(
    this: WeakEntity<T>,
    window: &mut Window,
    cx: &mut App,
    action: impl Fn(&mut T, &mut Window, &mut Context<T>) + 'static,
) {
    confirm_discard(window, cx, move |window, cx| {
        this.update(cx, |this, cx| action(this, window, cx)).ok();
    });
}

// ============================================================================
// The first screen: choose the database to open, from the recent ones or a
// file.
// ============================================================================

pub enum LauncherEvent {
    /// Open this database, creating it if it does not exist.
    Open(PathBuf),
}

pub struct Launcher {
    recent: Rc<RefCell<RecentDatabases>>,
}

impl EventEmitter<LauncherEvent> for Launcher {}

/// A row of the list: a remembered database, or the original
/// application's default one.
struct Row {
    path: PathBuf,
    opened: Option<String>,
    default: bool,
}

impl Launcher {
    pub fn new(recent: Rc<RefCell<RecentDatabases>>) -> Self {
        Self { recent }
    }

    fn rows(&self) -> Vec<Row> {
        let today = today();
        let mut rows: Vec<Row> = self
            .recent
            .borrow()
            .entries()
            .iter()
            .map(|entry| Row {
                path: entry.path.clone(),
                opened: Some({
                    let when = format_relative(entry.opened_at.date_naive(), today);
                    pick(format!("dibuka {when}"), format!("opened {when}"))
                }),
                default: false,
            })
            .collect();
        let default = default_database();
        let listed = rows.iter().any(|row| {
            row.path == default || default.canonicalize().is_ok_and(|real| real == row.path)
        });
        if default.exists() && !listed {
            rows.push(Row {
                path: default,
                opened: None,
                default: true,
            });
        }
        rows
    }

    fn open(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        cx.emit(LauncherEvent::Open(path));
    }

    fn forget(&mut self, path: &Path, cx: &mut Context<Self>) {
        self.recent.borrow_mut().remove(path).ok();
        cx.notify();
    }

    pub fn choose_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let chosen = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(t("Buka", "Open").into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(paths))) = chosen.await else {
                return;
            };
            if let Some(path) = paths.into_iter().next() {
                this.update(cx, |this, cx| this.open(path, cx)).ok();
            }
        })
        .detach();
    }

    pub fn new_database(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let folder = dirs::document_dir().unwrap_or_else(app_dir);
        let chosen = cx.prompt_for_new_path(&folder, Some("kutilang.sqlite3"));
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(path))) = chosen.await else {
                return;
            };
            this.update_in(cx, |this, window, cx| {
                // Never start a new database over an existing file.
                let exists = std::fs::metadata(&path).is_ok_and(|m| m.len() > 0);
                if exists {
                    window.push_notification(
                        Notification::error({
                            let path = path.display();
                            pick(
                                format!("{path} sudah ada. Pilih nama lain, atau buka dengan Buka Database."),
                                format!("{path} already exists. Choose another name, or open it with Open Database."),
                            )
                        }),
                        cx,
                    );
                } else {
                    this.open(path, cx);
                }
            })
            .ok();
        })
        .detach();
    }

    fn render_row(&self, index: usize, row: Row, cx: &mut Context<Self>) -> AnyElement {
        let exists = row.path.exists();
        let name = row
            .path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| row.path.display().to_string());
        let folder = row
            .path
            .parent()
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        let note: SharedString = if !exists {
            t("Berkas tidak ditemukan", "File not found").into()
        } else if row.default {
            t(
                "Database aplikasi lama",
                "Database of the original application",
            )
            .into()
        } else {
            row.opened.clone().unwrap_or_default().into()
        };
        let theme = cx.theme();
        let (hover, muted, danger, border) = (
            theme.list_hover,
            theme.muted_foreground,
            theme.danger,
            theme.border,
        );
        let path = row.path.clone();
        let forget = row.path.clone();

        h_flex()
            .id(("recent", index))
            .w_full()
            .gap_3()
            .px_4()
            .py_2p5()
            .when(index > 0, |row| row.border_t_1().border_color(border))
            .when(exists, |row| {
                row.cursor_pointer()
                    .hover(move |row| row.bg(hover))
                    .on_click(cx.listener(move |this, _, _, cx| this.open(path.clone(), cx)))
            })
            .child(Icon::new(IconName::Database).text_color(if exists {
                theme.primary
            } else {
                muted
            }))
            .child(
                v_flex()
                    .flex_1()
                    .min_w(px(0.))
                    .child(
                        div()
                            .truncate()
                            .font_weight(FontWeight::MEDIUM)
                            .when(!exists, |name| name.text_color(muted))
                            .child(name),
                    )
                    .child(div().truncate().text_xs().text_color(muted).child(folder)),
            )
            .child(
                div()
                    .flex_none()
                    .text_xs()
                    .text_color(if exists { muted } else { danger })
                    .child(note),
            )
            .when(!row.default, |row| {
                row.child(
                    Button::new(("forget", index))
                        .icon(IconName::Close)
                        .ghost()
                        .xsmall()
                        .tooltip(t("Hapus dari daftar", "Remove from the list"))
                        .on_click(cx.listener(move |this, _, _, cx| this.forget(&forget, cx))),
                )
            })
            .into_any_element()
    }
}

impl Render for Launcher {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let rows: Vec<AnyElement> = self
            .rows()
            .into_iter()
            .enumerate()
            .map(|(index, row)| self.render_row(index, row, cx))
            .collect();
        let list = if rows.is_empty() {
            div()
                .w_full()
                .px_4()
                .py_6()
                .text_center()
                .child(
                    muted(
                        t(
                            "Belum ada database yang dibuka.",
                            "No databases opened yet.",
                        ),
                        cx,
                    )
                    .text_sm(),
                )
                .into_any_element()
        } else {
            v_flex().w_full().children(rows).into_any_element()
        };

        v_flex()
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(
                TitleBar::new().child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child("Kutilang"),
                ),
            )
            .child(
                div()
                    .id("launcher-scroll")
                    .flex_1()
                    .min_h(px(0.))
                    .overflow_y_scrollbar()
                    .child(
                        v_flex().w_full().items_center().py_12().child(
                            v_flex()
                                .w(px(580.))
                                .gap_6()
                                .child(
                                    h_flex()
                                        .gap_3()
                                        .child(
                                            Icon::new(IconName::Stethoscope)
                                                .size(px(36.))
                                                .text_color(cx.theme().primary),
                                        )
                                        .child(
                                            v_flex()
                                                .child(
                                                    div()
                                                        .text_2xl()
                                                        .font_weight(FontWeight::SEMIBOLD)
                                                        .child("Kutilang"),
                                                )
                                                .child(muted(BRAND, cx)),
                                        ),
                                )
                                .child(
                                    v_flex()
                                        .gap_2()
                                        .child(
                                            div()
                                                .font_weight(FontWeight::SEMIBOLD)
                                                .child(t("Database Terakhir", "Recent Databases")),
                                        )
                                        .child(
                                            div()
                                                .w_full()
                                                .border_1()
                                                .border_color(cx.theme().border)
                                                .rounded(cx.theme().radius)
                                                .overflow_hidden()
                                                .child(list),
                                        ),
                                )
                                .child(
                                    h_flex()
                                        .gap_2()
                                        .child(
                                            Button::new("open-database")
                                                .primary()
                                                .icon(IconName::FolderOpen)
                                                .label(t("Buka Database…", "Open Database…"))
                                                .on_click(cx.listener(|this, _, window, cx| {
                                                    this.choose_file(window, cx)
                                                })),
                                        )
                                        .child(
                                            Button::new("new-database")
                                                .icon(IconName::Plus)
                                                .label(t("Database Baru…", "New Database…"))
                                                .on_click(cx.listener(|this, _, window, cx| {
                                                    this.new_database(window, cx)
                                                })),
                                        ),
                                ),
                        ),
                    ),
            )
    }
}

// ============================================================================
// The main window: title bar, sidebar and the current section.
// ============================================================================

/// Shown in the title bar, as in the original application.
pub const BRAND: &str = "MR Pasien dr Syamsul Hadi, SpPD";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Section {
    Patients,
    Visits,
    Settings,
}

pub struct KutilangApp {
    store: Rc<Store>,
    /// Focused when the section has no field to focus.
    focus_handle: FocusHandle,
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
            focus_handle: cx.focus_handle(),
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

    /// Whether text was written that is not saved yet: a visit, or the
    /// template.
    pub fn has_unsaved_changes(&self, cx: &App) -> bool {
        self.patients.read(cx).has_unsaved_changes(cx)
            || self.settings.read(cx).has_unsaved_changes(cx)
    }

    pub fn new_patient(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.show(Section::Patients, window, cx);
        self.patients
            .update(cx, |patients, cx| patients.new_patient(window, cx));
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
            Section::Settings => self.focus_handle.focus(window, cx),
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
                    .child(format_count(count))
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
    let (patients, visits) = (format_count(report.patients), format_count(report.visits));
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
            .track_focus(&self.focus_handle)
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

// ============================================================================
// The Patients section: the list of all patients and the selected one.
// ============================================================================

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
        let today = today();
        let mut details = Vec::new();
        if let Some(birth) = parse_stored(&patient.birth_date) {
            details.push(format_age_short(age_years(birth, today)));
        }
        if !patient.head_family_name.trim().is_empty() {
            details.push(format!("KK {}", patient.head_family_name.trim()));
        }
        if !patient.address.trim().is_empty() {
            details.push(patient.address.trim().to_string());
        }
        let last_visit = parse_stored(&patient.last_visit)
            .map(format_short)
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
                            .child(
                                div()
                                    .flex_none()
                                    .text_xs()
                                    .text_color(muted)
                                    .child(last_visit),
                            ),
                    )
                    .child(div().w_full().truncate().text_xs().text_color(muted).child(
                        if details.is_empty() {
                            " ".to_string()
                        } else {
                            details.join(" · ")
                        },
                    )),
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

    fn render_empty(
        &mut self,
        _: &mut Window,
        cx: &mut Context<ListState<Self>>,
    ) -> impl IntoElement {
        let (title, hint) = match &self.error {
            Some(error) => (
                t("Pasien tidak dapat ditampilkan", "Cannot list the patients"),
                error.clone(),
            ),
            None => (
                t("Pasien tidak ditemukan", "No patients found"),
                t("Coba nama lain.", "Try another name.").to_string(),
            ),
        };
        empty_state(IconName::Users, title, hint, cx)
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
        let subscription =
            cx.subscribe_in(&list, window, |this, list, event, window, cx| match event {
                ListEvent::Select(ix) | ListEvent::Confirm(ix) => {
                    let id = list
                        .read(cx)
                        .delegate()
                        .patients
                        .get(ix.row)
                        .map(|p| p.id.clone());
                    if let Some(id) = id {
                        this.request_patient(id, window, cx);
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
        self.list
            .update(cx, |list, cx| list.set_query(query, window, cx));
    }

    /// Whether a visit being written for the shown patient is not saved.
    pub fn has_unsaved_changes(&self, cx: &App) -> bool {
        self.detail
            .as_ref()
            .is_some_and(|detail| detail.read(cx).has_unsaved_changes(cx))
    }

    fn shown_patient(&self, cx: &App) -> Option<String> {
        self.detail
            .as_ref()
            .map(|detail| detail.read(cx).patient_id().to_string())
    }

    /// Shows the patient chosen in the list, asking first when that would
    /// lose a visit being written.
    fn request_patient(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
        let shown = self.shown_patient(cx);
        if shown.as_deref() == Some(id.as_str()) {
            return;
        }
        if self.has_unsaved_changes(cx) {
            // Keep the highlight on the patient shown until the user decides.
            if let Some(shown) = shown {
                self.reselect(&shown, window, cx);
            }
            confirm_then(cx.weak_entity(), window, cx, move |this, window, cx| {
                this.reselect(&id, window, cx);
                this.show_patient(id.clone(), window, cx);
            });
            return;
        }
        self.show_patient(id, window, cx);
    }

    fn show_patient(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
        if self
            .detail
            .as_ref()
            .is_some_and(|d| d.read(cx).patient_id() == id)
        {
            return;
        }
        let store = self.store.clone();
        let detail = cx.new(|cx| PatientDetail::new(store, id, window, cx));
        self.detail_subscription =
            Some(
                cx.subscribe_in(&detail, window, |this, _, event, window, cx| {
                    match event {
                        PatientDetailEvent::Changed => this.reload_list(window, cx),
                        PatientDetailEvent::Deleted => {
                            this.detail = None;
                            this.reload_list(window, cx);
                            this.focus_search(window, cx);
                        }
                    }
                    cx.emit(PatientsEvent::Changed);
                }),
            );
        self.detail = Some(detail);
        cx.notify();
    }

    /// Selects a patient in the list (clearing the search if it hides the
    /// patient) and shows it, asking first when that would lose a visit
    /// being written.
    pub fn select_patient(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        if self.has_unsaved_changes(cx) && self.shown_patient(cx).as_deref() != Some(id) {
            let id = id.to_string();
            confirm_then(cx.weak_entity(), window, cx, move |this, window, cx| {
                this.select_patient_now(&id, window, cx)
            });
            return;
        }
        self.select_patient_now(id, window, cx);
    }

    fn select_patient_now(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        if self.list.read(cx).delegate().position(id).is_none() {
            self.list
                .update(cx, |list, cx| list.set_query("", window, cx));
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
        let selected = self
            .detail
            .as_ref()
            .map(|d| d.read(cx).patient_id().to_string());
        match selected {
            Some(id) => self.reselect(&id, window, cx),
            None => self
                .list
                .update(cx, |list, cx| list.set_selected_index(None, window, cx)),
        }
        cx.notify();
    }

    fn set_order(&mut self, order: PatientOrder, window: &mut Window, cx: &mut Context<Self>) {
        self.list
            .update(cx, |list, _| list.delegate_mut().order = order);
        self.reload_list(window, cx);
    }

    pub fn new_patient(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let form = cx.new(|cx| ProfileForm::new(None, window, cx));
        self.form_subscription =
            Some(
                cx.subscribe_in(&form, window, |this, _, event, window, cx| {
                    let ProfileFormEvent::Submit(profile) = event;
                    match this.store.create_patient(profile, today()) {
                        Ok(patient) => {
                            window.close_dialog(cx);
                            window.push_notification(
                                Notification::success(t("Pasien ditambahkan", "Patient added")),
                                cx,
                            );
                            this.reload_list(window, cx);
                            this.select_patient(&patient.id, window, cx);
                            cx.emit(PatientsEvent::Changed);
                        }
                        Err(error) => {
                            window.push_notification(Notification::error(error.to_string()), cx)
                        }
                    }
                }),
            );

        let first_render = Cell::new(true);
        window.open_dialog(cx, move |dialog, window, cx| {
            if first_render.replace(false) {
                form.update(cx, |form, cx| form.focus(window, cx));
            }
            let save = form.clone();
            dialog
                .title(t("Pasien Baru", "New Patient"))
                .w(px(520.))
                .child(form.clone())
                .footer(
                    DialogFooter::new()
                        .child(
                            Button::new("new-patient-cancel")
                                .label(t("Batal", "Cancel"))
                                .on_click(|_, window, cx| window.close_dialog(cx)),
                        )
                        .child(
                            Button::new("new-patient-save")
                                .primary()
                                .label(t("Simpan", "Save"))
                                .on_click(move |_, window, cx| {
                                    save.update(cx, |form, cx| form.submit(window, cx))
                                }),
                        ),
                )
        });
    }

    fn render_list_pane(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let delegate = self.list.read(cx).delegate();
        let (count, order) = (delegate.patients.len(), delegate.order);
        let sort =
            |id: &'static str, icon: IconName, tooltip: &'static str, value: PatientOrder| {
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
                pane_header(t("Pasien", "Patients"), Some(count), cx)
                    .child(
                        sort(
                            "sort-recent",
                            IconName::Clock,
                            t("Kunjungan terbaru dahulu", "Most recent visit first"),
                            PatientOrder::RecentVisit,
                        )
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.set_order(PatientOrder::RecentVisit, window, cx)
                        })),
                    )
                    .child(
                        sort(
                            "sort-name",
                            IconName::ArrowDownAZ,
                            t("Menurut nama", "By name"),
                            PatientOrder::Name,
                        )
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.set_order(PatientOrder::Name, window, cx)
                        })),
                    )
                    .child(
                        Button::new("new-patient")
                            .icon(IconName::UserPlus)
                            .ghost()
                            .xsmall()
                            .tooltip(t("Pasien baru", "New patient"))
                            .on_click(
                                cx.listener(|this, _, window, cx| this.new_patient(window, cx)),
                            ),
                    ),
            )
            .child(div().flex_1().min_h(px(0.)).child(
                List::new(&self.list).search_placeholder(t("Cari pasien", "Search patients")),
            ))
    }
}

impl Render for PatientsView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let detail = match &self.detail {
            Some(detail) => detail.clone().into_any_element(),
            None => empty_state(
                IconName::Users,
                t("Belum ada pasien yang dipilih", "No patient selected"),
                t(
                    "Pilih pasien dari daftar, atau tambahkan pasien baru.",
                    "Choose a patient from the list, or add a new one.",
                ),
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

// ============================================================================
// The detail pane of a patient: the profile, the list of their visits, and
// the selected visit (or the editor of a new one).
// ============================================================================

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
        let today = today();
        let (date, relative) = match parse_stored(&row.visit.date) {
            Some(date) => (format_short(date), format_relative(date, today)),
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
        empty_state(IconName::Calendar, title, hint, cx)
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
                            this.request_visit(&id, window, cx);
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

    /// Whether a visit being written or edited is not saved.
    pub fn has_unsaved_changes(&self, cx: &App) -> bool {
        self.composer
            .as_ref()
            .is_some_and(|(composer, _)| composer.read(cx).has_changes(cx))
            || self
                .editor
                .as_ref()
                .is_some_and(|(_, editor, _)| editor.read(cx).has_changes(cx))
    }

    /// Shows the visit chosen in the list, asking first when that would lose
    /// a visit being written.
    fn request_visit(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let editing_it = self.composer.is_none() && self.entry_editor(id).is_some();
        if editing_it || !self.has_unsaved_changes(cx) {
            self.select_visit(id, window, cx);
            return;
        }
        // Keep the highlight on the visit shown until the user decides.
        let shown = self.selected.clone();
        self.visits.update(cx, |list, cx| {
            let row = shown.and_then(|id| list.delegate().position(&id));
            list.set_selected_index(row.map(IndexPath::new), window, cx);
        });
        let id = id.to_string();
        confirm_then(cx.weak_entity(), window, cx, move |this, window, cx| {
            this.select_visit(&id, window, cx)
        });
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
        if self.has_unsaved_changes(cx) {
            confirm_then(cx.weak_entity(), window, cx, |this, window, cx| {
                this.editor = None;
                this.open_composer(window, cx);
            });
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
        if self.composer.is_none() && self.entry_editor(id).is_some() {
            return;
        }
        if self.has_unsaved_changes(cx) {
            let id = id.to_string();
            confirm_then(cx.weak_entity(), window, cx, move |this, window, cx| {
                this.composer = None;
                this.editor = None;
                this.edit_entry(&id, window, cx);
            });
            return;
        }
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
                .cancel_text(t("Batal", "Cancel"))
                .ok_text(t("Hapus", "Delete"))
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
        match diagnosis::write_visit_page(patient, visit) {
            Ok(path) => cx.open_url(&diagnosis::file_url(&path)),
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
                .cancel_text(t("Batal", "Cancel"))
                .ok_text(t("Hapus", "Delete"))
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
        let today = today();
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
                display_with_age(&patient.birth_date, today),
                1,
            )
            .item(t("Alamat", "Address"), or_dash(&patient.address), 1)
            .item(
                t("Kunjungan Terakhir", "Last Visit"),
                display_with_relative(&patient.last_visit, today),
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
                pane_header(t("Kunjungan", "Visits"), Some(self.visit_count), cx).child(
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
                return empty_state(
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
            return empty_state(
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

// ============================================================================
// The editor of a visit entry: a date and a Markdown diagnosis.
// ============================================================================

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
    /// The date and text the editor started with.
    initial_date: NaiveDate,
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
        Self::build(
            "new-entry".into(),
            today(),
            template,
            false,
            false,
            window,
            cx,
        )
    }

    /// An editor for an existing visit.
    pub fn edit(visit: &Visit, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let date = parse_stored(&visit.date).unwrap_or_else(today);
        let text = diagnosis::editable_markdown(&visit.diagnosis, visit.format);
        let from_html = visit.format == DiagnosisFormat::Html;
        Self::build(
            format!("edit-{}", visit.id).into(),
            date,
            text,
            true,
            from_html,
            window,
            cx,
        )
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
            initial_date: date,
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

    /// Whether the date or the text was changed.
    pub fn has_changes(&self, cx: &App) -> bool {
        let same_date = matches!(self.date.read(cx).date(), Date::Single(Some(date)) if date == self.initial_date);
        !same_date || self.text.read(cx).value().as_ref() != self.initial_text
    }

    pub fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Date::Single(Some(date)) = self.date.read(cx).date() else {
            window.push_notification(
                Notification::error(t(
                    "Pilih tanggal kunjungan",
                    "Please choose the date of the visit",
                )),
                cx,
            );
            return;
        };
        let text = self.text(cx);
        if text.trim().is_empty() {
            window.push_notification(
                Notification::error(t("Diagnosis belum diisi", "Missing diagnosis")),
                cx,
            );
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
                    .tooltip(t(
                        "Tebal (mengapit pilihan dengan **)",
                        "Bold (wraps the selection in **)",
                    ))
                    .disabled(self.preview)
                    .on_click(cx.listener(|this, _, window, cx| this.bold(window, cx))),
            )
            .child(
                Button::new("preview")
                    .icon(IconName::Eye)
                    .label(t("Pratinjau", "Preview"))
                    .ghost()
                    .small()
                    .selected(self.preview)
                    .on_click(cx.listener(|this, _, window, cx| this.toggle_preview(window, cx))),
            );

        let note = self.from_html.then(|| {
            muted(
                t(
                    "Ditulis dengan editor lama dan ditampilkan di sini setelah dikonversi. \
                     Aslinya tetap disimpan kecuali teksnya diubah.",
                    "Written with the old editor and shown here converted. \
                     The original is kept unless you change the text.",
                ),
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
                            .label(t("Batal", "Cancel"))
                            .on_click(cx.listener(|_, _, _, cx| cx.emit(VisitEditorEvent::Cancel))),
                    )
                    .child(
                        Button::new("visit-save")
                            .primary()
                            .small()
                            .label(t("Simpan", "Save"))
                            .on_click(cx.listener(|this, _, window, cx| this.submit(window, cx))),
                    ),
            )
    }
}

// ============================================================================
// The patient profile form, shown in a dialog for new patients and edits.
// ============================================================================

pub enum ProfileFormEvent {
    /// The form was submitted and is valid.
    Submit(PatientProfile),
}

pub struct ProfileForm {
    name: Entity<InputState>,
    head_family_name: Entity<InputState>,
    birth_date: Entity<InputState>,
    address: Entity<TextareaState>,
    errors: PatientErrors,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<ProfileFormEvent> for ProfileForm {}

impl ProfileForm {
    /// A form for a new patient, or for editing `patient`.
    pub fn new(patient: Option<&Patient>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let value =
            |get: fn(&Patient) -> &String| patient.map(|p| get(p).clone()).unwrap_or_default();
        let name = cx.new(|cx| InputState::new(window, cx).default_value(value(|p| &p.name)));
        let head_family_name =
            cx.new(|cx| InputState::new(window, cx).default_value(value(|p| &p.head_family_name)));
        let birth_date = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("YYYY-MM-DD")
                .default_value(value(|p| &p.birth_date))
        });
        let address = cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(2, 6)
                .default_value(value(|p| &p.address))
        });

        // Enter in a single-line field saves, as in a native dialog.
        let submit_on_enter = |this: &mut Self,
                               _: &Entity<InputState>,
                               event: &InputEvent,
                               window: &mut Window,
                               cx: &mut Context<Self>| {
            if let InputEvent::PressEnter { .. } = event {
                this.submit(window, cx);
            }
        };
        let subscriptions = vec![
            cx.subscribe_in(&name, window, submit_on_enter),
            cx.subscribe_in(&head_family_name, window, submit_on_enter),
            cx.subscribe_in(&birth_date, window, submit_on_enter),
        ];

        Self {
            name,
            head_family_name,
            birth_date,
            address,
            errors: PatientErrors::default(),
            _subscriptions: subscriptions,
        }
    }

    pub fn focus(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.name.update(cx, |input, cx| input.focus(window, cx));
    }

    /// Validates the form, and emits [`ProfileFormEvent::Submit`] when valid.
    pub fn submit(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        let input = PatientInput {
            name: self.name.read(cx).value().to_string(),
            head_family_name: self.head_family_name.read(cx).value().to_string(),
            birth_date: self.birth_date.read(cx).value().to_string(),
            address: self.address.read(cx).value().to_string(),
        };
        match input.validate(today()) {
            Ok(profile) => {
                self.errors = PatientErrors::default();
                cx.emit(ProfileFormEvent::Submit(profile));
            }
            Err(errors) => self.errors = errors,
        }
        cx.notify();
    }
}

impl Render for ProfileForm {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let with_error = |input: AnyElement, error: Option<&String>, cx: &Context<Self>| {
            v_flex()
                .w_full()
                .child(input)
                .children(field_error(error, cx))
        };
        v_form()
            .child(
                field()
                    .label(t("Nama", "Name"))
                    .required(true)
                    .child(with_error(
                        Input::new(&self.name).into_any_element(),
                        self.errors.name.as_ref(),
                        cx,
                    )),
            )
            .child(
                field()
                    .label(t("Nama KK", "Head of Family"))
                    .child(with_error(
                        Input::new(&self.head_family_name).into_any_element(),
                        self.errors.head_family_name.as_ref(),
                        cx,
                    )),
            )
            .child(
                field()
                    .label(t("Tanggal Lahir", "Date of Birth"))
                    .required(true)
                    .description(t(
                        "YYYY-MM-DD, atau DD-MM-YYYY",
                        "YYYY-MM-DD, or DD-MM-YYYY",
                    ))
                    .child(with_error(
                        Input::new(&self.birth_date).into_any_element(),
                        self.errors.birth_date.as_ref(),
                        cx,
                    )),
            )
            .child(
                field()
                    .label(t("Alamat", "Address"))
                    .required(true)
                    .child(with_error(
                        Textarea::new(&self.address).into_any_element(),
                        self.errors.address.as_ref(),
                        cx,
                    )),
            )
    }
}

// ============================================================================
// How a visit entry is displayed.
// ============================================================================

/// A visit as a journal entry: its date, actions, and the rendered text.
/// `scope` keeps the rendered text of the same visit in two views apart.
pub fn entry_view(
    scope: &str,
    visit: &Visit,
    actions: impl IntoIterator<Item = AnyElement>,
    cx: &App,
) -> Div {
    let today = today();
    let (title, relative) = match parse_stored(&visit.date) {
        Some(date) => (format_with_weekday(date), format_relative(date, today)),
        None => (visit.date.clone(), String::new()),
    };
    let text_id = SharedString::from(format!("{scope}-visit-{}", visit.id));
    let body = match visit.format {
        DiagnosisFormat::Html => TextView::html(text_id, visit.diagnosis.clone()),
        DiagnosisFormat::Markdown => {
            TextView::markdown(text_id, diagnosis::with_line_breaks(&visit.diagnosis))
        }
    };

    v_flex()
        .w_full()
        .gap_2()
        .child(
            h_flex()
                .w_full()
                .gap_2()
                .child(div().font_weight(FontWeight::SEMIBOLD).child(title))
                .child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(relative),
                )
                .child(div().flex_1())
                .children(actions),
        )
        .child(div().w_full().child(body))
}

// ============================================================================
// The Visits section: every visit of every patient, newest first.
// ============================================================================

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
        match self
            .store
            .list_visit_summaries(&self.query, self.from, self.to)
        {
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
        let title = parse_stored(&day.date)
            .map(format_with_weekday)
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
                .child(format_count(day.visits.len())),
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
            format!(
                "{} (KK {})",
                visit.patient_name,
                visit.head_family_name.trim()
            )
        };
        let summary = if visit.summary.is_empty() {
            " ".to_string()
        } else {
            visit.summary.clone()
        };
        Some(
            ListItem::new(SharedString::from(format!("visit-{}", visit.id))).child(
                v_flex()
                    .w_full()
                    .min_w(px(0.))
                    .child(
                        div()
                            .w_full()
                            .truncate()
                            .font_weight(FontWeight::MEDIUM)
                            .child(name),
                    )
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

    fn set_selected_index(
        &mut self,
        _: Option<IndexPath>,
        _: &mut Window,
        _: &mut Context<ListState<Self>>,
    ) {
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
                t("Tidak ada kunjungan", "No visits"),
                t(
                    "Tidak ada yang cocok dengan nama dan tanggal.",
                    "Nothing matches the name and dates.",
                )
                .to_string(),
            ),
        };
        empty_state(IconName::Calendar, title, hint, cx)
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
        match diagnosis::write_visit_page(&selected.patient, &selected.visit) {
            Ok(path) => cx.open_url(&diagnosis::file_url(&path)),
            Err(error) => window.push_notification(
                Notification::error(pick(
                    format!("Gagal mencetak: {error}"),
                    format!("Cannot print: {error}"),
                )),
                cx,
            ),
        }
    }

    fn render_list_pane(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let total = self.list.read(cx).delegate().total;
        let today = today();
        let days_ago = |days: u64| today.checked_sub_days(Days::new(days)).unwrap_or(today);
        let month_start = today.with_day(1).unwrap_or(today);
        let year_start = today.with_ordinal(1).unwrap_or(today);
        let presets = vec![
            DateRangePreset::range(t("Hari ini", "Today"), today, today),
            DateRangePreset::range(t("7 hari terakhir", "Last 7 days"), days_ago(6), today),
            DateRangePreset::range(t("30 hari terakhir", "Last 30 days"), days_ago(29), today),
            DateRangePreset::range(t("Bulan ini", "This month"), month_start, today),
            DateRangePreset::range(t("Tahun ini", "This year"), year_start, today),
        ];
        v_flex()
            .size_full()
            .child(pane_header(t("Kunjungan", "Visits"), Some(total), cx))
            .child(
                div().w_full().flex_none().px_3().pt_2().child(
                    DatePicker::new(&self.range)
                        .small()
                        .cleanable(true)
                        .placeholder(t("Semua tanggal", "All dates"))
                        .presets(presets),
                ),
            )
            .child(
                div().flex_1().min_h(px(0.)).child(
                    List::new(&self.list)
                        .search_placeholder(t("Saring menurut pasien", "Filter by patient")),
                ),
            )
    }

    fn render_detail(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(selected) = &self.selected else {
            return empty_state(
                IconName::Calendar,
                t("Belum ada kunjungan yang dipilih", "No visit selected"),
                t(
                    "Pilih kunjungan dari daftar.",
                    "Choose a visit from the list.",
                ),
                cx,
            );
        };
        let patient = &selected.patient;
        let today = today();
        let mut subtitle = Vec::new();
        if let Some(birth) = parse_stored(&patient.birth_date) {
            subtitle.push(format_age(age_years(birth, today)));
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
                            .tooltip(t("Cetak", "Print"))
                            .on_click(cx.listener(|this, _, window, cx| this.print(window, cx))),
                    )
                    .child(
                        Button::new("open-patient")
                            .small()
                            .icon(IconName::User)
                            .label(t("Buka Pasien", "Open Patient"))
                            .on_click(cx.listener(move |_, _, _, cx| {
                                cx.emit(VisitsEvent::OpenPatient(patient_id.clone()))
                            })),
                    ),
            )
            .child(
                div()
                    .id(SharedString::from(format!(
                        "visit-scroll-{}",
                        selected.visit.id
                    )))
                    .flex_1()
                    .min_h(px(0.))
                    .w_full()
                    .overflow_y_scrollbar()
                    .child(div().px_5().py_4().child(entry_view(
                        "visits",
                        &selected.visit,
                        [],
                        cx,
                    ))),
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

// ============================================================================
// The Settings section: the template of new entries, backups and export,
// and database details.
// ============================================================================

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
    pub fn new(
        store: Rc<Store>,
        db_path: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
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

    /// Whether the template was changed and not saved.
    pub fn has_unsaved_changes(&self, cx: &App) -> bool {
        self.template.read(cx).value().as_ref() != self.saved
    }

    fn set_text(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.template.update(cx, |state, cx| {
            state.set_value(text.to_string(), window, cx)
        });
        cx.notify();
    }

    pub fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.text(cx);
        match self.store.set_diagnosis_template(Some(&text)) {
            Ok(()) => {
                self.saved = text;
                window.push_notification(
                    Notification::success(t(
                        "Kunjungan baru akan dimulai dari templat ini.",
                        "New visits will start from this template.",
                    )),
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

        GroupBox::new().fill().title(t("Templat kunjungan baru", "New visit template")).child(
            v_flex()
                .w_full()
                .gap_3()
                .child(
                    muted(
                        t(
                            "Setiap kunjungan baru dimulai dengan teks ini. Ditulis dalam Markdown: \
                             **tebal**, \"- \" untuk poin, \"1. \" untuk butir bernomor, \
                             \"##### \" untuk judul. Kunjungan yang sudah ditulis tidak berubah.",
                            "Every new visit starts with this text. It is written in Markdown: \
                             **bold**, \"- \" for a bullet, \"1. \" for a numbered item, \
                             \"##### \" for a heading. Visits already written do not change.",
                        ),
                        cx,
                    )
                    .text_sm(),
                )
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
                                .child(column_title(t("Pratinjau", "Preview")))
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
                                .label(t("Kembalikan ke Bawaan", "Reset to Default"))
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
                                .label(t("Buang Perubahan", "Discard Changes"))
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
                                .label(t("Simpan", "Save"))
                                .disabled(!changed)
                                .on_click(cx.listener(|this, _, window, cx| this.save(window, cx))),
                        ),
                ),
        )
    }

    // Backup, restore, export -------------------------------------------

    /// Asks where to save a backup, then writes it.
    pub fn back_up(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let name = format!("kutilang-backup-{}.sqlite3", to_stored(today()));
        let chosen = cx.prompt_for_new_path(&default_folder(&self.db_path), Some(&name));
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(path))) = chosen.await else {
                return;
            };
            this.update_in(cx, |this, window, cx| match this.store.backup_to(&path) {
                Ok(()) => window.push_notification(
                    Notification::success(pick(
                        format!("Cadangan disimpan sebagai {}.", path.display()),
                        format!("Backup saved as {}.", path.display()),
                    )),
                    cx,
                ),
                Err(error) => {
                    window.push_notification(Notification::error(format!("{error:#}")), cx)
                }
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
            prompt: Some(t("Pulihkan", "Restore").into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(paths))) = chosen.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            this.update_in(cx, |this, window, cx| {
                this.confirm_restore(&path, window, cx)
            })
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
        let (patients, visits) = (format_count(backup.patients), format_count(backup.visits));
        let file = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let description = pick(
            format!(
                "Semua pasien dan kunjungan yang ada sekarang akan diganti dengan {patients} pasien \
                 dan {visits} kunjungan dari {file}. Salinan data saat ini disimpan terlebih dahulu."
            ),
            format!(
                "Every patient and visit now in Kutilang will be replaced by the {patients} patients \
                 and {visits} visits of {file}. A copy of the current data is saved first."
            ),
        );
        // The dialog's builder can run more than once; the backup is used once.
        let backup = Rc::new(RefCell::new(Some(backup)));
        let this = cx.weak_entity();
        window.open_alert_dialog(cx, move |alert, _, _| {
            let (this, backup) = (this.clone(), backup.clone());
            alert
                .title(t("Pulihkan cadangan ini?", "Restore this backup?"))
                .description(description.clone())
                .show_cancel(true)
                .cancel_text(t("Batal", "Cancel"))
                .ok_text(t("Pulihkan", "Restore"))
                .ok_variant(ButtonVariant::Danger)
                .on_ok(move |_, window, cx| {
                    if let Some(backup) = backup.borrow_mut().take() {
                        this.update(cx, |this, cx| this.apply_restore(backup, window, cx))
                            .ok();
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
                Notification::error(pick(
                    format!("Tidak ada yang dipulihkan: salinan data saat ini tidak dapat disimpan ({error:#})."),
                    format!("Nothing was restored: cannot save a copy of the current data first ({error:#})."),
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
                    Notification::success({
                        let (patients, visits, previous) = (
                            format_count(backup.patients),
                            format_count(backup.visits),
                            previous.display(),
                        );
                        pick(
                            format!("{patients} pasien dan {visits} kunjungan dipulihkan. Data sebelumnya disimpan sebagai {previous}."),
                            format!("Restored {patients} patients and {visits} visits. The previous data was saved as {previous}."),
                        )
                    })
                    .autohide(false),
                    cx,
                );
                cx.emit(SettingsEvent::DataReplaced);
            }
            Err(error) => window.push_notification(
                Notification::error(pick(
                    format!("Gagal memulihkan: {error:#}"),
                    format!("Cannot restore: {error:#}"),
                )),
                cx,
            ),
        }
        cx.notify();
    }

    /// Asks where to save the spreadsheet, then writes it.
    pub fn export(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let name = format!("kutilang-{}.xlsx", to_stored(today()));
        let chosen = cx.prompt_for_new_path(&default_folder(&self.db_path), Some(&name));
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(path))) = chosen.await else {
                return;
            };
            this.update_in(cx, |this, window, cx| {
                match store::export_xlsx(&this.store, &path) {
                    Ok(report) => window.push_notification(
                        Notification::success({
                            let (patients, visits, path) = (
                                format_count(report.patients),
                                format_count(report.visits),
                                path.display(),
                            );
                            pick(
                                format!(
                                    "{patients} pasien dan {visits} kunjungan diekspor ke {path}."
                                ),
                                format!(
                                    "Exported {patients} patients and {visits} visits to {path}."
                                ),
                            )
                        }),
                        cx,
                    ),
                    Err(error) => window.push_notification(
                        Notification::error(pick(
                            format!("Gagal mengekspor: {error:#}"),
                            format!("Cannot export: {error:#}"),
                        )),
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
                .child(muted(text, cx).text_sm())
        };
        GroupBox::new().fill().title(t("Cadangan & Ekspor", "Backup & Export")).child(
            v_flex()
                .w_full()
                .gap_3()
                .child(row(
                    Button::new("back-up")
                        .small()
                        .label(t("Cadangkan…", "Back Up…"))
                        .on_click(cx.listener(|this, _, window, cx| this.back_up(window, cx))),
                    t("Simpan salinan semua pasien, kunjungan, dan pengaturan ke berkas pilihan Anda.", "Save a copy of every patient, visit and setting to a file of your choice."),
                    cx,
                ))
                .child(row(
                    Button::new("restore")
                        .small()
                        .label(t("Pulihkan…", "Restore…"))
                        .on_click(cx.listener(|this, _, window, cx| this.restore(window, cx))),
                    t(
                        "Ganti semua data dengan cadangan (bisa juga database aplikasi lama). \
                         Data saat ini disimpan dulu di samping database.",
                        "Replace everything with a backup (also a database of the old application). \
                         The current data is saved next to the database first.",
                    ),
                    cx,
                ))
                .child(row(
                    Button::new("export")
                        .small()
                        .label(t("Ekspor ke Excel…", "Export to Excel…"))
                        .on_click(cx.listener(|this, _, window, cx| this.export(window, cx))),
                    t("Tulis semua pasien dan kunjungan ke berkas Excel (.xlsx), masing-masing satu lembar.", "Write all patients and visits to an Excel workbook (.xlsx), one sheet each."),
                    cx,
                )),
        )
    }

    pub fn set_language(&mut self, language: Language, cx: &mut Context<Self>) {
        if language == crate::language() {
            return;
        }
        set_language(language);
        save_preference(language).ok();
        set_menus(cx);
        // Every text is chosen when it is drawn: drawing again is enough.
        cx.refresh_windows();
    }

    fn render_language(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let current = language();
        GroupBox::new().fill().title(t("Bahasa", "Language")).child(
            v_flex()
                .w_full()
                .gap_3()
                .child(muted(
                    t(
                        "Bahasa menu, tombol, tanggal, dan ekspor Excel. Isi kunjungan tidak berubah.",
                        "The language of menus, buttons, dates and the Excel export. Visits are not changed.",
                    ),
                    cx,
                ))
                .child(h_flex().gap_2().children(Language::ALL.map(|language| {
                    Button::new(language.code())
                        .small()
                        .label(language.name())
                        .when(language == current, |button| button.primary())
                        .on_click(cx.listener(move |this, _, _, cx| this.set_language(language, cx)))
                }))),
        )
    }

    fn render_database(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let count =
            |result: anyhow::Result<usize>| result.map(format_count).unwrap_or_else(|_| "-".into());
        GroupBox::new()
            .fill()
            .title(t("Database", "Database"))
            .child(
                DescriptionList::horizontal()
                    .columns(1)
                    .label_width(px(140.))
                    .item(t("Berkas", "File"), self.db_path.display().to_string(), 1)
                    .item(
                        t("Pasien", "Patients"),
                        count(self.store.count_patients()),
                        1,
                    )
                    .item(
                        t("Kunjungan", "Visits"),
                        count(self.store.count_visits()),
                        1,
                    )
                    .item(
                        t("Format", "Format"),
                        format!("Kutilang v2, schema {SCHEMA_VERSION}"),
                        1,
                    ),
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
                    .child(
                        div()
                            .text_xl()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(t("Pengaturan", "Settings")),
                    )
                    .child(self.render_language(cx))
                    .child(self.render_data(cx))
                    .child(self.render_template(cx))
                    .child(self.render_database(cx)),
            )
    }
}

// ============================================================================
// Small presentational helpers shared by the views.
// ============================================================================

/// `2437` -> `2.437` (`2,437` in English).
pub fn format_count(count: usize) -> String {
    let separator = pick('.', ',');
    let digits = count.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(separator);
        }
        out.push(c);
    }
    out
}

/// Muted secondary text.
pub fn muted(text: impl Into<SharedString>, cx: &App) -> Div {
    div()
        .text_color(cx.theme().muted_foreground)
        .child(text.into())
}

/// The header row above a list: a title, a count and actions.
pub fn pane_header(title: &'static str, count: Option<usize>, cx: &App) -> Div {
    h_flex()
        .w_full()
        .flex_none()
        .h(px(44.))
        .px_3()
        .gap_2()
        .border_b_1()
        .border_color(cx.theme().border)
        .child(div().font_weight(FontWeight::SEMIBOLD).child(title))
        .children(count.map(|count| {
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(format_count(count))
        }))
        .child(div().flex_1())
}

/// A centered placeholder for an empty pane.
pub fn empty_state(
    icon: impl Into<Icon>,
    title: impl Into<SharedString>,
    hint: impl Into<SharedString>,
    cx: &App,
) -> AnyElement {
    v_flex()
        .size_full()
        .items_center()
        .justify_center()
        .gap_2()
        .text_color(cx.theme().muted_foreground)
        .child(
            icon.into()
                .size(px(40.))
                .text_color(cx.theme().muted_foreground.opacity(0.5)),
        )
        .child(div().font_weight(FontWeight::MEDIUM).child(title.into()))
        .child(div().text_sm().child(hint.into()))
        .into_any_element()
}

/// A validation message under a form field.
pub fn field_error(message: Option<&String>, cx: &App) -> Option<AnyElement> {
    message.map(|message| {
        div()
            .mt_1()
            .text_sm()
            .text_color(cx.theme().danger)
            .child(message.clone())
            .into_any_element()
    })
}

#[cfg(test)]
mod widgets_tests {
    use super::format_count;

    #[test]
    fn thousands_separators() {
        assert_eq!(format_count(0), "0");
        assert_eq!(format_count(999), "999");
        assert_eq!(format_count(2437), "2.437");
        assert_eq!(format_count(1234567), "1.234.567");
    }
}

// ============================================================================
// Icons: the default set of the component library, plus the few Lucide icons
// this application uses that are not part of it (see `assets/icons`, ISC
// licensed). Embedding the whole Lucide catalog would add 7 MB.
// ============================================================================

macro_rules! extra_icons {
    ($($name:literal),* $(,)?) => {
        [$((concat!("icons/", $name, ".svg"), include_bytes!(concat!("../assets/icons/", $name, ".svg")) as &[u8])),*]
    };
}

const EXTRA_ICONS: [(&str, &[u8]); 11] = extra_icons!(
    "arrow-down-a-z",
    "bold",
    "clock",
    "database",
    "pencil",
    "printer",
    "rotate-ccw",
    "stethoscope",
    "trash",
    "user-plus",
    "users",
);

pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if let Some((_, data)) = EXTRA_ICONS.iter().find(|(icon, _)| *icon == path) {
            return Ok(Some(Cow::Borrowed(data)));
        }
        gpui_kit::assets::Assets.load(path)
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut paths = gpui_kit::assets::Assets.list(path)?;
        paths.extend(
            EXTRA_ICONS
                .iter()
                .filter(|(icon, _)| icon.starts_with(path))
                .map(|(icon, _)| SharedString::from(*icon)),
        );
        Ok(paths)
    }
}

#[cfg(test)]
mod assets_tests {
    use super::*;
    use gpui_kit::assets::IconName;

    #[test]
    fn serves_the_icons_the_interface_uses() {
        for icon in [
            IconName::ArrowDownAZ,
            IconName::Bold,
            IconName::Calendar,
            IconName::Clock,
            IconName::Database,
            IconName::FolderOpen,
            IconName::Close,
            IconName::Eye,
            IconName::Inbox,
            IconName::Pencil,
            IconName::Plus,
            IconName::Printer,
            IconName::RotateCcw,
            IconName::Search,
            IconName::Settings,
            IconName::Stethoscope,
            IconName::Trash,
            IconName::User,
            IconName::UserPlus,
            IconName::Users,
        ] {
            let path = icon.path();
            assert!(
                matches!(Assets.load(&path), Ok(Some(data)) if !data.is_empty()),
                "missing {path}"
            );
        }
    }
}
