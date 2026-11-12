//! SQLite storage: the schema and its upgrades (including the migration of
//! the original application's database), patients, visits and settings,
//! backup and restore, and the Excel export.

use crate::{
    Cursor, DiagnosisFormat, MAX_QUERY_KEYWORDS, Page, Patient, PatientOrder, PatientProfile,
    Visit, VisitSummary, age_years, diagnosis, parse_stored, patient_keywords, t, to_stored, today,
    tokenize,
};
use anyhow::{Context as _, Result, bail};
use chrono::{Datelike as _, Local, NaiveDate};
use rusqlite::types::Value;
use rusqlite::{Connection, OptionalExtension as _, Row, params, params_from_iter};
use rust_xlsxwriter::{ExcelDateTime, Format, FormatAlign, Workbook, Worksheet};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Duration;
use uuid::Uuid;

// ============================================================================
// Opening a database; patients, visits and settings.
// ============================================================================

/// The settings key of the entry template.
const TEMPLATE_SETTING: &str = "diagnosis_template";

const PATIENT_COLUMNS: &str =
    "p.id, p.name, p.head_family_name, p.birth_date, p.address, p.last_visit";
const VISIT_COLUMNS: &str = "v.id, v.patient_id, v.date, v.diagnosis, v.diagnosis_format";

pub struct Store {
    conn: Connection,
}

fn new_id() -> String {
    Uuid::new_v4().simple().to_string()
}

fn now() -> String {
    Local::now().to_rfc3339()
}

fn patient_from_row(row: &Row) -> rusqlite::Result<Patient> {
    Ok(Patient {
        id: row.get(0)?,
        name: row.get(1)?,
        head_family_name: row.get(2)?,
        birth_date: row.get(3)?,
        address: row.get(4)?,
        last_visit: row.get(5)?,
    })
}

fn visit_from_row(row: &Row) -> rusqlite::Result<Visit> {
    let format: String = row.get(4)?;
    Ok(Visit {
        id: row.get(0)?,
        patient_id: row.get(1)?,
        date: row.get(2)?,
        diagnosis: row.get(3)?,
        format: DiagnosisFormat::parse(&format).unwrap_or(DiagnosisFormat::Html),
    })
}

/// Splits `limit + 1` fetched rows into a page and the cursor of the next one.
fn into_page<T>(mut items: Vec<T>, limit: usize, cursor: impl Fn(&T) -> Cursor) -> Page<T> {
    let next = if items.len() > limit {
        items.truncate(limit);
        items.last().map(cursor)
    } else {
        None
    };
    Page { items, next }
}

fn check_diagnosis(diagnosis: &str) -> Result<()> {
    if diagnosis.trim().is_empty() {
        bail!(crate::t("Diagnosis belum diisi", "Missing diagnosis"));
    }
    Ok(())
}

impl Store {
    /// Opens (or creates) the database at `path`, upgrading a database of the
    /// original application when needed.
    pub fn open(path: &Path) -> Result<(Self, Option<MigrationReport>)> {
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        Self::init(conn, Some(path))
    }

    /// An empty database in memory, for tests.
    pub fn open_in_memory() -> Result<Self> {
        Ok(Self::init(Connection::open_in_memory()?, None)?.0)
    }

    /// Upgrades and wraps an existing connection, for tests.
    pub fn from_connection(conn: Connection) -> Result<(Self, Option<MigrationReport>)> {
        Self::init(conn, None)
    }

    fn init(conn: Connection, path: Option<&Path>) -> Result<(Self, Option<MigrationReport>)> {
        conn.busy_timeout(Duration::from_secs(5))?;
        let report = upgrade(&conn, path)?;
        conn.pragma_update(None, "foreign_keys", true)?;
        Ok((Store { conn }, report))
    }

    pub fn connection(&self) -> &Connection {
        &self.conn
    }

    // Patients ----------------------------------------------------------

    pub fn get_patient(&self, id: &str) -> Result<Option<Patient>> {
        Ok(self
            .conn
            .query_row(
                &format!(
                    "SELECT {PATIENT_COLUMNS} FROM patients p WHERE p.id = ?1 AND p.is_deleted = 0"
                ),
                [id],
                patient_from_row,
            )
            .optional()?)
    }

