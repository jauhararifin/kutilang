use std::path::Path;

use chrono::{NaiveDate, TimeZone as _};
use kutilangv2::diagnosis::TEMPLATE;
use kutilangv2::store::{SCHEMA_VERSION, Store};
use kutilangv2::{DiagnosisFormat, PatientInput, PatientOrder, PatientProfile};
use rusqlite::{Connection, params};

fn date(y: i32, m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, d).unwrap()
}

fn profile(name: &str, family: &str) -> PatientProfile {
    PatientInput {
        name: name.into(),
        head_family_name: family.into(),
        birth_date: "1980-05-17".into(),
        address: "Jl. Kenanga 2".into(),
    }
    .validate(date(2026, 11, 9))
    .unwrap()
}

fn user_version(conn: &Connection) -> i64 {
    conn.query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap()
}

fn tables(conn: &Connection) -> Vec<String> {
    let mut stmt = conn
        .prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")
        .unwrap();
    stmt.query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

/// The schema of the released Go application (`kutilang_amd64.exe`).
const GO_SCHEMA: &str = r#"
create table if not exists patients (
	id text primary key unique on conflict fail,
	name text,
	head_family_name text,
	birth_date text,
	address text,
	last_visit text,
	is_deleted bool
);
create index if not exists patients_id_idx on patients(last_visit, id);
create table if not exists patient_indices (
	keyword text,
	last_visit integer,
	patient_id text
);
create index if not exists patient_indices_idx on patient_indices(keyword, last_visit, patient_id);
CREATE TABLE if not exists visits (
	id text primary key unique on conflict fail,
	patient_id text,
	date text,
	diagnosis text,
	is_deleted bool
);
create index if not exists visits_patient_id_idx on visits(patient_id, date);
create index if not exists visits_id_idx on visits(id);
"#;

/// The earlier Go schema, with Unix timestamps.
const GO_TIMESTAMP_SCHEMA: &str = r#"
create table if not exists patients (
	id text primary key,
	name text,
	head_family_name text,
	birth_date text,
	address text,
	last_visit integer,
	is_deleted bool
);
create index if not exists patients_id_idx on patients(id, last_visit);
create table if not exists patient_indices (
	keyword text,
	last_visit integer,
	patient_id text
);
create index if not exists patient_indices_idx on patient_indices(keyword, last_visit, patient_id);
create table if not exists visits (
	id text primary key,
	patient_id text,
	timestamp integer,
	diagnosis text,
	is_deleted bool
);
create index if not exists visits_patient_id_idx on visits(patient_id, timestamp);
create index if not exists visits_id_idx on visits(id);
"#;

const HTML_A1: &str = "<h5>Subjektif</h5>\n<p><strong>KU</strong>: lemas&nbsp;</p>\n";
const HTML_A2: &str = "<ul><li><strong>TD</strong>: 120/80<br></li></ul>\n";

fn write_go_database(path: &Path) {
    let conn = Connection::open(path).unwrap();
    conn.execute_batch(GO_SCHEMA).unwrap();
    let patient = "INSERT INTO patients VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)";
    conn.execute(
        patient,
        params![
            "a",
            "Siti Aminah",
            "Joko",
            "1990-01-05",
            "Jl. Mawar",
            "2021-08-23",
            false
        ],
    )
    .unwrap();
    conn.execute(
        patient,
        params![
            "b",
            "Ma'ruf Amin,",
            "",
            "1960-12-12",
            "Desa",
            "2022-01-01",
            true
        ],
    )
    .unwrap();
    conn.execute(
        patient,
        params![
            "c",
            "Budi",
            "Santoso",
            "2003-12-11",
            "Kota",
            "2026-01-01",
            false
        ],
    )
    .unwrap();
    let visit = "INSERT INTO visits VALUES (?1, ?2, ?3, ?4, ?5)";
    conn.execute(visit, params!["v1", "a", "2021-08-23", HTML_A1, false])
        .unwrap();
    // Re-dated after it was written: patients.last_visit was not updated.
    conn.execute(visit, params!["v2", "a", "2024-03-01", HTML_A2, false])
        .unwrap();
    conn.execute(
        visit,
        params!["v3", "a", "2025-01-01", "<p>deleted</p>", true],
    )
    .unwrap();
    conn.execute(visit, params!["v4", "b", "2022-01-01", "<p>b</p>", false])
        .unwrap();
    conn.execute(
        visit,
        params!["v5", "gone", "2022-02-02", "<p>orphan</p>", false],
    )
    .unwrap();
    // The index the Go app kept, including its quirks.
    let index = "INSERT INTO patient_indices VALUES (?1, ?2, ?3)";
    conn.execute(index, params!["siti", "", "a"]).unwrap();
    conn.execute(index, params!["budi", 1629726702, "c"])
        .unwrap();
}

#[test]
fn creates_a_fresh_database() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("nested/db.sqlite3");
    let (store, report) = Store::open(&path).unwrap();
    assert!(report.is_none());
    assert_eq!(user_version(store.connection()), SCHEMA_VERSION);
    assert_eq!(store.count_patients().unwrap(), 0);
    drop(store);

    let (_, report) = Store::open(&path).unwrap();
    assert!(report.is_none());
}

