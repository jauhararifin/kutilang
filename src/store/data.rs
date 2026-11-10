//! Backing up, restoring and exporting the whole database.

use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, bail};
use chrono::Local;
use rusqlite::params;

use super::Store;
use crate::model::DiagnosisFormat;

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
                crate::i18n::pick(
                    format!("tidak dapat menulis ke {}", dir.display()),
                    format!("cannot write to {}", dir.display()),
                )
            })?
            .into_temp_path();
        std::fs::remove_file(&temp)?;
        self.conn
            .execute("VACUUM INTO ?1", params![temp.to_string_lossy()])
            .with_context(|| {
                crate::i18n::pick(
                    format!("tidak dapat mencadangkan ke {}", path.display()),
                    format!("cannot back up to {}", path.display()),
                )
            })?;
        temp.persist(path).with_context(|| {
            crate::i18n::pick(
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
            crate::i18n::pick(
                format!("tidak dapat membaca {}", path.display()),
                format!("cannot read {}", path.display()),
            )
        })?;
        let (store, _) = Store::open(&copy).with_context(|| {
            crate::i18n::pick(
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
            bail!(crate::i18n::pick(
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
