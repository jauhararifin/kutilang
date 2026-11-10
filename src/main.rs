// No console window for the released Windows application.
#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]

use std::path::PathBuf;
use std::process::ExitCode;

use kutilangv2::store::Store;

const HELP: &str = "\
Kutilang: medical records for a clinic.

Usage: kutilangv2 [--db PATH] [--upgrade-only]

Options:
  --db PATH        Open this database. Without it, the application starts on
                   a list of the recently opened databases.
  --upgrade-only   Upgrade the database to the current format (if needed),
                   print what was done and exit without opening a window.
                   Without --db, the original application's database:
                   <config dir>/kutilang/db.sqlite3
  -h, --help       Show this help.
  -V, --version    Show the version.

A database of the original application is upgraded automatically when it is
opened; a copy of the original file is saved next to it first.";

struct Args {
    db: Option<PathBuf>,
    upgrade_only: bool,
}

fn parse_args() -> Result<Option<Args>, String> {
    let mut args = Args {
        db: None,
        upgrade_only: false,
    };
    let mut iter = std::env::args_os().skip(1);
    while let Some(arg) = iter.next() {
        match arg.to_str() {
            Some("-h" | "--help") => {
                println!("{HELP}");
                return Ok(None);
            }
            Some("-V" | "--version") => {
                println!("kutilangv2 {}", env!("CARGO_PKG_VERSION"));
                return Ok(None);
            }
            Some("--upgrade-only") => args.upgrade_only = true,
            Some("--db") => {
                let path = iter.next().ok_or("--db needs a path")?;
                args.db = Some(PathBuf::from(path));
            }
            Some(other) if other.starts_with("--db=") => {
                args.db = Some(PathBuf::from(&other["--db=".len()..]));
            }
            // macOS passes this when started from Finder.
            Some(other) if other.starts_with("-psn_") => {}
            _ => return Err(format!("unknown argument {arg:?}, see --help")),
        }
    }
    Ok(Some(args))
}

fn main() -> ExitCode {
    let args = match parse_args() {
        Ok(Some(args)) => args,
        Ok(None) => return ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("kutilangv2: {message}");
            return ExitCode::FAILURE;
        }
    };
    if args.upgrade_only {
        let db_path = args.db.unwrap_or_else(kutilangv2::default_database);
        let existed = db_path.exists();
        return match Store::open(&db_path) {
            Ok((store, report)) => {
                match report {
                    None if !existed => println!("Created {}", db_path.display()),
                    Some(report) => {
                        println!("Upgraded {}", db_path.display());
                        println!("  patients: {}", report.patients);
                        println!("  visits: {}", report.visits);
                        println!(
                            "  visits without a patient (left out): {}",
                            report.orphan_visits
                        );
                        println!(
                            "  corrected last visit dates: {}",
                            report.corrected_last_visits
                        );
                        if let Some(backup) = report.backup_path {
                            println!("  backup of the original: {}", backup.display());
                        }
                    }
                    None => println!("{} is up to date", db_path.display()),
                }
                match store.count_patients() {
                    Ok(count) => println!("  patients in the database: {count}"),
                    Err(error) => eprintln!("kutilangv2: {error:#}"),
                }
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("kutilangv2: cannot open {}: {error:#}", db_path.display());
                ExitCode::FAILURE
            }
        };
    }

    kutilangv2::ui::run(args.db);
    ExitCode::SUCCESS
}