#[test]
fn patients_and_visits_round_trip() {
    let store = Store::open_in_memory().unwrap();
    let today = date(2026, 11, 9);
    let patient = store
        .create_patient(&profile("Siti Ma'ruf", "Joko"), today)
        .unwrap();
    assert_eq!(patient.id.len(), 32);
    assert_eq!(patient.last_visit, "2026-11-09");
    assert_eq!(patient.birth_date, "1980-05-17");

    let other = store
        .create_patient(&profile("Sinta", "Budi"), today)
        .unwrap();
    let ids = |query: &str| {
        let mut ids: Vec<String> = store
            .list_patients(query, PatientOrder::RecentVisit)
            .unwrap()
            .into_iter()
            .map(|p| p.id)
            .collect();
        ids.sort();
        ids
    };
    // Every word must start a word of the name or head of family name.
    for query in [
        "siti",
        "SITI",
        "sit",
        "maruf",
        "ma'ruf",
        "Ma’ruf",
        "jo",
        "siti joko",
        "joko m",
    ] {
        assert_eq!(
            ids(query),
            std::slice::from_ref(&patient.id),
            "query {query:?}"
        );
    }
    let mut both = vec![patient.id.clone(), other.id.clone()];
    both.sort();
    assert_eq!(ids("s"), both);
    assert_eq!(ids("  "), both, "an empty query lists everyone");
    assert!(ids("siti budi").is_empty());
    assert!(ids("x").is_empty());
    store.delete_patient(&other.id).unwrap();

    // Visits, newest first; an older visit does not move last_visit back.
    let v1 = store
        .add_visit(&patient.id, date(2026, 11, 10), "**KU**: baik")
        .unwrap();
    let v2 = store
        .add_visit(&patient.id, date(2026, 1, 1), "lama")
        .unwrap();
    assert_eq!(v1.format, DiagnosisFormat::Markdown);
    let visits = store.list_visits(&patient.id, None, 30).unwrap();
    assert_eq!(
        visits
            .items
            .iter()
            .map(|v| v.id.as_str())
            .collect::<Vec<_>>(),
        [&v1.id, &v2.id]
    );
    assert!(visits.next.is_none());
    assert_eq!(
        store.get_patient(&patient.id).unwrap().unwrap().last_visit,
        "2026-11-10"
    );

    // Re-dating a visit later moves last_visit forward.
    let v2 = store.update_visit(&v2.id, date(2026, 12, 1), None).unwrap();
    assert_eq!(v2.diagnosis, "lama");
    assert_eq!(
        store.get_patient(&patient.id).unwrap().unwrap().last_visit,
        "2026-12-01"
    );
    let v2 = store
        .update_visit(&v2.id, date(2026, 12, 1), Some("baru"))
        .unwrap();
    assert_eq!(v2.diagnosis, "baru");
    assert!(
        store
            .update_visit(&v2.id, date(2026, 12, 1), Some("  "))
            .is_err()
    );
    assert!(store.add_visit(&patient.id, today, "\n").is_err());

    store.delete_visit(&v1.id).unwrap();
    assert!(store.delete_visit(&v1.id).is_err());
    assert_eq!(
        store
            .list_visits(&patient.id, None, 30)
            .unwrap()
            .items
            .len(),
        1
    );

    // Renaming re-indexes.
    let renamed = store
        .update_patient(&patient.id, &profile("Rina", ""))
        .unwrap();
    assert_eq!(renamed.name, "Rina");
    assert!(ids("siti").is_empty());
    assert_eq!(ids("rina").len(), 1);

    store.delete_patient(&patient.id).unwrap();
    assert!(store.get_patient(&patient.id).unwrap().is_none());
    assert!(ids("rina").is_empty());
    assert!(store.add_visit(&patient.id, today, "x").is_err());
    assert!(
        store
            .update_patient(&patient.id, &profile("X", ""))
            .is_err()
    );
}

