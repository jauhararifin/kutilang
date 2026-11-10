//! The window's root: the database picker, or the open database.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use gpui_kit::component::WindowExt as _;
use gpui_kit::component::notification::Notification;
use gpui_kit::prelude::*;
use gpui_kit::{Context, Entity, Subscription, Window, div};

use crate::i18n::{pick, t};
use crate::recent::RecentDatabases;
use crate::store::Store;
use crate::ui::app::{AppEvent, KutilangApp, Section};
use crate::ui::launcher::{Launcher, LauncherEvent};
use crate::ui::{CloseDatabase, OpenDatabase};

enum Content {
    Launcher {
        view: Entity<Launcher>,
        _subscription: Subscription,
    },
    App {
        view: Entity<KutilangApp>,
        _subscription: Subscription,
    },
}

pub struct Shell {
    recent: Rc<RefCell<RecentDatabases>>,
    content: Content,
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
        let shell = Self { recent, content };
        if let Some(path) = database {
            // Notifications need the window to be set up.
            cx.defer_in(window, move |shell, window, cx| {
                shell.open(&path, window, cx)
            });
        }
        shell
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

    /// Opens a database (creating it if needed). On failure the picker stays,
    /// with the reason.
    pub fn open(&mut self, path: &Path, window: &mut Window, cx: &mut Context<Self>) {
        match Store::open(path) {
            Ok((store, report)) => {
                if let Some(report) = &report {
                    eprintln!("kutilang: upgraded {} ({report:?})", path.display());
                }
                self.recent.borrow_mut().add(path).ok();
                let path = path.to_path_buf();
                let app = cx.new(|cx| KutilangApp::new(Rc::new(store), report, path, window, cx));
                self.show_app(app, window, cx);
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
                if matches!(self.content, Content::App { .. }) {
                    self.content = Self::launcher(&self.recent, window, cx);
                }
            }
        }
        cx.notify();
    }

    fn show_app(&mut self, app: Entity<KutilangApp>, window: &mut Window, cx: &mut Context<Self>) {
        let subscription =
            cx.subscribe_in(&app, window, |shell, app, event, window, cx| match event {
                AppEvent::LanguageChanged => {
                    // Build the views again so every text is in the new language.
                    let (store, path) = {
                        let app = app.read(cx);
                        (app.store().clone(), app.db_path().clone())
                    };
                    let fresh = cx.new(|cx| {
                        let mut fresh = KutilangApp::new(store, None, path, window, cx);
                        fresh.show(Section::Settings, window, cx);
                        fresh
                    });
                    shell.show_app(fresh, window, cx);
                }
            });
        self.content = Content::App {
            view: app,
            _subscription: subscription,
        };
        cx.notify();
    }

    /// Closes the open database and shows the picker.
    pub fn show_launcher(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Content::App { .. } = self.content {
            self.content = Self::launcher(&self.recent, window, cx);
            cx.notify();
        }
    }

    pub fn app(&self) -> Option<&Entity<KutilangApp>> {
        match &self.content {
            Content::App { view, .. } => Some(view),
            Content::Launcher { .. } => None,
        }
    }

    pub fn launcher_view(&self) -> Option<&Entity<Launcher>> {
        match &self.content {
            Content::Launcher { view, .. } => Some(view),
            Content::App { .. } => None,
        }
    }
}

impl Render for Shell {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let content = match &self.content {
            Content::Launcher { view, .. } => view.clone().into_any_element(),
            Content::App { view, .. } => view.clone().into_any_element(),
        };
        div()
            .size_full()
            .on_action(cx.listener(|shell, _: &OpenDatabase, window, cx| {
                shell.show_launcher(window, cx);
                if let Content::Launcher { view, .. } = &shell.content {
                    view.update(cx, |launcher, cx| launcher.choose_file(window, cx));
                }
            }))
            .on_action(
                cx.listener(|shell, _: &CloseDatabase, window, cx| shell.show_launcher(window, cx)),
            )
            .child(content)
    }
}
