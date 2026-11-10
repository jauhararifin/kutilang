//! The databases opened recently, newest first, kept in a small text file:
//! one `<RFC 3339 time>\t<path>` line per database.

use std::path::{Path, PathBuf};

use anyhow::Result;
use chrono::{DateTime, FixedOffset, Local};

/// How many databases are remembered.
pub const MAX_RECENT: usize = 10;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecentDatabase {
    pub path: PathBuf,
    pub opened_at: DateTime<FixedOffset>,
}

pub struct RecentDatabases {
    file: PathBuf,
    entries: Vec<RecentDatabase>,
}

/// The folder of the application's own files, the one the original
/// application used (Go's `os.UserConfigDir`).
pub fn app_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("kutilang")
}

/// The database the original application used.
pub fn default_database() -> PathBuf {
    app_dir().join("db.sqlite3")
}

impl RecentDatabases {
    /// The list kept in the application folder.
    pub fn load_default() -> Self {
        Self::load(app_dir().join("recent-databases.txt"))
    }

    /// Reads the list from `file`; a missing or unreadable file is an empty
    /// list, and lines that cannot be read are skipped.
    pub fn load(file: PathBuf) -> Self {
        let entries = std::fs::read_to_string(&file)
            .unwrap_or_default()
            .lines()
            .filter_map(|line| {
                let (time, path) = line.split_once('\t')?;
                Some(RecentDatabase {
                    path: PathBuf::from(path),
                    opened_at: DateTime::parse_from_rfc3339(time).ok()?,
                })
            })
            .take(MAX_RECENT)
            .collect();
        Self { file, entries }
    }

    pub fn entries(&self) -> &[RecentDatabase] {
        &self.entries
    }

    /// Puts `path` first, as opened now.
    pub fn add(&mut self, path: &Path) -> Result<()> {
        let path = absolute(path);
        self.entries.retain(|entry| entry.path != path);
        self.entries.insert(
            0,
            RecentDatabase {
                path,
                opened_at: Local::now().fixed_offset(),
            },
        );
        self.entries.truncate(MAX_RECENT);
        self.save()
    }

    pub fn remove(&mut self, path: &Path) -> Result<()> {
        self.entries.retain(|entry| entry.path != path);
        self.save()
    }

    fn save(&self) -> Result<()> {
        if let Some(dir) = self.file.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let text: String = self
            .entries
            .iter()
            .map(|entry| {
                format!(
                    "{}\t{}\n",
                    entry.opened_at.to_rfc3339(),
                    entry.path.display()
                )
            })
            .collect();
        std::fs::write(&self.file, text)?;
        Ok(())
    }
}

/// The path made absolute, and resolved when the file exists, so the same
/// file is remembered once.
fn absolute(path: &Path) -> PathBuf {
    path.canonicalize()
        .or_else(|_| std::path::absolute(path))
        .unwrap_or_else(|_| path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remembers_databases_newest_first() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("config/recent.txt");
        let a = dir.path().join("a.sqlite3");
        let b = dir.path().join("b.sqlite3");
        std::fs::write(&a, "").unwrap();

        let mut recent = RecentDatabases::load(file.clone());
        assert!(recent.entries().is_empty());
        recent.add(&a).unwrap();
        recent.add(&b).unwrap();
        // Opening `a` again moves it first instead of listing it twice.
        recent.add(&dir.path().join("./a.sqlite3")).unwrap();

        let reloaded = RecentDatabases::load(file.clone());
        let paths: Vec<_> = reloaded.entries().iter().map(|e| e.path.clone()).collect();
        assert_eq!(paths, [a.canonicalize().unwrap(), absolute(&b)]);

        let mut reloaded = reloaded;
        reloaded.remove(&absolute(&b)).unwrap();
        assert_eq!(RecentDatabases::load(file).entries().len(), 1);
    }

    #[test]
    fn keeps_the_most_recent_ten_and_skips_bad_lines() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("recent.txt");
        let mut recent = RecentDatabases::load(file.clone());
        for i in 0..15 {
            recent
                .add(&dir.path().join(format!("{i}.sqlite3")))
                .unwrap();
        }
        let mut text = std::fs::read_to_string(&file).unwrap();
        text.insert_str(0, "not a line\nyesterday\t/x.sqlite3\n");
        std::fs::write(&file, text).unwrap();

        let recent = RecentDatabases::load(file);
        assert_eq!(recent.entries().len(), MAX_RECENT);
        assert!(recent.entries()[0].path.ends_with("14.sqlite3"));
    }
}