#[test]
fn lists_are_ordered_and_visits_paginate_by_date() {
    let store = Store::open_in_memory().unwrap();
    let mut ids = Vec::new();
    for i in 0..65 {
        let patient = store
            .create_patient(&profile(&format!("Nur {i:02}"), ""), date(2020, 1, 1))
            .unwrap();
        // Distinct last visits, a few sharing a date to exercise the id tie-break.
        store
            .add_visit(
                &patient.id,
                date(2021, 1, 1) + chrono::Days::new(i / 2),
                "x",
            )
            .unwrap();
        ids.push(patient.id);
    }
    let recent = store
        .list_patients("nur", PatientOrder::RecentVisit)
        .unwrap();
    assert_eq!(recent.len(), 65);
    let keys: Vec<_> = recent
        .iter()
        .map(|p| (p.last_visit.clone(), p.id.clone()))
        .collect();
    let mut sorted = keys.clone();
    sorted.sort_by(|a, b| b.cmp(a));
    assert_eq!(keys, sorted, "most recent first");
    let by_name = store.list_patients("", PatientOrder::Name).unwrap();
    assert_eq!(by_name.first().unwrap().name, "Nur 00");
    assert_eq!(by_name.last().unwrap().name, "Nur 64");

    let patient = &ids[0];
    for i in 0..31 {
        store
            .add_visit(patient, date(2022, 1, 1) + chrono::Days::new(i), "y")
            .unwrap();
    }
    let first = store.list_visits(patient, None, 30).unwrap();
    assert_eq!(first.items.len(), 30);
    let second = store.list_visits(patient, first.next.as_ref(), 30).unwrap();
    assert_eq!(second.items.len(), 2);
    assert!(second.next.is_none());
}

#[test]
fn migrates_the_go_database() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db.sqlite3");
    write_go_database(&path);

    let (store, report) = Store::open(&path).unwrap();
    let report = report.expect("a migration");
    assert_eq!(report.patients, 3);
    assert_eq!(report.visits, 4);
    assert_eq!(report.orphan_visits, 1);
    assert_eq!(report.corrected_last_visits, 1);

    // The backup is the untouched original.
    let backup = report.backup_path.clone().unwrap();
    assert_eq!(backup.parent(), path.parent());
    let original = Connection::open(&backup).unwrap();
    assert_eq!(user_version(&original), 0);
    assert_eq!(tables(&original), ["patient_indices", "patients", "visits"]);
    let backup_visits: i64 = original
        .query_row("SELECT count(*) FROM visits", [], |r| r.get(0))
        .unwrap();
    assert_eq!(backup_visits, 5);

    let conn = store.connection();
    assert_eq!(user_version(conn), SCHEMA_VERSION);
    assert_eq!(
        tables(conn),
        ["meta", "patient_keywords", "patients", "settings", "visits"]
    );
    let source: String = conn
        .query_row(
            "SELECT value FROM meta WHERE key = 'migrated_from'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(source.starts_with("kutilang-go"));

    // Data is carried over; the stale last visit is corrected.
    let a = store.get_patient("a").unwrap().unwrap();
    assert_eq!(
        (a.name.as_str(), a.head_family_name.as_str()),
        ("Siti Aminah", "Joko")
    );
    assert_eq!(a.birth_date, "1990-01-05");
    assert_eq!(a.last_visit, "2024-03-01");
    assert_eq!(
        store.get_patient("c").unwrap().unwrap().last_visit,
        "2026-01-01"
    );
    assert!(
        store.get_patient("b").unwrap().is_none(),
        "deleted patients stay hidden"
    );

    let visits = store.list_visits("a", None, 30).unwrap().items;
    assert_eq!(
        visits.iter().map(|v| v.id.as_str()).collect::<Vec<_>>(),
        ["v2", "v1"]
    );
    assert_eq!(visits[1].diagnosis, HTML_A1, "HTML kept byte for byte");
    assert!(visits.iter().all(|v| v.format == DiagnosisFormat::Html));

    // The search index is rebuilt from the names.
    let ids = |q: &str| {
        store
            .list_patients(q, PatientOrder::RecentVisit)
            .unwrap()
            .into_iter()
            .map(|p| p.id)
            .collect::<Vec<_>>()
    };
    assert_eq!(ids("siti joko"), ["a"]);
    // Most recently visited first: c (2026-01-01) before a (2024-03-01).
    assert_eq!(ids(""), ["c", "a"]);
    assert!(ids("maruf").is_empty());

    // Editing only the date keeps the original HTML.
    let edited = store.update_visit("v1", date(2021, 8, 24), None).unwrap();
    assert_eq!(
        (edited.diagnosis.as_str(), edited.format),
        (HTML_A1, DiagnosisFormat::Html)
    );
    let edited = store
        .update_visit("v1", date(2021, 8, 24), Some("**KU**: lemas"))
        .unwrap();
    assert_eq!(edited.format, DiagnosisFormat::Markdown);

    // Reopening does not migrate again.
    drop(store);
    let (_, report) = Store::open(&path).unwrap();
    assert!(report.is_none());
}

