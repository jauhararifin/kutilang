//! Opening a database: creating a fresh one, or upgrading one written by the
//! original Go application.
//!
//! The Go app produced two layouts over its lifetime:
//!
//! * the released one (`kutilang_amd64.exe`): dates stored as `YYYY-MM-DD`
//!   text in `patients.last_visit` and `visits.date`;
//! * an earlier one: Unix timestamps in `patients.last_visit` and in a
//!   `visits.timestamp` column.
//!
//! Both are converted into the v2 schema in a single transaction, after a
//! complete copy of the original file has been written next to it.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, bail};
use chrono::Local;
use rusqlite::{Connection, OptionalExtension as _, params};

use super::schema::{CREATE_V2, SCHEMA_VERSION, UPGRADES};

/// The version the base tables of [`CREATE_V2`] have.
const BASE_VERSION: i64 = 2;
use crate::keywords;

/// What was done to upgrade a database from the original application.
#[derive(Clone, Debug)]
pub struct MigrationReport {
    /// Copy of the database as it was before the upgrade, `None` for an
    /// in-memory database.
    pub backup_path: Option<PathBuf>,
    pub patients: usize,
    pub visits: usize,
    /// Visits whose patient no longer exists. They cannot be shown and are
    /// left out of the new tables; they remain in the backup.
    pub orphan_visits: usize,
    /// Patients whose last visit date was older than their most recent visit
    /// (a visit's date was changed after it was written) and got corrected.
    pub corrected_last_visits: usize,
}

#[derive(Debug, PartialEq, Eq)]
enum VisitDateColumn {
    /// `visits.date`, `YYYY-MM-DD` text.
    Date,
    /// `visits.timestamp`, Unix seconds.
    Timestamp,
}

#[derive(Debug, PartialEq, Eq)]
enum Layout {
    Empty,
    Legacy(VisitDateColumn),
    /// Written by this application, at the given schema version.
    Versioned(i64),
}

/// Brings the database to the current schema. Returns a report when a
/// database of the original application was converted.
pub fn upgrade(conn: &Connection, path: Option<&Path>) -> Result<Option<MigrationReport>> {
    let report = match detect(conn)? {
        Layout::Versioned(_) => None,
        Layout::Empty => {
            create_fresh(conn)?;
            None
        }
        Layout::Legacy(date_column) => Some(migrate_legacy(conn, path, date_column)?),
    };
    apply_upgrades(conn)?;
    Ok(report)
}

/// Runs the schema steps above the database's version, each in its own
/// transaction together with the version it produces.
fn apply_upgrades(conn: &Connection) -> Result<()> {
    let current = user_version(conn)?;
    for &(version, sql) in UPGRADES.iter().filter(|(version, _)| *version > current) {
        let tx = conn.unchecked_transaction()?;
        tx.execute_batch(sql).with_context(|| {
            crate::i18n::pick(
                format!("database tidak dapat diperbarui ke skema {version}"),
                format!("cannot upgrade the database to schema {version}"),
            )
        })?;
        tx.pragma_update(None, "user_version", version)?;
        tx.commit()?;
    }
    Ok(())
}

fn user_version(conn: &Connection) -> Result<i64> {
    Ok(conn.query_row("PRAGMA user_version", [], |row| row.get(0))?)
}

fn table_names(conn: &Connection) -> Result<HashSet<String>> {
    let mut stmt = conn.prepare(
        "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%'",
    )?;
    let names = stmt
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<_>>()?;
    Ok(names)
}

fn column_names(conn: &Connection, table: &str) -> Result<HashSet<String>> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info(\"{table}\")"))?;
    let names = stmt
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<rusqlite::Result<_>>()?;
    Ok(names)
}

fn has_columns(columns: &HashSet<String>, required: &[&str]) -> bool {
    required.iter().all(|c| columns.contains(*c))
}