    pub fn create_patient(&self, profile: &PatientProfile, today: NaiveDate) -> Result<Patient> {
        let patient = Patient {
            id: new_id(),
            name: profile.name.clone(),
            head_family_name: profile.head_family_name.clone(),
            birth_date: to_stored(profile.birth_date),
            address: profile.address.clone(),
            last_visit: to_stored(today),
        };
        let now = now();
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO patients
                 (id, name, head_family_name, birth_date, address, last_visit, is_deleted, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0, ?7, ?7)",
            params![
                patient.id,
                patient.name,
                patient.head_family_name,
                patient.birth_date,
                patient.address,
                patient.last_visit,
                now
            ],
        )?;
        insert_keywords(&tx, &patient.id, &patient.name, &patient.head_family_name)?;
        tx.commit()?;
        Ok(patient)
    }

    pub fn update_patient(&self, id: &str, profile: &PatientProfile) -> Result<Patient> {
        let tx = self.conn.unchecked_transaction()?;
        let changed = tx.execute(
            "UPDATE patients
             SET name = ?2, head_family_name = ?3, birth_date = ?4, address = ?5, updated_at = ?6
             WHERE id = ?1 AND is_deleted = 0",
            params![
                id,
                profile.name,
                profile.head_family_name,
                to_stored(profile.birth_date),
                profile.address,
                now()
            ],
        )?;
        if changed == 0 {
            bail!(crate::t("Pasien tidak ditemukan", "Patient not found"));
        }
        tx.execute("DELETE FROM patient_keywords WHERE patient_id = ?1", [id])?;
        insert_keywords(&tx, id, &profile.name, &profile.head_family_name)?;
        tx.commit()?;
        self.get_patient(id)?
            .ok_or_else(|| anyhow::anyhow!(crate::t("Pasien tidak ditemukan", "Patient not found")))
    }

    /// Hides the patient. Nothing is erased: the row and the visits stay in
    /// the database, as in the original application.
    pub fn delete_patient(&self, id: &str) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        let changed = tx.execute(
            "UPDATE patients SET is_deleted = 1, updated_at = ?2 WHERE id = ?1 AND is_deleted = 0",
            params![id, now()],
        )?;
        if changed == 0 {
            bail!(crate::t("Pasien tidak ditemukan", "Patient not found"));
        }
        tx.execute("DELETE FROM patient_keywords WHERE patient_id = ?1", [id])?;
        tx.commit()?;
        Ok(())
    }

    /// All patients, or those matching `query`: every word of the query must
    /// start a word of the patient's name or head of family name, so the
    /// list narrows while the user types (`nu` finds `Nur` and `Nuri`).
    pub fn list_patients(&self, query: &str, order: PatientOrder) -> Result<Vec<Patient>> {
        let mut sql = format!("SELECT {PATIENT_COLUMNS} FROM patients p WHERE p.is_deleted = 0");
        let mut args = Vec::new();
        push_name_filter(query, "p.id", &mut sql, &mut args)?;
        sql.push_str(match order {
            PatientOrder::RecentVisit => " ORDER BY p.last_visit DESC, p.id DESC",
            PatientOrder::Name => " ORDER BY p.name COLLATE NOCASE, p.id",
        });
        let mut stmt = self.conn.prepare(&sql)?;
        let patients = stmt
            .query_map(params_from_iter(args), patient_from_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(patients)
    }

    pub fn count_patients(&self) -> Result<usize> {
        let count: i64 = self.conn.query_row(
            "SELECT count(*) FROM patients WHERE is_deleted = 0",
            [],
            |r| r.get(0),
        )?;
        Ok(count as usize)
    }

    /// Visits of the patients that are not deleted.
    pub fn count_visits(&self) -> Result<usize> {
        let count: i64 = self.conn.query_row(
            "SELECT count(*) FROM visits v JOIN patients p ON p.id = v.patient_id
             WHERE v.is_deleted = 0 AND p.is_deleted = 0",
            [],
            |r| r.get(0),
        )?;
        Ok(count as usize)
    }

    /// Visits of a patient that are not deleted.
    pub fn count_patient_visits(&self, patient_id: &str) -> Result<usize> {
        let count: i64 = self.conn.query_row(
            "SELECT count(*) FROM visits WHERE patient_id = ?1 AND is_deleted = 0",
            [patient_id],
            |r| r.get(0),
        )?;
        Ok(count as usize)
    }

    /// Every visit, newest first, optionally limited to patients matching
    /// `query` (as in [`Store::list_patients`]) and to dates within
    /// `from..=to`.
    pub fn list_visit_summaries(
        &self,
        query: &str,
        from: Option<NaiveDate>,
        to: Option<NaiveDate>,
    ) -> Result<Vec<VisitSummary>> {
        // The summary only needs the beginning of the text.
        let mut sql = String::from(
            "SELECT v.id, v.patient_id, p.name, p.head_family_name, v.date,
                    substr(v.diagnosis, 1, 1200), v.diagnosis_format
             FROM visits v JOIN patients p ON p.id = v.patient_id
             WHERE v.is_deleted = 0 AND p.is_deleted = 0",
        );
        let mut args = Vec::new();
        if let Some(from) = from {
            sql.push_str(" AND v.date >= ?");
            args.push(Value::Text(to_stored(from)));
        }
        if let Some(to) = to {
            sql.push_str(" AND v.date <= ?");
            args.push(Value::Text(to_stored(to)));
        }
        push_name_filter(query, "p.id", &mut sql, &mut args)?;
        sql.push_str(" ORDER BY v.date DESC, v.id DESC");

        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params_from_iter(args), |row| {
            let text: String = row.get(5)?;
            let format: String = row.get(6)?;
            let format = DiagnosisFormat::parse(&format).unwrap_or(DiagnosisFormat::Html);
            Ok(VisitSummary {
                id: row.get(0)?,
                patient_id: row.get(1)?,
                patient_name: row.get(2)?,
                head_family_name: row.get(3)?,
                date: row.get(4)?,
                summary: diagnosis::summary(&text, format, 160),
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    // Settings ----------------------------------------------------------

    pub fn setting(&self, key: &str) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| {
                r.get(0)
            })
            .optional()?)
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT (key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    pub fn remove_setting(&self, key: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM settings WHERE key = ?1", [key])?;
        Ok(())
    }

    /// The template new entries start from: the one set up in the settings,
    /// or the built-in one.
    pub fn diagnosis_template(&self) -> String {
        self.setting(TEMPLATE_SETTING)
            .ok()
            .flatten()
            .unwrap_or_else(|| diagnosis::TEMPLATE.to_string())
    }

    /// Stores a custom template; `None` goes back to the built-in one.
    pub fn set_diagnosis_template(&self, template: Option<&str>) -> Result<()> {
        match template {
            Some(template) if template != diagnosis::TEMPLATE => {
                self.set_setting(TEMPLATE_SETTING, template)
            }
            _ => self.remove_setting(TEMPLATE_SETTING),
        }
    }

    /// Whether a custom template is set up.
    pub fn has_custom_template(&self) -> bool {
        matches!(self.setting(TEMPLATE_SETTING), Ok(Some(_)))
    }

    // Visits ------------------------------------------------------------

    pub fn get_visit(&self, id: &str) -> Result<Option<Visit>> {
        Ok(self
            .conn
            .query_row(
                &format!(
                    "SELECT {VISIT_COLUMNS} FROM visits v WHERE v.id = ?1 AND v.is_deleted = 0"
                ),
                [id],
                visit_from_row,
            )
            .optional()?)
    }

    /// Visits of a patient, newest first.
    pub fn list_visits(
        &self,
        patient_id: &str,
        after: Option<&Cursor>,
        limit: usize,
    ) -> Result<Page<Visit>> {
        let mut sql = format!(
            "SELECT {VISIT_COLUMNS} FROM visits v WHERE v.patient_id = ? AND v.is_deleted = 0"
        );
        let mut args = vec![Value::Text(patient_id.to_string())];
        if let Some(cursor) = after {
            sql.push_str(" AND (v.date < ? OR (v.date = ? AND v.id < ?))");
            args.push(Value::Text(cursor.date.clone()));
            args.push(Value::Text(cursor.date.clone()));
            args.push(Value::Text(cursor.id.clone()));
        }
        sql.push_str(" ORDER BY v.date DESC, v.id DESC LIMIT ?");
        args.push(Value::Integer(limit as i64 + 1));

        let mut stmt = self.conn.prepare(&sql)?;
        let items = stmt
            .query_map(params_from_iter(args), visit_from_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(into_page(items, limit, |v| Cursor {
            date: v.date.clone(),
            id: v.id.clone(),
        }))
    }

    /// Records a visit, written as plain text.
    pub fn add_visit(&self, patient_id: &str, date: NaiveDate, diagnosis: &str) -> Result<Visit> {
        check_diagnosis(diagnosis)?;
        let tx = self.conn.unchecked_transaction()?;
        let exists = tx
            .query_row(
                "SELECT 1 FROM patients WHERE id = ?1 AND is_deleted = 0",
                [patient_id],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        if !exists {
            bail!(crate::t("Pasien tidak ditemukan", "Patient does not exist"));
        }

        let visit = Visit {
            id: new_id(),
            patient_id: patient_id.to_string(),
            date: to_stored(date),
            diagnosis: diagnosis.to_string(),
            format: DiagnosisFormat::Text,
        };
        tx.execute(
            "INSERT INTO visits
                 (id, patient_id, date, diagnosis, diagnosis_format, is_deleted, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, 0, ?6, ?6)",
            params![
                visit.id,
                visit.patient_id,
                visit.date,
                visit.diagnosis,
                visit.format.as_str(),
                now()
            ],
        )?;
        raise_last_visit(&tx, patient_id, &visit.date)?;
        tx.commit()?;
        Ok(visit)
    }

    /// Changes a visit's date and, when `diagnosis` is given, its text (which
    /// is then stored as plain text). Leaving the text alone keeps a visit of
    /// the original application in its original HTML.
    pub fn update_visit(
        &self,
        id: &str,
        date: NaiveDate,
        diagnosis: Option<&str>,
    ) -> Result<Visit> {
        if let Some(diagnosis) = diagnosis {
            check_diagnosis(diagnosis)?;
        }
        let Some(visit) = self.get_visit(id)? else {
            bail!(crate::t("Kunjungan tidak ditemukan", "Visit not found"));
        };
        let date = to_stored(date);
        let tx = self.conn.unchecked_transaction()?;
        match diagnosis {
            Some(diagnosis) => tx.execute(
                "UPDATE visits SET date = ?2, diagnosis = ?3, diagnosis_format = 'text', updated_at = ?4
                 WHERE id = ?1",
                params![id, date, diagnosis, now()],
            )?,
            None => tx.execute(
                "UPDATE visits SET date = ?2, updated_at = ?3 WHERE id = ?1",
                params![id, date, now()],
            )?,
        };
        refresh_last_visit(&tx, &visit.patient_id)?;
        tx.commit()?;
        self.get_visit(id)?.ok_or_else(|| {
            anyhow::anyhow!(crate::t("Kunjungan tidak ditemukan", "Visit not found"))
        })
    }

    /// Hides a visit; the row stays in the database.
    pub fn delete_visit(&self, id: &str) -> Result<()> {
        let Some(visit) = self.get_visit(id)? else {
            bail!(crate::t("Kunjungan tidak ditemukan", "Visit not found"));
        };
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "UPDATE visits SET is_deleted = 1, updated_at = ?2 WHERE id = ?1 AND is_deleted = 0",
            params![id, now()],
        )?;
        refresh_last_visit(&tx, &visit.patient_id)?;
        tx.commit()?;
        Ok(())
    }

    /// Whether `path` is the file of this database, which must never be
    /// overwritten by a backup or an export while it is open.
    fn is_own_file(&self, path: &Path) -> bool {
        let Some(own) = self.conn.path().filter(|p| !p.is_empty()) else {
            return false;
        };
        match (Path::new(own).canonicalize(), path.canonicalize()) {
            (Ok(own), Ok(path)) => own == path,
            _ => false,
        }
    }

    fn refuse_own_file(&self, path: &Path) -> Result<()> {
        if self.is_own_file(path) {
            bail!(crate::pick(
                format!(
                    "{} adalah database yang sedang dibuka; pilih berkas lain.",
                    path.display()
                ),
                format!(
                    "{} is the database that is open; choose another file.",
                    path.display()
                ),
            ));
        }
        Ok(())
    }
}

/// Restricts a query to patients whose name words start with every word of
/// `query`. `id_column` is the patient id column of the query.
fn push_name_filter(
    query: &str,
    id_column: &str,
    sql: &mut String,
    args: &mut Vec<Value>,
) -> Result<()> {
    let words = tokenize(query);
    if words.len() > MAX_QUERY_KEYWORDS {
        bail!(crate::t(
            "Kata pencarian terlalu panjang",
            "Search query is too long"
        ));
    }
    for word in words {
        // Every keyword starting with `word` sorts between `word` and `word`
        // followed by the highest code point.
        sql.push_str(&format!(
            " AND {id_column} IN (SELECT patient_id FROM patient_keywords WHERE keyword >= ? AND keyword < ?)"
        ));
        let upper = format!("{word}\u{10FFFF}");
        args.push(Value::Text(word));
        args.push(Value::Text(upper));
    }
    Ok(())
}

fn insert_keywords(
    conn: &Connection,
    patient_id: &str,
    name: &str,
    head_family_name: &str,
) -> Result<()> {
    let mut stmt =
        conn.prepare_cached("INSERT INTO patient_keywords (keyword, patient_id) VALUES (?1, ?2)")?;
    for keyword in patient_keywords(name, head_family_name) {
        stmt.execute(params![keyword, patient_id])?;
    }
    Ok(())
}

/// Sets the patient's last visit after a visit's date changed or a visit was
/// deleted: the most recent visit, or the registration date when it is later.
/// A patient of the original application has no registration date; with no
/// visit left, their last visit is kept.
fn refresh_last_visit(conn: &Connection, patient_id: &str) -> Result<()> {
    conn.execute(
        "UPDATE patients SET last_visit = COALESCE(
             NULLIF(max(
                 COALESCE((SELECT max(v.date) FROM visits v
                           WHERE v.patient_id = patients.id AND v.is_deleted = 0), ''),
                 COALESCE(substr(created_at, 1, 10), '')
             ), ''),
             last_visit)
         WHERE id = ?1",
        [patient_id],
    )?;
    Ok(())
}

/// Moves the patient's last visit forward to `date` when it is later.
fn raise_last_visit(conn: &Connection, patient_id: &str, date: &str) -> Result<()> {
    conn.execute(
        "UPDATE patients SET last_visit = ?2 WHERE id = ?1 AND last_visit < ?2",
        params![patient_id, date],
    )?;
    Ok(())
}

// ============================================================================
// The v2 database schema.
//
// Versioning uses `PRAGMA user_version`. The Go app never set it, so a
// database it created reports `0` and is recognised by its tables instead.
// ============================================================================

/// Schema version written by this app.
pub const SCHEMA_VERSION: i64 = 4;

/// A step from one schema version to the next.
type Upgrade = fn(&Connection) -> Result<()>;

/// The schema steps: the version each step produces, and the step. A
/// database is brought up to date by running every step above its current
/// version, in order.
const UPGRADES: &[(i64, Upgrade)] = &[(3, add_settings), (4, plain_text_visits)];

/// Version 3: the settings (the template of new visits).
fn add_settings(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE settings (
             key   TEXT PRIMARY KEY NOT NULL,
             value TEXT NOT NULL
         ) WITHOUT ROWID;",
    )?;
    Ok(())
}

/// Version 4: visits are written as plain text instead of Markdown. Visits
/// and the template written in Markdown by version 2 and 3 are converted.
fn plain_text_visits(conn: &Connection) -> Result<()> {
    let markdown_visits = conn
        .prepare("SELECT id, diagnosis FROM visits WHERE diagnosis_format = 'markdown'")?
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut update = conn.prepare("UPDATE visits SET diagnosis = ?2 WHERE id = ?1")?;
    for (id, markdown) in markdown_visits {
        update.execute(params![id, diagnosis::markdown_to_text(&markdown)])?;
    }
    let template: Option<String> = conn
        .query_row(
            "SELECT value FROM settings WHERE key = ?1",
            [TEMPLATE_SETTING],
            |row| row.get(0),
        )
        .optional()?;
    if let Some(template) = template {
        conn.execute(
            "UPDATE settings SET value = ?2 WHERE key = ?1",
            params![TEMPLATE_SETTING, diagnosis::markdown_to_text(&template)],
        )?;
    }
    // The format check cannot be changed in place.
    conn.execute_batch(
        "CREATE TABLE visits_v4 (
             id               TEXT PRIMARY KEY NOT NULL,
             patient_id       TEXT NOT NULL REFERENCES patients(id),
             date             TEXT NOT NULL,
             diagnosis        TEXT NOT NULL,
             diagnosis_format TEXT NOT NULL CHECK (diagnosis_format IN ('html', 'text')),
             is_deleted       INTEGER NOT NULL DEFAULT 0 CHECK (is_deleted IN (0, 1)),
             created_at       TEXT,
             updated_at       TEXT
         );
         INSERT INTO visits_v4
             (id, patient_id, date, diagnosis, diagnosis_format, is_deleted, created_at, updated_at)
         SELECT id, patient_id, date, diagnosis,
                CASE diagnosis_format WHEN 'html' THEN 'html' ELSE 'text' END,
                is_deleted, created_at, updated_at
         FROM visits;
         DROP TABLE visits;
         ALTER TABLE visits_v4 RENAME TO visits;
         CREATE INDEX visits_patient_idx ON visits(patient_id, is_deleted, date DESC, id DESC);",
    )?;
    Ok(())
}

/// The tables of schema version 2, the first version of this app (visits
/// were then written in Markdown). Newer versions are reached through
/// [`UPGRADES`].
pub const CREATE_V2: &str = r#"
CREATE TABLE meta (
    key   TEXT PRIMARY KEY NOT NULL,
    value TEXT NOT NULL
) WITHOUT ROWID;

CREATE TABLE patients (
    id               TEXT PRIMARY KEY NOT NULL,
    name             TEXT NOT NULL,
    head_family_name TEXT NOT NULL DEFAULT '',
    birth_date       TEXT NOT NULL,
    address          TEXT NOT NULL DEFAULT '',
    last_visit       TEXT NOT NULL,
    is_deleted       INTEGER NOT NULL DEFAULT 0 CHECK (is_deleted IN (0, 1)),
    created_at       TEXT,
    updated_at       TEXT
);
CREATE INDEX patients_recent_idx ON patients(is_deleted, last_visit DESC, id DESC);

CREATE TABLE patient_keywords (
    keyword    TEXT NOT NULL,
    patient_id TEXT NOT NULL REFERENCES patients(id) ON DELETE CASCADE,
    PRIMARY KEY (keyword, patient_id)
) WITHOUT ROWID;
CREATE INDEX patient_keywords_patient_idx ON patient_keywords(patient_id);

CREATE TABLE visits (
    id               TEXT PRIMARY KEY NOT NULL,
    patient_id       TEXT NOT NULL REFERENCES patients(id),
    date             TEXT NOT NULL,
    diagnosis        TEXT NOT NULL,
    diagnosis_format TEXT NOT NULL CHECK (diagnosis_format IN ('html', 'markdown')),
    is_deleted       INTEGER NOT NULL DEFAULT 0 CHECK (is_deleted IN (0, 1)),
    created_at       TEXT,
    updated_at       TEXT
);
CREATE INDEX visits_patient_idx ON visits(patient_id, is_deleted, date DESC, id DESC);
"#;

// ============================================================================
// Opening a database: creating a fresh one, or upgrading one written by the
// original Go application.
//
// The Go app produced two layouts over its lifetime:
//
// * the released one (`kutilang_amd64.exe`): dates stored as `YYYY-MM-DD`
// text in `patients.last_visit` and `visits.date`;
// * an earlier one: Unix timestamps in `patients.last_visit` and in a
// `visits.timestamp` column.
//
// Both are converted into the v2 schema in a single transaction, after a
// complete copy of the original file has been written next to it.
// ============================================================================

/// The version the base tables of [`CREATE_V2`] have.
const BASE_VERSION: i64 = 2;

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
    for &(version, upgrade) in UPGRADES.iter().filter(|(version, _)| *version > current) {
        let tx = conn.unchecked_transaction()?;
        upgrade(&tx).with_context(|| {
            crate::pick(
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
        bail!(crate::pick(
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
        bail!(crate::pick(
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
        bail!(crate::t(
            "Susunan database Kutilang tidak dikenal, berkas tidak diubah.",
            "Unrecognised Kutilang database layout, refusing to modify it."
        ));
    }

    if visit_columns.contains("date") {
        Ok(Layout::Legacy(VisitDateColumn::Date))
    } else if visit_columns.contains("timestamp") {
        Ok(Layout::Legacy(VisitDateColumn::Timestamp))
    } else {
        bail!(crate::t(
            "Susunan database Kutilang tidak dikenal: kunjungan tidak punya kolom tanggal.",
            "Unrecognised Kutilang database layout: visits have no date column."
        ));
    }
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
                    crate::pick(
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
            for keyword in patient_keywords(&name, &head_family_name) {
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
        orphan_visits: total_visits.saturating_sub(visits),
        corrected_last_visits,
    })
}

// ============================================================================
// Backing up, restoring and exporting the whole database.
// ============================================================================

/// A backup opened for restoring: a working copy, upgraded to the current
/// schema, so the file the user chose is never modified.
pub struct Backup {
    _dir: tempfile::TempDir,
    copy: PathBuf,
    pub source: PathBuf,
    pub patients: usize,
    pub visits: usize,
}

/// A patient row of the export.
pub struct ExportPatient {
    pub id: String,
    pub name: String,
    pub head_family_name: String,
    pub birth_date: String,
    pub address: String,
    pub last_visit: String,
    pub visits: usize,
}

/// A visit row of the export.
pub struct ExportVisit {
    pub id: String,
    pub date: String,
    pub patient_id: String,
    pub patient_name: String,
    pub head_family_name: String,
    pub diagnosis: String,
    pub format: DiagnosisFormat,
}

/// `<dir>/<stem>.<label>-<timestamp>.sqlite3`, a name not taken yet.
pub fn timestamped_path(db_path: &Path, label: &str) -> PathBuf {
    let stem = db_path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "db".into());
    let stamp = Local::now().format("%Y%m%d-%H%M%S");
    let dir = db_path.parent().unwrap_or(Path::new("."));
    let mut candidate = dir.join(format!("{stem}.{label}-{stamp}.sqlite3"));
    let mut n = 1;
    while candidate.exists() {
        candidate = dir.join(format!("{stem}.{label}-{stamp}-{n}.sqlite3"));
        n += 1;
    }
    candidate
}

impl Store {
    /// Writes a complete, consistent copy of the database to `path`,
    /// replacing the file if it exists.
    pub fn backup_to(&self, path: &Path) -> Result<()> {
        self.refuse_own_file(path)?;
        let dir = path
            .parent()
            .filter(|d| !d.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        // `VACUUM INTO` needs a new file: write next to the target, then
        // move it into place so a failure never leaves half a file.
        let temp = tempfile::Builder::new()
            .prefix(".kutilang-backup-")
            .suffix(".sqlite3")
            .tempfile_in(dir)
            .with_context(|| {
                crate::pick(
                    format!("tidak dapat menulis ke {}", dir.display()),
                    format!("cannot write to {}", dir.display()),
                )
            })?
            .into_temp_path();
        std::fs::remove_file(&temp)?;
        self.conn
            .execute("VACUUM INTO ?1", params![temp.to_string_lossy()])
            .with_context(|| {
                crate::pick(
                    format!("tidak dapat mencadangkan ke {}", path.display()),
                    format!("cannot back up to {}", path.display()),
                )
            })?;
        temp.persist(path).with_context(|| {
            crate::pick(
                format!("tidak dapat menulis {}", path.display()),
                format!("cannot write {}", path.display()),
            )
        })?;
        Ok(())
    }

    /// Opens a backup for [`Store::restore`]: a backup made by this
    /// application, or a database of the original one.
    pub fn open_backup(path: &Path) -> Result<Backup> {
        let dir = tempfile::tempdir()?;
        let copy = dir.path().join("backup.sqlite3");
        std::fs::copy(path, &copy).with_context(|| {
            crate::pick(
                format!("tidak dapat membaca {}", path.display()),
                format!("cannot read {}", path.display()),
            )
        })?;
        let (store, _) = Store::open(&copy).with_context(|| {
            crate::pick(
                format!("{} bukan cadangan Kutilang", path.display()),
                format!("{} is not a Kutilang backup", path.display()),
            )
        })?;
        let patients = store.count_patients()?;
        let visits = store.count_visits()?;
        Ok(Backup {
            _dir: dir,
            copy,
            source: path.to_path_buf(),
            patients,
            visits,
        })
    }

    /// Replaces every record (patients, visits, settings) with the backup's,
    /// in one transaction.
    pub fn restore(&self, backup: &Backup) -> Result<()> {
        self.conn.execute(
            "ATTACH DATABASE ?1 AS backup",
            params![backup.copy.to_string_lossy()],
        )?;
        let result = (|| -> Result<()> {
            let tx = self.conn.unchecked_transaction()?;
            tx.execute_batch(
                "DELETE FROM patient_keywords;
                 DELETE FROM visits;
                 DELETE FROM patients;
                 DELETE FROM settings;
                 INSERT INTO patients
                     (id, name, head_family_name, birth_date, address, last_visit, is_deleted, created_at, updated_at)
                 SELECT id, name, head_family_name, birth_date, address, last_visit, is_deleted, created_at, updated_at
                 FROM backup.patients;
                 INSERT INTO visits
                     (id, patient_id, date, diagnosis, diagnosis_format, is_deleted, created_at, updated_at)
                 SELECT id, patient_id, date, diagnosis, diagnosis_format, is_deleted, created_at, updated_at
                 FROM backup.visits;
                 INSERT INTO patient_keywords (keyword, patient_id)
                 SELECT keyword, patient_id FROM backup.patient_keywords;
                 INSERT INTO settings (key, value) SELECT key, value FROM backup.settings;",
            )?;
            let mut meta = tx.prepare(
                "INSERT INTO meta (key, value) VALUES (?1, ?2)
                 ON CONFLICT (key) DO UPDATE SET value = excluded.value",
            )?;
            meta.execute(params!["restored_from", backup.source.to_string_lossy()])?;
            meta.execute(params!["restored_at", Local::now().to_rfc3339()])?;
            drop(meta);
            tx.commit()?;
            Ok(())
        })();
        self.conn.execute("DETACH DATABASE backup", [])?;
        result?;

        let violation: Option<String> = rusqlite::OptionalExtension::optional(
            self.conn
                .query_row("PRAGMA foreign_key_check", [], |row| row.get(0)),
        )?;
        if let Some(table) = violation {
            bail!(crate::pick(
                format!("data yang dipulihkan tidak konsisten ({table})"),
                format!("the restored data is inconsistent ({table})")
            ));
        }
        Ok(())
    }

    /// Every patient that is not deleted, by name, with their visit counts.
    pub fn export_patients(&self) -> Result<Vec<ExportPatient>> {
        let mut stmt = self.conn.prepare(
            "SELECT p.id, p.name, p.head_family_name, p.birth_date, p.address, p.last_visit,
                    (SELECT count(*) FROM visits v WHERE v.patient_id = p.id AND v.is_deleted = 0)
             FROM patients p WHERE p.is_deleted = 0
             ORDER BY p.name COLLATE NOCASE, p.id",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(ExportPatient {
                id: row.get(0)?,
                name: row.get(1)?,
                head_family_name: row.get(2)?,
                birth_date: row.get(3)?,
                address: row.get(4)?,
                last_visit: row.get(5)?,
                visits: row.get::<_, i64>(6)? as usize,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Every visit of the patients that are not deleted, newest first.
    pub fn export_visits(&self) -> Result<Vec<ExportVisit>> {
        let mut stmt = self.conn.prepare(
            "SELECT v.id, v.date, v.patient_id, p.name, p.head_family_name, v.diagnosis, v.diagnosis_format
             FROM visits v JOIN patients p ON p.id = v.patient_id
             WHERE v.is_deleted = 0 AND p.is_deleted = 0
             ORDER BY v.date DESC, v.id DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            let format: String = row.get(6)?;
            Ok(ExportVisit {
                id: row.get(0)?,
                date: row.get(1)?,
                patient_id: row.get(2)?,
                patient_name: row.get(3)?,
                head_family_name: row.get(4)?,
                diagnosis: row.get(5)?,
                format: DiagnosisFormat::parse(&format).unwrap_or(DiagnosisFormat::Html),
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }
}

// ============================================================================
// Exporting every patient and visit to an Excel workbook.
// ============================================================================

/// The most text an Excel cell can hold.
const CELL_LIMIT: usize = 32_767;

/// What was written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExportReport {
    pub patients: usize,
    pub visits: usize,
}

struct Formats {
    header: Format,
    date: Format,
    wrap: Format,
}

/// A stored `YYYY-MM-DD` date as an Excel date, or as text when Excel
/// cannot represent it (before 1900, or not a date).
fn write_date(
    sheet: &mut Worksheet,
    row: u32,
    col: u16,
    value: &str,
    formats: &Formats,
) -> Result<()> {
    let date = parse_stored(value).and_then(|d| {
        ExcelDateTime::from_ymd(d.year().try_into().ok()?, d.month() as u8, d.day() as u8).ok()
    });
    match date {
        Some(date) => sheet.write_datetime_with_format(row, col, &date, &formats.date)?,
        None => sheet.write_string(row, col, limit(value))?,
    };
    Ok(())
}

/// `text`, cut to what an Excel cell can hold (longer text is an error).
fn limit(text: &str) -> &str {
    match text.char_indices().nth(CELL_LIMIT) {
        Some((end, _)) => &text[..end],
        None => text,
    }
}

fn header(sheet: &mut Worksheet, columns: &[(&str, f64)], formats: &Formats) -> Result<()> {
    for (col, (title, width)) in columns.iter().enumerate() {
        sheet.write_string_with_format(0, col as u16, *title, &formats.header)?;
        sheet.set_column_width(col as u16, *width)?;
    }
    sheet.set_freeze_panes(1, 0)?;
    Ok(())
}

/// Writes a workbook with a sheet of patients and a sheet of visits.
pub fn export_xlsx(store: &Store, path: &Path) -> Result<ExportReport> {
    store.refuse_own_file(path)?;
    let today = today();
    let formats = Formats {
        header: Format::new().set_bold().set_background_color(0xE7E6E6),
        date: Format::new()
            .set_num_format("dd/mm/yyyy")
            .set_align(FormatAlign::Top),
        wrap: Format::new().set_text_wrap().set_align(FormatAlign::Top),
    };
    let mut workbook = Workbook::new();

    let patients = store.export_patients()?;
    let sheet = workbook.add_worksheet();
    sheet.set_name(t("Pasien", "Patients"))?;
    let columns = [
        (t("Nama", "Name"), 28.0),
        (t("Nama KK", "Head of Family"), 24.0),
        (t("Tanggal Lahir", "Date of Birth"), 14.0),
        (t("Umur", "Age"), 7.0),
        (t("Alamat", "Address"), 40.0),
        (t("Kunjungan Terakhir", "Last Visit"), 18.0),
        (t("Jumlah Kunjungan", "Visits"), 17.0),
        (t("ID Pasien", "Patient ID"), 34.0),
    ];
    header(sheet, &columns, &formats)?;
    for (i, patient) in patients.iter().enumerate() {
        let row = i as u32 + 1;
        sheet.write_string(row, 0, limit(&patient.name))?;
        sheet.write_string(row, 1, limit(&patient.head_family_name))?;
        write_date(sheet, row, 2, &patient.birth_date, &formats)?;
        if let Some(birth) = parse_stored(&patient.birth_date) {
            sheet.write_number(row, 3, age_years(birth, today))?;
        }
        sheet.write_string(row, 4, limit(&patient.address))?;
        write_date(sheet, row, 5, &patient.last_visit, &formats)?;
        sheet.write_number(row, 6, patient.visits as f64)?;
        sheet.write_string(row, 7, limit(&patient.id))?;
    }
    sheet.autofilter(0, 0, patients.len() as u32, columns.len() as u16 - 1)?;

    let visits = store.export_visits()?;
    let sheet = workbook.add_worksheet();
    sheet.set_name(t("Kunjungan", "Visits"))?;
    let columns = [
        (t("Tanggal", "Date"), 12.0),
        (t("Nama", "Name"), 28.0),
        (t("Nama KK", "Head of Family"), 24.0),
        (t("Diagnosis", "Diagnosis"), 90.0),
        (t("ID Pasien", "Patient ID"), 34.0),
        (t("ID Kunjungan", "Visit ID"), 34.0),
    ];
    header(sheet, &columns, &formats)?;
    for (i, visit) in visits.iter().enumerate() {
        let row = i as u32 + 1;
        write_date(sheet, row, 0, &visit.date, &formats)?;
        sheet.write_string_with_format(row, 1, limit(&visit.patient_name), &formats.wrap)?;
        sheet.write_string_with_format(row, 2, limit(&visit.head_family_name), &formats.wrap)?;
        let text = diagnosis::plain_text(&visit.diagnosis, visit.format);
        sheet.write_string_with_format(row, 3, limit(&text), &formats.wrap)?;
        sheet.write_string_with_format(row, 4, limit(&visit.patient_id), &formats.wrap)?;
        sheet.write_string_with_format(row, 5, limit(&visit.id), &formats.wrap)?;
    }
    sheet.autofilter(0, 0, visits.len() as u32, columns.len() as u16 - 1)?;

    workbook.save(path)?;
    Ok(ExportReport {
        patients: patients.len(),
        visits: visits.len(),
    })
}