#[test]
fn migrates_the_timestamp_layout() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db.sqlite3");
    let conn = Connection::open(&path).unwrap();
    conn.execute_batch(GO_TIMESTAMP_SCHEMA).unwrap();
    let last_visit: i64 = 1629726702;
    let visit_time: i64 = 1629728011;
    conn.execute(
        "INSERT INTO patients VALUES ('a', 'Joko', 'Suwarso', '1970-01-01', 'Desa', ?1, 0)",
        [last_visit],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO visits VALUES ('v', 'a', ?1, '<p>x</p>', 0)",
        [visit_time],
    )
    .unwrap();
    drop(conn);

    let local = |ts: i64| {
        chrono::Local
            .timestamp_opt(ts, 0)
            .unwrap()
            .date_naive()
            .format("%Y-%m-%d")
            .to_string()
    };
    let (store, report) = Store::open(&path).unwrap();
    assert_eq!(report.unwrap().visits, 1);
    let patient = store.get_patient("a").unwrap().unwrap();
    assert_eq!(patient.last_visit, local(last_visit).max(local(visit_time)));
    assert_eq!(
        store.list_visits("a", None, 30).unwrap().items[0].date,
        local(visit_time)
    );
    assert_eq!(
        store
            .list_patients("suwarso", PatientOrder::RecentVisit)
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn refuses_a_database_from_a_newer_version() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db.sqlite3");
    let (store, _) = Store::open(&path).unwrap();
    store
        .connection()
        .pragma_update(None, "user_version", SCHEMA_VERSION + 1)
        .unwrap();
    drop(store);
    let error = Store::open(&path).err().unwrap().to_string();
    assert!(error.contains("newer version"), "{error}");
}

#[test]
fn leaves_unknown_databases_alone() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("other.sqlite3");
    let conn = Connection::open(&path).unwrap();
    conn.execute_batch("CREATE TABLE notes (body TEXT); INSERT INTO notes VALUES ('keep');")
        .unwrap();
    drop(conn);

    assert!(Store::open(&path).is_err());
    let conn = Connection::open(&path).unwrap();
    assert_eq!(tables(&conn), ["notes"]);
    assert_eq!(user_version(&conn), 0);
    assert_eq!(
        std::fs::read_dir(dir.path()).unwrap().count(),
        1,
        "no backup written"
    );
}