fn detect(conn: &Connection) -> Result<Layout> {
    let version = user_version(conn)?;
    if version > SCHEMA_VERSION {
        bail!(
            "This database was written by a newer version of Kutilang (schema {version}). \
             Please update the application."
        );
    }
    if version >= BASE_VERSION {
        return Ok(Layout::Versioned(version));
    }
    if version != 0 {
        bail!(crate::i18n::pick(
            format!(
                "Versi skema database {version} tidak dikenal; mungkin dibuat oleh versi Kutilang yang lebih baru."
            ),
            format!("Unknown database schema version {version}.")
        ));
    }

    let tables = table_names(conn)?;
    if tables.is_empty() {
        return Ok(Layout::Empty);
    }

    if !tables.contains("patients") || !tables.contains("visits") {
        bail!(crate::i18n::pick(
            format!("Berkas ini bukan database Kutilang (tabel: {tables:?})."),
            format!("This file does not look like a Kutilang database (tables: {tables:?}).")
        ));
    }
    let patient_columns = column_names(conn, "patients")?;
    let visit_columns = column_names(conn, "visits")?;
    let patients_ok = has_columns(
        &patient_columns,
        &[
            "id",
            "name",
            "head_family_name",
            "birth_date",
            "address",
            "last_visit",
            "is_deleted",
        ],
    );
    let visits_ok = has_columns(
        &visit_columns,
        &["id", "patient_id", "diagnosis", "is_deleted"],
    );
    if !patients_ok || !visits_ok {
        bail!(crate::i18n::t(
            "Susunan database Kutilang tidak dikenal, berkas tidak diubah.",
            "Unrecognised Kutilang database layout, refusing to modify it."
        ));
    }

    if visit_columns.contains("date") {
        Ok(Layout::Legacy(VisitDateColumn::Date))
    } else if visit_columns.contains("timestamp") {
        Ok(Layout::Legacy(VisitDateColumn::Timestamp))
    } else {
        bail!(crate::i18n::t(
            "Susunan database Kutilang tidak dikenal: kunjungan tidak punya kolom tanggal.",
            "Unrecognised Kutilang database layout: visits have no date column."
        ));
    }
}

fn now() -> String {
    Local::now().to_rfc3339()
}

fn create_fresh(conn: &Connection) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    tx.execute_batch(CREATE_V2)?;
    tx.execute(
        "INSERT INTO meta (key, value) VALUES ('created_at', ?1)",
        params![now()],
    )?;
    tx.pragma_update(None, "user_version", BASE_VERSION)?;
    tx.commit()?;
    Ok(())
}

/// `db.sqlite3` -> `db.v1-backup-20261109-133000.sqlite3`, next to the original.
fn backup_path_for(path: &Path) -> PathBuf {
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "db".into());
    let stamp = Local::now().format("%Y%m%d-%H%M%S");
    let dir = path.parent().unwrap_or(Path::new("."));
    let mut candidate = dir.join(format!("{stem}.v1-backup-{stamp}.sqlite3"));
    let mut n = 1;
    while candidate.exists() {
        candidate = dir.join(format!("{stem}.v1-backup-{stamp}-{n}.sqlite3"));
        n += 1;
    }
    candidate
}

/// SQL converting a legacy date value: Unix seconds or `YYYY-MM-DD` text.
fn date_expr(column: &str) -> String {
    format!(
        "CASE WHEN typeof({column}) IN ('integer', 'real') \
              THEN date({column}, 'unixepoch', 'localtime') \
              ELSE COALESCE({column}, '') END"
    )
}

