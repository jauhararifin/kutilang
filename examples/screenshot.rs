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
use gpui_kit::{
    AnyWindowHandle, AsyncApp, Context, Entity, Window,
};
use kutilangv2::dates;
use kutilangv2::model::{DiagnosisFormat, PatientOrder};
use kutilangv2::store::Store;
use kutilangv2::ui::{self, KutilangApp, Section};

async fn settle(cx: &mut AsyncApp) {
    cx.background_executor().timer(Duration::from_millis(500)).await;
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
        .update(cx, |_, window, cx| app.update(cx, |app, cx| f(app, window, cx)))
        .expect("window");
}

fn main() {
    let mut args = std::env::args().skip(1);
    let db = PathBuf::from(args.next().expect("database path"));
    let out = PathBuf::from(args.next().expect("output directory"));
    let query = args.next().unwrap_or_else(|| "nur".into());
    std::fs::create_dir_all(&out).unwrap();

    gpui_kit::application().with_assets(ui::Assets).run(move |cx| {
        ui::init(cx);
        let (store, report) = Store::open(&db).expect("open database");
        let store = Rc::new(store);
        let check = store.clone();
        let patient = store
            .list_patients(&query, PatientOrder::RecentVisit)
            .unwrap()
            .into_iter()
            .next()
            .expect("a patient matching the query");

        let (window, app) = gpui_kit::open_window(ui::window_options(cx), cx, |window, cx| {
            cx.new(|cx| KutilangApp::new(store.clone(), report, db.clone(), window, cx))
        })
        .expect("open window");
        cx.activate(true);

        cx.spawn(async move |cx| {
            capture(window, &out, "01-patients", cx).await;

            update(window, &app, cx, |app, window, cx| {
                app.patients().update(cx, |p, cx| p.search(&query, window, cx))
            });
            capture(window, &out, "02-patients-search", cx).await;

            let id = patient.id.clone();
            update(window, &app, cx, move |app, window, cx| app.open_patient(&id, window, cx));
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
                    (detail.selected_visit().map(str::to_string), detail.entry_ids(cx)[0].clone())
                })
                .unwrap();
            assert_eq!(selected.as_deref(), Some(first.as_str()), "the new entry is selected");
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
            assert_eq!((after.format, &after.diagnosis), (DiagnosisFormat::Html, &old.diagnosis));
            println!("ok: an old entry saved without changes keeps its HTML");

            // The history of every visit.
            update(window, &app, cx, |app, window, cx| app.show(Section::Visits, window, cx));
            let first = check.list_visit_summaries("", None, None).unwrap()[0].id.clone();
            update(window, &app, cx, move |app, window, cx| {
                app.visits().update(cx, |v, cx| v.show_visit(&first, window, cx))
            });
            capture(window, &out, "06-visits", cx).await;
            update(window, &app, cx, |app, window, cx| {
                let today = dates::today();
                let week = today - chrono::Days::new(6);
                app.visits().update(cx, |v, cx| v.set_dates(Some(week), Some(today), window, cx))
            });
            capture(window, &out, "07-visits-last-week", cx).await;

            // Settings: a custom template is used by the next new entry.
            // Through the action behind Cmd-, and the Settings menu item.
            window
                .update(cx, |_, window, cx| window.dispatch_action(Box::new(ui::ShowSettings), cx))
                .unwrap();
            settle(cx).await;
            let section = window.update(cx, |_, _, cx| app.read(cx).section()).unwrap();
            assert_eq!(section, Section::Settings, "the Settings action switches the section");
            println!("ok: the Settings shortcut opens the settings");
            capture(window, &out, "08-settings", cx).await;
            check.set_diagnosis_template(Some("**Keluhan**: \n\n**Terapi**: \n")).unwrap();
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
            update(window, &app, cx, |app, window, cx| {
                let detail = app.patients().read(cx).detail().unwrap().clone();
                let first = detail.read(cx).entry_ids(cx)[0].clone();
                detail.update(cx, |d, cx| d.confirm_delete_entry(first, window, cx));
            });
            capture(window, &out, "10-delete-entry", cx).await;
            update(window, &app, cx, |_, window, cx| {
                use gpui_kit::component::WindowExt as _;
                window.close_all_dialogs(cx);
            });

            cx.update(|cx| cx.quit());
        })
        .detach();
    });
}
