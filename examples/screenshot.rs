//! Drives the application through its main screens and saves each one as a
//! PNG, rendered by GPUI itself (no screen recording permission needed),
//! checking the main editing flows on the way. A development aid; it writes
//! to the database, so give it a copy.
//!
//! ```sh
//! cargo run --example screenshot -- <copy-of-db.sqlite3> <output-dir> [search query]
//! ```

use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

use gpui_kit::prelude::*;
use gpui_kit::{AnyWindowHandle, AsyncApp, Context, Entity, Window};
use kutilangv2::Language;
use kutilangv2::RecentDatabases;
use kutilangv2::store::Store;
use kutilangv2::ui::{self, KutilangApp, Section, Shell};
use kutilangv2::{DiagnosisFormat, PatientOrder};

async fn settle(cx: &mut AsyncApp) {
    cx.background_executor()
        .timer(Duration::from_millis(500))
        .await;
}

/// Draws a frame now: a window that is not on screen gets no display-link
/// frames, so its last rendered scene could be stale.
fn draw(window: AnyWindowHandle, cx: &mut AsyncApp) {
    window
        .update(cx, |_, window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        })
        .expect("window");
}

async fn capture(window: AnyWindowHandle, out: &Path, name: &str, cx: &mut AsyncApp) {
    let path = out.join(format!("{name}.png"));
    // Two frames apart in time, so layout settles and animations finish.
    draw(window, cx);
    settle(cx).await;
    let result = window.update(cx, |_, window, cx| {
        window.refresh();
        let drawn = window.draw(cx);
        let image = window.render_to_image();
        drawn.clear(cx);
        image
    });
    match result {
        Ok(Ok(image)) => {
            image.save(&path).expect("save png");
            println!("wrote {}", path.display());
        }
        Ok(Err(error)) | Err(error) => eprintln!("{name}: {error:#}"),
    }
}

fn update(
    window: AnyWindowHandle,
    app: &Entity<KutilangApp>,
    cx: &mut AsyncApp,
    f: impl FnOnce(&mut KutilangApp, &mut Window, &mut Context<KutilangApp>),
) {
    window
        .update(cx, |_, window, cx| {
            app.update(cx, |app, cx| f(app, window, cx))
        })
        .expect("window");
}