#[test]
fn lists_every_visit_with_filters() {
    let store = Store::open_in_memory().unwrap();
    let siti = store
        .create_patient(&profile("Siti Aminah", "Joko"), date(2026, 1, 1))
        .unwrap();
    let budi = store
        .create_patient(&profile("Budi", ""), date(2026, 1, 1))
        .unwrap();
    store
        .add_visit(&siti.id, date(2026, 11, 1), "**KU**: demam\n**RPS**: \n")
        .unwrap();
    store
        .add_visit(
            &budi.id,
            date(2026, 11, 5),
            "##### Subjektif\n\n**KU**: batuk",
        )
        .unwrap();
    let gone = store
        .add_visit(&budi.id, date(2026, 11, 6), "hidden")
        .unwrap();
    store.delete_visit(&gone.id).unwrap();
    store
        .add_visit(&budi.id, date(2026, 10, 1), "lama")
        .unwrap();
    assert_eq!(store.count_visits().unwrap(), 3);
    assert_eq!(store.count_patient_visits(&budi.id).unwrap(), 2);

    let all = store.list_visit_summaries("", None, None).unwrap();
    let dates: Vec<_> = all.iter().map(|v| v.date.as_str()).collect();
    assert_eq!(dates, ["2026-11-05", "2026-11-01", "2026-10-01"]);
    assert_eq!(all[0].patient_name, "Budi");
    assert_eq!(all[0].summary, "KU: batuk");
    assert_eq!(all[1].summary, "KU: demam");

    let november = store
        .list_visit_summaries("", Some(date(2026, 11, 1)), Some(date(2026, 11, 30)))
        .unwrap();
    assert_eq!(november.len(), 2);
    let siti_only = store.list_visit_summaries("sit jo", None, None).unwrap();
    assert_eq!(siti_only.len(), 1);
    assert_eq!(siti_only[0].patient_id, siti.id);

    // Visits of a deleted patient are not listed.
    store.delete_patient(&budi.id).unwrap();
    assert_eq!(store.list_visit_summaries("", None, None).unwrap().len(), 1);
    assert_eq!(store.count_visits().unwrap(), 1);
}

#[test]
fn entry_template_can_be_set_up() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db.sqlite3");
    let (store, _) = Store::open(&path).unwrap();
    assert_eq!(store.diagnosis_template(), TEMPLATE);
    assert!(!store.has_custom_template());

    store
        .set_diagnosis_template(Some("**Keluhan**: \n"))
        .unwrap();
    drop(store);
    let (store, _) = Store::open(&path).unwrap();
    assert_eq!(
        store.diagnosis_template(),
        "**Keluhan**: \n",
        "kept across restarts"
    );
    assert!(store.has_custom_template());

    // Saving the built-in template, or none, goes back to the default.
    store.set_diagnosis_template(Some(TEMPLATE)).unwrap();
    assert!(!store.has_custom_template());
    store.set_diagnosis_template(Some("x")).unwrap();
    store.set_diagnosis_template(None).unwrap();
    assert_eq!(store.diagnosis_template(), TEMPLATE);
}

#[test]
fn upgrades_a_version_2_database() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db.sqlite3");
    // A database written by the first release of this app (schema 2).
    write_go_database(&path);
    {
        let (store, _) = Store::open(&path).unwrap();
        store
            .connection()
            .execute_batch("DROP TABLE settings; PRAGMA user_version = 2;")
            .unwrap();
    }
    let (store, report) = Store::open(&path).unwrap();
    assert!(report.is_none(), "not a migration from the Go application");
    assert_eq!(user_version(store.connection()), SCHEMA_VERSION);
    assert!(tables(store.connection()).contains(&"settings".to_string()));
    assert_eq!(store.get_patient("a").unwrap().unwrap().name, "Siti Aminah");
    store.set_diagnosis_template(Some("x")).unwrap();
}

