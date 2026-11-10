//! The first screen: choose the database to open, from the recent ones or a
//! file.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::notification::Notification;
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::{
    ActiveTheme as _, Icon, Sizable as _, TitleBar, WindowExt as _, h_flex, v_flex,
};
use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, Context, EventEmitter, FontWeight, PathPromptOptions, SharedString, Window, div,
    px,
};

use crate::dates;
use crate::recent::{self, RecentDatabases};
use crate::ui::app::BRAND;
use crate::ui::widgets;

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
        let today = dates::today();
        let mut rows: Vec<Row> = self
            .recent
            .borrow()
            .entries()
            .iter()
            .map(|entry| Row {
                path: entry.path.clone(),
                opened: Some(format!(
                    "dibuka {}",
                    dates::format_relative(entry.opened_at.date_naive(), today)
                )),
                default: false,
            })
            .collect();
        let default = recent::default_database();
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
            prompt: Some("Open".into()),
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
        let folder = dirs::document_dir().unwrap_or_else(recent::app_dir);
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
                        Notification::error(format!(
                            "{} already exists. Choose another name, or open it with Open Database.",
                            path.display()
                        )),
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
            "File not found".into()
        } else if row.default {
            "Database of the original application".into()
        } else {
            row.opened.clone().unwrap_or_default().into()
        };
        let theme = cx.theme();
        let (hover, muted, danger, border) = (theme.list_hover, theme.muted_foreground, theme.danger, theme.border);
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
            .child(Icon::new(IconName::Database).text_color(if exists { theme.primary } else { muted }))
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
                        .tooltip("Remove from the list")
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
                .child(widgets::muted("No databases opened yet.", cx).text_sm())
                .into_any_element()
        } else {
            v_flex().w_full().children(rows).into_any_element()
        };

        v_flex()
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(TitleBar::new().child(div().text_sm().font_weight(FontWeight::SEMIBOLD).child("Kutilang")))
            .child(
                div().id("launcher-scroll").flex_1().min_h(px(0.)).overflow_y_scrollbar().child(
                    v_flex().w_full().items_center().py_12().child(
                        v_flex()
                            .w(px(580.))
                            .gap_6()
                            .child(
                                h_flex()
                                    .gap_3()
                                    .child(Icon::new(IconName::Stethoscope).size(px(36.)).text_color(cx.theme().primary))
                                    .child(
                                        v_flex()
                                            .child(div().text_2xl().font_weight(FontWeight::SEMIBOLD).child("Kutilang"))
                                            .child(widgets::muted(BRAND, cx)),
                                    ),
                            )
                            .child(
                                v_flex()
                                    .gap_2()
                                    .child(div().font_weight(FontWeight::SEMIBOLD).child("Recent Databases"))
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
                                            .label("Open Database…")
                                            .on_click(cx.listener(|this, _, window, cx| this.choose_file(window, cx))),
                                    )
                                    .child(
                                        Button::new("new-database")
                                            .icon(IconName::Plus)
                                            .label("New Database…")
                                            .on_click(cx.listener(|this, _, window, cx| this.new_database(window, cx))),
                                    ),
                            ),
                    ),
                ),
            )
    }
}