fn main() {
    let mut args = std::env::args().skip(1);
    let db = PathBuf::from(args.next().expect("database path"));
    let out = PathBuf::from(args.next().expect("output directory"));
    let query = args.next().unwrap_or_else(|| "nur".into());
    std::fs::create_dir_all(&out).unwrap();
    // The language choice and the recent databases go to the output folder,
    // never to the real settings, even if the tour fails half-way.
    // SAFETY: nothing else runs yet.
    unsafe { std::env::set_var("KUTILANG_CONFIG_DIR", out.join("config")) };

    gpui_kit::application()
        .with_assets(ui::Assets)
        .run(move |cx| {
            ui::init(cx);
            let (store, _) = Store::open(&db).expect("open database");
            let store = Rc::new(store);
            let check = store.clone();
            let patient = store
                .list_patients(&query, PatientOrder::RecentVisit)
                .unwrap()
                .into_iter()
                .next()
                .expect("a patient matching the query");

            // The picker, with this database and one that was moved away.
            let recent_file = out.join("recent-databases.txt");
            let mut recent = RecentDatabases::load(recent_file.clone());
            recent.add(&out.join("moved-away.sqlite3")).unwrap();
            recent.add(&db).unwrap();
            let (window, shell) =
                gpui_kit::open_window(ui::window_options(cx), cx, |window, cx| {
                    cx.new(|cx| Shell::new(recent, None, window, cx))
                })
                .expect("open window");
            cx.activate(true);

            cx.spawn(async move |cx| {
                capture(window, &out, "00-launcher", cx).await;

                // A file that is not a Kutilang database is refused; the picker stays.
                let other = out.join("notes.sqlite3");
                rusqlite::Connection::open(&other)
                    .unwrap()
                    .execute_batch("CREATE TABLE IF NOT EXISTS notes (body TEXT);")
                    .unwrap();
                window
                    .update(cx, |_, window, cx| {
                        shell.update(cx, |s, cx| s.open(&other, window, cx))
                    })
                    .unwrap();
                settle(cx).await;
                let opened = window
                    .update(cx, |_, _, cx| shell.read(cx).app().is_some())
                    .unwrap();
                assert!(!opened, "a foreign file is not opened");
                println!("ok: a file that is not a Kutilang database is refused");

                window
                    .update(cx, |_, window, cx| {
                        shell.update(cx, |s, cx| s.open(&db, window, cx))
                    })
                    .unwrap();
                let app = window
                    .update(cx, |_, _, cx| shell.read(cx).app().cloned())
                    .unwrap()
                    .expect("opened");
                let first = RecentDatabases::load(recent_file.clone()).entries()[0]
                    .path
                    .clone();
                assert_eq!(
                    first,
                    db.canonicalize().unwrap(),
                    "the opened database is remembered first"
                );
                println!("ok: the database opened from the picker is remembered first");

                capture(window, &out, "01-patients", cx).await;

                update(window, &app, cx, |app, window, cx| {
                    app.patients()
                        .update(cx, |p, cx| p.search(&query, window, cx))
                });
                capture(window, &out, "02-patients-search", cx).await;

                let id = patient.id.clone();
                update(window, &app, cx, move |app, window, cx| {
                    app.open_patient(&id, window, cx)
                });
                capture(window, &out, "03-patient", cx).await;

                // A new entry, written through the composer.
                let markdown_entries = |store: &Store| {
                    store
                        .list_visits(&patient.id, None, 100)
                        .unwrap()
                        .items
                        .iter()
                        .filter(|v| v.format == DiagnosisFormat::Markdown)
                        .count()
                };
                let before = markdown_entries(&check);
                update(window, &app, cx, |app, window, cx| {
                    let detail = app.patients().read(cx).detail().unwrap().clone();
                    detail.update(cx, |d, cx| d.open_composer(window, cx));
                });
                capture(window, &out, "04-new-entry", cx).await;
                update(window, &app, cx, |app, window, cx| {
                    let detail = app.patients().read(cx).detail().unwrap().clone();
                    let composer = detail.read(cx).composer().unwrap().clone();
                    composer.update(cx, |c, cx| c.submit(window, cx));
                });
                settle(cx).await;
                assert_eq!(markdown_entries(&check), before + 1, "new entry stored");
                let (selected, first) = window
                    .update(cx, |_, _, cx| {
                        let detail = app.read(cx).patients().read(cx).detail().unwrap().clone();
                        let detail = detail.read(cx);
                        (
                            detail.selected_visit().map(str::to_string),
                            detail.entry_ids(cx)[0].clone(),
                        )
                    })
                    .unwrap();
                assert_eq!(
                    selected.as_deref(),
                    Some(first.as_str()),
                    "the new entry is selected"
                );
                println!("ok: a new entry is stored, listed first and shown");

                // Editing an old entry; saving it untouched keeps its HTML.
                let old = check
                    .list_visits(&patient.id, None, 100)
                    .unwrap()
                    .items
                    .into_iter()
                    .find(|v| v.format == DiagnosisFormat::Html)
                    .expect("an entry of the original application");
                let old_id = old.id.clone();
                update(window, &app, cx, move |app, window, cx| {
                    let detail = app.patients().read(cx).detail().unwrap().clone();
                    detail.update(cx, |d, cx| d.edit_entry(&old_id, window, cx));
                });
                capture(window, &out, "05-edit-old-entry", cx).await;
                let old_id = old.id.clone();
                update(window, &app, cx, move |app, window, cx| {
                    let detail = app.patients().read(cx).detail().unwrap().clone();
                    let editor = detail.read(cx).entry_editor(&old_id).unwrap();
                    editor.update(cx, |e, cx| e.submit(window, cx));
                });
                settle(cx).await;
                let after = check.get_visit(&old.id).unwrap().unwrap();
                assert_eq!(
                    (after.format, &after.diagnosis),
                    (DiagnosisFormat::Html, &old.diagnosis)
                );
                println!("ok: an old entry saved without changes keeps its HTML");

                // The history of every visit.
                update(window, &app, cx, |app, window, cx| {
                    app.show(Section::Visits, window, cx)
                });
                let first = check.list_visit_summaries("", None, None).unwrap()[0]
                    .id
                    .clone();
                update(window, &app, cx, move |app, window, cx| {
                    app.visits()
                        .update(cx, |v, cx| v.show_visit(&first, window, cx))
                });
                capture(window, &out, "06-visits", cx).await;
                update(window, &app, cx, |app, window, cx| {
                    let today = kutilangv2::today();
                    let week = today - chrono::Days::new(6);
                    app.visits()
                        .update(cx, |v, cx| v.set_dates(Some(week), Some(today), window, cx))
                });
                capture(window, &out, "07-visits-last-week", cx).await;

                // Settings: a custom template is used by the next new entry.
                // Through the action behind Cmd-, and the Settings menu item.
                window
                    .update(cx, |_, window, cx| {
                        window.dispatch_action(Box::new(ui::ShowSettings), cx)
                    })
                    .unwrap();
                settle(cx).await;
                let section = window
                    .update(cx, |_, _, cx| app.read(cx).section())
                    .unwrap();
                assert_eq!(
                    section,
                    Section::Settings,
                    "the Settings action switches the section"
                );
                println!("ok: the Settings shortcut opens the settings");
                capture(window, &out, "08-settings", cx).await;
                check
                    .set_diagnosis_template(Some("**Keluhan**: \n\n**Terapi**: \n"))
                    .unwrap();
                let composer_text = window
                    .update(cx, |_, window, cx| {
                        app.update(cx, |app, cx| app.open_patient(&patient.id, window, cx));
                        let detail = app.read(cx).patients().read(cx).detail().unwrap().clone();
                        detail.update(cx, |d, cx| d.open_composer(window, cx));
                        let composer = detail.read(cx).composer().unwrap().clone();
                        composer.read(cx).content(cx).to_string()
                    })
                    .unwrap();
                check.set_diagnosis_template(None).unwrap();
                assert_eq!(composer_text, "**Keluhan**: \n\n**Terapi**: \n");
                println!("ok: the composer starts from the saved template");

                // Backup and restore: a patient added after the backup is gone
                // once it is restored, and the data before the restore is kept.
                let backup = out.join("backup.sqlite3");
                check.backup_to(&backup).unwrap();
                let counts_before = window.update(cx, |_, _, cx| app.read(cx).counts()).unwrap();
                let extra = check
                    .create_patient(
                        &kutilangv2::PatientInput {
                            name: "Zztest Pasien".into(),
                            head_family_name: String::new(),
                            birth_date: "1990-01-01".into(),
                            address: "Tes".into(),
                        }
                        .validate(kutilangv2::today())
                        .unwrap(),
                        kutilangv2::today(),
                    )
                    .unwrap();
                update(window, &app, cx, |app, window, cx| {
                    app.show(Section::Settings, window, cx);
                    let settings = app.settings().clone();
                    let backup = backup.clone();
                    settings.update(cx, |s, cx| s.confirm_restore(&backup, window, cx));
                });
                capture(window, &out, "11-restore-confirm", cx).await;
                update(window, &app, cx, |app, window, cx| {
                    use gpui_kit::component::WindowExt as _;
                    window.close_all_dialogs(cx);
                    let opened = Store::open_backup(&backup).unwrap();
                    let settings = app.settings().clone();
                    settings.update(cx, |s, cx| s.apply_restore(opened, window, cx));
                });
                settle(cx).await;
                assert!(
                    check.get_patient(&extra.id).unwrap().is_none(),
                    "restored over the extra patient"
                );
                let counts_after = window.update(cx, |_, _, cx| app.read(cx).counts()).unwrap();
                assert_eq!(
                    counts_after, counts_before,
                    "the sidebar counts follow the restore"
                );
                let kept = std::fs::read_dir(db.parent().unwrap())
                    .unwrap()
                    .filter_map(|e| e.ok())
                    .any(|e| e.file_name().to_string_lossy().contains("before-restore"));
                assert!(kept, "the data before the restore was saved");
                println!(
                    "ok: restore replaces the data, refreshes the views and keeps the previous data"
                );

                let started = std::time::Instant::now();
                let report =
                    kutilangv2::store::export_xlsx(&check, &out.join("export.xlsx")).unwrap();
                println!(
                    "ok: exported {} patients and {} visits in {:?}",
                    report.patients,
                    report.visits,
                    started.elapsed()
                );

                // Dialogs.
                update(window, &app, cx, |app, window, cx| {
                    app.show(Section::Patients, window, cx);
                    app.patients().update(cx, |p, cx| p.new_patient(window, cx));
                });
                settle(cx).await;
                capture(window, &out, "09-new-patient", cx).await;
                update(window, &app, cx, |_, window, cx| {
                    use gpui_kit::component::WindowExt as _;
                    window.close_all_dialogs(cx);
                });
                let id = patient.id.clone();
                update(window, &app, cx, move |app, window, cx| {
                    // The restore rebuilt the views: open the patient again.
                    app.open_patient(&id, window, cx);
                    let detail = app.patients().read(cx).detail().unwrap().clone();
                    let first = detail.read(cx).entry_ids(cx)[0].clone();
                    detail.update(cx, |d, cx| d.confirm_delete_entry(first, window, cx));
                });
                capture(window, &out, "10-delete-entry", cx).await;
                update(window, &app, cx, |_, window, cx| {
                    use gpui_kit::component::WindowExt as _;
                    window.close_all_dialogs(cx);
                });

                // A visit being written is not lost by choosing another patient.
                let other = check
                    .list_patients("", PatientOrder::RecentVisit)
                    .unwrap()
                    .into_iter()
                    .find(|p| p.id != patient.id)
                    .expect("another patient")
                    .id;
                let id = patient.id.clone();
                update(window, &app, cx, move |app, window, cx| {
                    app.open_patient(&id, window, cx);
                    let detail = app.patients().read(cx).detail().unwrap().clone();
                    detail.update(cx, |d, cx| d.open_composer(window, cx));
                });
                settle(cx).await;
                window
                    .update(cx, |_, window, cx| {
                        window.dispatch_keystroke(gpui_kit::Keystroke::parse("x").unwrap(), cx)
                    })
                    .unwrap();
                settle(cx).await;
                let unsaved = window
                    .update(cx, |_, _, cx| app.read(cx).has_unsaved_changes(cx))
                    .unwrap();
                assert!(unsaved, "typing in the composer is an unsaved change");
                let other_id = other.clone();
                update(window, &app, cx, move |app, window, cx| {
                    app.open_patient(&other_id, window, cx)
                });
                settle(cx).await;
                let (shown, asking) = window
                    .update(cx, |_, window, cx| {
                        use gpui_kit::component::WindowExt as _;
                        let detail = app.read(cx).patients().read(cx).detail().unwrap().clone();
                        let shown = detail.read(cx).patient_id().to_string();
                        (shown, window.has_active_dialog(cx))
                    })
                    .unwrap();
                assert_eq!(shown, patient.id, "the patient being written for stays");
                assert!(asking, "the user is asked before the text is lost");
                capture(window, &out, "14-discard-changes", cx).await;
                // Answer "Cancel", then leave the draft.
                update(window, &app, cx, |app, window, cx| {
                    use gpui_kit::component::WindowExt as _;
                    window.close_all_dialogs(cx);
                    let detail = app.patients().read(cx).detail().unwrap().clone();
                    let first = detail.read(cx).entry_ids(cx)[0].clone();
                    detail.update(cx, |d, cx| d.select_visit(&first, window, cx));
                });
                settle(cx).await;
                let unsaved = window
                    .update(cx, |_, _, cx| app.read(cx).has_unsaved_changes(cx))
                    .unwrap();
                assert!(!unsaved);
                println!("ok: choosing another patient asks before losing a visit being written");

                // Switching the language redraws the views in English, and back.
                drop(app);
                for language in [Language::English, Language::Indonesian] {
                    let current = window
                        .update(cx, |_, _, cx| shell.read(cx).app().cloned())
                        .unwrap()
                        .expect("still open");
                    update(window, &current, cx, move |app, window, cx| {
                        // The language is chosen in the settings.
                        app.show(Section::Settings, window, cx);
                        app.settings()
                            .update(cx, |s, cx| s.set_language(language, cx))
                    });
                    settle(cx).await;
                    let fresh = window
                        .update(cx, |_, _, cx| shell.read(cx).app().cloned())
                        .unwrap()
                        .expect("still open");
                    let section = window
                        .update(cx, |_, _, cx| fresh.read(cx).section())
                        .unwrap();
                    assert_eq!(section, Section::Settings, "the settings stay open");
                    if language == Language::English {
                        capture(window, &out, "12-settings-english", cx).await;
                        update(window, &fresh, cx, |app, window, cx| {
                            app.show(Section::Patients, window, cx)
                        });
                        capture(window, &out, "13-patients-english", cx).await;
                        update(window, &fresh, cx, |app, window, cx| {
                            app.show(Section::Settings, window, cx)
                        });
                    }
                }
                assert_eq!(kutilangv2::language(), Language::Indonesian);
                println!(
                    "ok: switching the language redraws the views and keeps the settings open"
                );

                // Shortcuts work in a section without a text field to focus.
                let app = window
                    .update(cx, |_, _, cx| shell.read(cx).app().cloned())
                    .unwrap()
                    .expect("still open");
                draw(window, cx);
                window
                    .update(cx, |_, window, cx| {
                        window.dispatch_action(Box::new(ui::ShowVisits), cx)
                    })
                    .unwrap();
                settle(cx).await;
                let section = window
                    .update(cx, |_, _, cx| app.read(cx).section())
                    .unwrap();
                assert_eq!(section, Section::Visits, "Cmd-2 works from the settings");
                println!("ok: shortcuts work from the settings");

                // Closing the database releases it (no view keeps it alive).
                let store = window
                    .update(cx, |_, _, cx| Rc::downgrade(app.read(cx).store()))
                    .unwrap();
                drop(app);
                // Actions go through the last frame; a window on screen draws
                // one after every change, this one only when asked.
                draw(window, cx);
                window
                    .update(cx, |_, window, cx| {
                        window.dispatch_action(Box::new(ui::CloseDatabase), cx)
                    })
                    .unwrap();
                settle(cx).await;
                draw(window, cx);
                let (closed, can_open) = window
                    .update(cx, |_, window, cx| {
                        (
                            shell.read(cx).app().is_none(),
                            window.is_action_available(&ui::OpenDatabase, cx),
                        )
                    })
                    .unwrap();
                assert!(closed, "Close Database shows the picker");
                assert!(can_open, "Cmd-O works on the picker");
                assert!(store.upgrade().is_none(), "the closed database is released");
                println!("ok: closing the database releases it, and Cmd-O works on the picker");

                cx.update(|cx| cx.quit());
            })
            .detach();
        });
}