fn migrate_legacy(
    conn: &Connection,
    path: Option<&Path>,
    date_column: VisitDateColumn,
) -> Result<MigrationReport> {
    // 1. Keep an untouched copy of the original database.
    let backup_path = match path {
        Some(path) => {
            let backup = backup_path_for(path);
            conn.execute("VACUUM INTO ?1", params![backup.to_string_lossy()])
                .with_context(|| {
                    crate::i18n::pick(
                        format!("cadangan tidak dapat ditulis ke {}", backup.display()),
                        format!("cannot write backup to {}", backup.display()),
                    )
                })?;
            Some(backup)
        }
        None => None,
    };

    // 2. Rebuild the tables. Foreign keys are only enforced afterwards.
    conn.pragma_update(None, "foreign_keys", false)?;
    let tx = conn.unchecked_transaction()?;

    tx.execute_batch(
        "ALTER TABLE patients RENAME TO legacy_patients;
         ALTER TABLE visits RENAME TO legacy_visits;
         DROP TABLE IF EXISTS patient_indices;",
    )?;
    tx.execute_batch(CREATE_V2)?;

    let patients = tx.execute(
        &format!(
            "INSERT INTO patients
                 (id, name, head_family_name, birth_date, address, last_visit, is_deleted)
             SELECT id, COALESCE(name, ''), COALESCE(head_family_name, ''),
                    COALESCE(birth_date, ''), COALESCE(address, ''), {last_visit},
                    CASE WHEN is_deleted THEN 1 ELSE 0 END
             FROM legacy_patients
             WHERE id IS NOT NULL",
            last_visit = date_expr("last_visit"),
        ),
        [],
    )?;

    let visit_date = match date_column {
        VisitDateColumn::Date => date_expr("date"),
        VisitDateColumn::Timestamp => date_expr("timestamp"),
    };
    let visits = tx.execute(
        &format!(
            "INSERT INTO visits (id, patient_id, date, diagnosis, diagnosis_format, is_deleted)
             SELECT v.id, v.patient_id, {visit_date}, COALESCE(v.diagnosis, ''), 'html',
                    CASE WHEN v.is_deleted THEN 1 ELSE 0 END
             FROM legacy_visits v
             WHERE v.id IS NOT NULL AND v.patient_id IN (SELECT id FROM patients)"
        ),
        [],
    )?;
    let total_visits: i64 = tx.query_row(
        "SELECT count(*) FROM legacy_visits WHERE id IS NOT NULL",
        [],
        |r| r.get(0),
    )?;
    let total_visits = total_visits as usize;

    // The Go app only raised `last_visit` when a visit was added, so changing
    // a visit's date later left it behind.
    let corrected_last_visits = tx.execute(
        "UPDATE patients
         SET last_visit = (SELECT max(v.date) FROM visits v
                           WHERE v.patient_id = patients.id AND v.is_deleted = 0)
         WHERE last_visit < (SELECT max(v.date) FROM visits v
                             WHERE v.patient_id = patients.id AND v.is_deleted = 0)",
        [],
    )?;

    // Rebuild the search index from the names.
    {
        let mut select =
            tx.prepare("SELECT id, name, head_family_name FROM patients WHERE is_deleted = 0")?;
        let mut insert =
            tx.prepare("INSERT INTO patient_keywords (keyword, patient_id) VALUES (?1, ?2)")?;
        let rows = select
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for (id, name, head_family_name) in rows {
            for keyword in keywords::patient_keywords(&name, &head_family_name) {
                insert.execute(params![keyword, id])?;
            }
        }
    }

    tx.execute_batch("DROP TABLE legacy_visits; DROP TABLE legacy_patients;")?;

    let source = match date_column {
        VisitDateColumn::Date => "kutilang-go (date text)",
        VisitDateColumn::Timestamp => "kutilang-go (unix timestamps)",
    };
    let mut meta = tx.prepare("INSERT INTO meta (key, value) VALUES (?1, ?2)")?;
    meta.execute(params!["migrated_from", source])?;
    meta.execute(params!["migrated_at", now()])?;
    if let Some(backup) = &backup_path {
        meta.execute(params!["migration_backup", backup.to_string_lossy()])?;
    }
    drop(meta);

    tx.pragma_update(None, "user_version", BASE_VERSION)?;
    tx.commit()?;

    // 3. Verify and compact.
    conn.pragma_update(None, "foreign_keys", true)?;
    let violation: Option<String> = conn
        .query_row("PRAGMA foreign_key_check", [], |row| row.get(0))
        .optional()?;
    if let Some(table) = violation {
        bail!("foreign key check failed on {table} after the upgrade");
    }
    if path.is_some() {
        conn.execute_batch("VACUUM")?;
    }

    Ok(MigrationReport {
        backup_path,
        patients,
        visits,
        orphan_visits: total_visits - visits,
        corrected_last_visits,
    })
}
