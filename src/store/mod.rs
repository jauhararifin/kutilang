//! SQLite storage.

mod data;
mod migrate;
mod schema;

use std::path::Path;
use std::time::Duration;

use anyhow::{Result, bail};
use chrono::{Local, NaiveDate};
use rusqlite::types::Value;
use rusqlite::{Connection, OptionalExtension as _, Row, params, params_from_iter};
use uuid::Uuid;

pub use data::{Backup, ExportPatient, ExportVisit, timestamped_path};
pub use migrate::MigrationReport;
pub use schema::SCHEMA_VERSION;

use crate::dates;
use crate::diagnosis;
use crate::keywords::{self, MAX_QUERY_KEYWORDS};
use crate::model::{
    Cursor, DiagnosisFormat, Page, Patient, PatientOrder, PatientProfile, Visit, VisitSummary,
};

/// The settings key of the entry template.
const TEMPLATE_SETTING: &str = "diagnosis_template";

const PATIENT_COLUMNS: &str = "p.id, p.name, p.head_family_name, p.birth_date, p.address, p.last_visit";
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
        bail!("Missing diagnosis");
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
        let report = migrate::upgrade(&conn, path)?;
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
                &format!("SELECT {PATIENT_COLUMNS} FROM patients p WHERE p.id = ?1 AND p.is_deleted = 0"),
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
            birth_date: dates::to_stored(profile.birth_date),
            address: profile.address.clone(),
            last_visit: dates::to_stored(today),
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
                dates::to_stored(profile.birth_date),
                profile.address,
                now()
            ],
        )?;
        if changed == 0 {
            bail!("Patient not found");
        }
        tx.execute("DELETE FROM patient_keywords WHERE patient_id = ?1", [id])?;
        insert_keywords(&tx, id, &profile.name, &profile.head_family_name)?;
        tx.commit()?;
        self.get_patient(id)?
            .ok_or_else(|| anyhow::anyhow!("Patient not found"))
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
            bail!("Patient not found");
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
        let count: i64 = self
            .conn
            .query_row("SELECT count(*) FROM patients WHERE is_deleted = 0", [], |r| r.get(0))?;
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
            args.push(Value::Text(dates::to_stored(from)));
        }
        if let Some(to) = to {
            sql.push_str(" AND v.date <= ?");
            args.push(Value::Text(dates::to_stored(to)));
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
            .query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| r.get(0))
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
        self.conn.execute("DELETE FROM settings WHERE key = ?1", [key])?;
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
                &format!("SELECT {VISIT_COLUMNS} FROM visits v WHERE v.id = ?1 AND v.is_deleted = 0"),
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
        let mut sql =
            format!("SELECT {VISIT_COLUMNS} FROM visits v WHERE v.patient_id = ? AND v.is_deleted = 0");
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

    /// Records a visit, written in Markdown.
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
            bail!("Patient does not exist");
        }

        let visit = Visit {
            id: new_id(),
            patient_id: patient_id.to_string(),
            date: dates::to_stored(date),
            diagnosis: diagnosis.to_string(),
            format: DiagnosisFormat::Markdown,
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
    /// is then stored as Markdown). Leaving the text alone keeps a visit of
    /// the original application in its original HTML.
    pub fn update_visit(&self, id: &str, date: NaiveDate, diagnosis: Option<&str>) -> Result<Visit> {
        if let Some(diagnosis) = diagnosis {
            check_diagnosis(diagnosis)?;
        }
        let Some(visit) = self.get_visit(id)? else {
            bail!("Visit not found");
        };
        let date = dates::to_stored(date);
        let tx = self.conn.unchecked_transaction()?;
        match diagnosis {
            Some(diagnosis) => tx.execute(
                "UPDATE visits SET date = ?2, diagnosis = ?3, diagnosis_format = 'markdown', updated_at = ?4
                 WHERE id = ?1",
                params![id, date, diagnosis, now()],
            )?,
            None => tx.execute(
                "UPDATE visits SET date = ?2, updated_at = ?3 WHERE id = ?1",
                params![id, date, now()],
            )?,
        };
        raise_last_visit(&tx, &visit.patient_id, &date)?;
        tx.commit()?;
        self.get_visit(id)?
            .ok_or_else(|| anyhow::anyhow!("Visit not found"))
    }

    /// Hides a visit; the row stays in the database.
    pub fn delete_visit(&self, id: &str) -> Result<()> {
        let changed = self.conn.execute(
            "UPDATE visits SET is_deleted = 1, updated_at = ?2 WHERE id = ?1 AND is_deleted = 0",
            params![id, now()],
        )?;
        if changed == 0 {
            bail!("Visit not found");
        }
        Ok(())
    }
}

/// Restricts a query to patients whose name words start with every word of
/// `query`. `id_column` is the patient id column of the query.
fn push_name_filter(query: &str, id_column: &str, sql: &mut String, args: &mut Vec<Value>) -> Result<()> {
    let words = keywords::tokenize(query);
    if words.len() > MAX_QUERY_KEYWORDS {
        bail!("Search query is too long");
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

fn insert_keywords(conn: &Connection, patient_id: &str, name: &str, head_family_name: &str) -> Result<()> {
    let mut stmt =
        conn.prepare_cached("INSERT INTO patient_keywords (keyword, patient_id) VALUES (?1, ?2)")?;
    for keyword in keywords::patient_keywords(name, head_family_name) {
        stmt.execute(params![keyword, patient_id])?;
    }
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