#[test]
fn backs_up_and_restores_everything() {
    let dir = tempfile::tempdir().unwrap();
    let (store, _) = Store::open(&dir.path().join("db.sqlite3")).unwrap();
    let siti = store
        .create_patient(&profile("Siti", "Joko"), date(2026, 1, 1))
        .unwrap();
    store
        .add_visit(&siti.id, date(2026, 11, 1), "**KU**: demam")
        .unwrap();
    store
        .set_diagnosis_template(Some("**Keluhan**: \n"))
        .unwrap();

    let backup = dir.path().join("backup.sqlite3");
    std::fs::write(&backup, b"an older file").unwrap();
    store.backup_to(&backup).unwrap();

    // Changes after the backup are undone by restoring it.
    let budi = store
        .create_patient(&profile("Budi", ""), date(2026, 1, 2))
        .unwrap();
    store
        .add_visit(&budi.id, date(2026, 11, 2), "batuk")
        .unwrap();
    store.set_diagnosis_template(None).unwrap();
    store.delete_patient(&siti.id).unwrap();

    let opened = Store::open_backup(&backup).unwrap();
    assert_eq!((opened.patients, opened.visits), (1, 1));
    store.restore(&opened).unwrap();
    assert_eq!(store.count_patients().unwrap(), 1);
    assert_eq!(store.count_visits().unwrap(), 1);
    assert!(store.get_patient(&budi.id).unwrap().is_none());
    assert_eq!(store.get_patient(&siti.id).unwrap().unwrap().name, "Siti");
    assert_eq!(
        store
            .list_patients("siti", PatientOrder::Name)
            .unwrap()
            .len(),
        1
    );
    assert_eq!(store.diagnosis_template(), "**Keluhan**: \n");
    let restored_from: String = store
        .connection()
        .query_row(
            "SELECT value FROM meta WHERE key = 'restored_from'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(restored_from.ends_with("backup.sqlite3"));
}

#[test]
fn restores_a_database_of_the_go_application_without_changing_it() {
    let dir = tempfile::tempdir().unwrap();
    let old = dir.path().join("old.sqlite3");
    write_go_database(&old);
    let before = std::fs::read(&old).unwrap();

    let store = Store::open_in_memory().unwrap();
    let opened = Store::open_backup(&old).unwrap();
    assert_eq!((opened.patients, opened.visits), (2, 2));
    store.restore(&opened).unwrap();
    assert_eq!(
        store.get_patient("a").unwrap().unwrap().last_visit,
        "2024-03-01"
    );
    assert_eq!(
        std::fs::read(&old).unwrap(),
        before,
        "the chosen file is not modified"
    );
    assert_eq!(
        std::fs::read_dir(dir.path()).unwrap().count(),
        1,
        "no backup written next to it"
    );
}

#[test]
fn refuses_to_restore_other_files() {
    let dir = tempfile::tempdir().unwrap();
    let other = dir.path().join("notes.sqlite3");
    Connection::open(&other)
        .unwrap()
        .execute_batch("CREATE TABLE notes (body TEXT);")
        .unwrap();
    let text = dir.path().join("notes.txt");
    std::fs::write(&text, "hello").unwrap();
    let store = Store::open_in_memory().unwrap();
    store
        .create_patient(&profile("Siti", ""), date(2026, 1, 1))
        .unwrap();
    assert!(Store::open_backup(&other).is_err());
    assert!(Store::open_backup(&text).is_err());
    assert_eq!(store.count_patients().unwrap(), 1, "nothing replaced");
}

#[test]
fn last_visit_follows_edited_and_deleted_visits() {
    let store = Store::open_in_memory().unwrap();
    let siti = store
        .create_patient(&profile("Siti", ""), date(2026, 1, 1))
        .unwrap();
    let last_visit = |store: &Store| store.get_patient(&siti.id).unwrap().unwrap().last_visit;
    // Dates after the registration date, whatever day the tests run.
    let first = store
        .add_visit(&siti.id, date(2090, 3, 1), "kontrol")
        .unwrap();
    let typo = store
        .add_visit(&siti.id, date(2099, 3, 2), "tahun salah ketik")
        .unwrap();
    assert_eq!(last_visit(&store), "2099-03-02");

    // Correcting the date brings the last visit back.
    store
        .update_visit(&typo.id, date(2091, 3, 2), None)
        .unwrap();
    assert_eq!(last_visit(&store), "2091-03-02");

    // Deleting the newest visit falls back to the one before.
    store.delete_visit(&typo.id).unwrap();
    assert_eq!(last_visit(&store), "2090-03-01");

    // With no visit left, the registration date.
    store.delete_visit(&first.id).unwrap();
    let registered = last_visit(&store);
    assert!(registered.as_str() < "2090", "{registered}");
    assert_eq!(registered, kutilangv2::to_stored(kutilangv2::today()));
}

#[test]
fn never_overwrites_the_open_database() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db.sqlite3");
    let (store, _) = Store::open(&path).unwrap();
    store
        .create_patient(&profile("Siti", ""), date(2026, 1, 1))
        .unwrap();

    // The same file, also under another name.
    let same = dir.path().join(".").join("db.sqlite3");
    assert!(store.backup_to(&same).is_err());
    assert!(kutilangv2::store::export_xlsx(&store, &same).is_err());

    assert!(
        std::fs::read(&path)
            .unwrap()
            .starts_with(b"SQLite format 3")
    );
    assert_eq!(store.count_patients().unwrap(), 1);
    drop(store);
    let (reopened, _) = Store::open(&path).unwrap();
    assert_eq!(reopened.count_patients().unwrap(), 1);
}

