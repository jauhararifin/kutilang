//! The v2 database schema.
//!
//! Versioning uses `PRAGMA user_version`. The Go app never set it, so a
//! database it created reports `0` and is recognised by its tables instead.

/// Schema version written by this app.
pub const SCHEMA_VERSION: i64 = 3;

/// Steps from one schema version to the next: the version each step
/// produces and its SQL. A database is brought up to date by running every
/// step above its current version, in order.
pub const UPGRADES: &[(i64, &str)] = &[(
    3,
    "CREATE TABLE settings (
         key   TEXT PRIMARY KEY NOT NULL,
         value TEXT NOT NULL
     ) WITHOUT ROWID;",
)];

/// The tables of schema version 2, the first version of this app. Newer
/// versions are reached through [`UPGRADES`].
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