#[test]
fn exports_patients_and_visits_to_excel() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open_in_memory().unwrap();
    let siti = store
        .create_patient(&profile("Siti", "Joko"), date(2026, 1, 1))
        .unwrap();
    store
        .add_visit(
            &siti.id,
            date(2026, 11, 1),
            "##### Subjektif\n\n**KU**: demam",
        )
        .unwrap();
    store
        .add_visit(&siti.id, date(8, 1, 29), "tanggal salah ketik")
        .unwrap();
    let gone = store
        .create_patient(&profile("Budi", ""), date(2026, 1, 1))
        .unwrap();
    store.delete_patient(&gone.id).unwrap();

    let exported = store.export_visits().unwrap();
    assert_eq!(exported.len(), 2);
    assert_eq!(store.export_patients().unwrap()[0].visits, 2);

    let path = dir.path().join("kutilang.xlsx");
    let report = kutilangv2::store::export_xlsx(&store, &path).unwrap();
    assert_eq!((report.patients, report.visits), (1, 2));
    let bytes = std::fs::read(&path).unwrap();
    assert!(bytes.starts_with(b"PK"), "an xlsx (zip) file");
}

/// Checks against a real database, which is not part of the repository.
///
/// ```sh
/// KUTILANG_REAL_DB=/path/to/copy/of/db.sqlite3 cargo test --test store real_data -- --ignored
/// ```
mod real_data {
    use html5ever::tendril::TendrilSink as _;
    use html5ever::{ParseOpts, parse_document};
    use kutilangv2::diagnosis::{html_to_markdown, with_line_breaks};
    use markup5ever_rcdom::{Handle, NodeData, RcDom};
    use rusqlite::{Connection, OpenFlags};

    /// Non-whitespace text of an HTML fragment, and the part of it that is bold.
    fn text_and_bold(html: &str) -> (String, String) {
        fn walk(node: &Handle, bold: bool, text: &mut String, bold_text: &mut String) {
            match &node.data {
                NodeData::Text { contents } => {
                    for c in contents.borrow().chars().filter(|c| !c.is_whitespace()) {
                        text.push(c);
                        if bold {
                            bold_text.push(c);
                        }
                    }
                }
                NodeData::Element { name, .. } => {
                    let tag = name.local.as_ref();
                    if tag == "script" || tag == "style" {
                        return;
                    }
                    let bold = bold || tag == "strong" || tag == "b";
                    for child in node.children.borrow().iter() {
                        walk(child, bold, text, bold_text);
                    }
                }
                _ => {
                    for child in node.children.borrow().iter() {
                        walk(child, bold, text, bold_text);
                    }
                }
            }
        }
        let dom = parse_document(RcDom::default(), ParseOpts::default()).one(html);
        let (mut text, mut bold) = (String::new(), String::new());
        walk(&dom.document, false, &mut text, &mut bold);
        (text, bold)
    }

    #[test]
    #[ignore = "needs KUTILANG_REAL_DB"]
    fn every_old_entry_converts_without_losing_text_or_bold() {
        let path = std::env::var("KUTILANG_REAL_DB").expect("KUTILANG_REAL_DB");
        let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
        let migrated: bool = conn
            .query_row(
                "SELECT count(*) > 0 FROM pragma_table_info('visits') WHERE name = 'diagnosis_format'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let sql = if migrated {
            "SELECT id, diagnosis FROM visits WHERE diagnosis_format = 'html'"
        } else {
            "SELECT id, diagnosis FROM visits"
        };
        let mut stmt = conn.prepare(sql).unwrap();
        let rows: Vec<(String, String)> = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert!(!rows.is_empty());

        let mut failures = Vec::new();
        for (id, html) in &rows {
            let markdown = html_to_markdown(html);
            let rendered = markdown::to_html_with_options(
                &with_line_breaks(&markdown),
                &markdown::Options::gfm(),
            )
            .unwrap();
            let (text, bold) = text_and_bold(html);
            let (new_text, new_bold) = text_and_bold(&rendered);
            if text != new_text || bold != new_bold {
                failures.push(id.clone());
            }
        }
        println!("checked {} entries, {} differ", rows.len(), failures.len());
        assert!(
            failures.is_empty(),
            "entries that differ: {:?}",
            &failures[..failures.len().min(20)]
        );
    }
}
