# Kutilang v2

The Kutilang medical record application rewritten in Rust with
[GPUI](https://gpui.rs), through [GPUI Kit](https://gpui-kit.com) (GPUI plus
the gpui-component library). It is a native desktop window; there is no web
server and no browser involved.

It opens the same `db.sqlite3` as the original Go application and upgrades it
automatically.

## Running

```sh
cargo run --release                         # starts on the database picker
cargo run --release -- --db path/to/db.sqlite3   # opens this file directly
cargo run --release -- --db path/to/db.sqlite3 --upgrade-only   # upgrade and exit
```

The application starts on a list of the recently opened databases, with
**Open Database…** and **New Database…**; File → Open Database… (Cmd/Ctrl-O)
or Close Database comes back to it. The list also offers the database the
original application used, when it exists, so an existing installation is one
click away:

| OS      | Path                                                   |
| ------- | ------------------------------------------------------ |
| Windows | `%AppData%\kutilang\db.sqlite3`                        |
| macOS   | `~/Library/Application Support/kutilang/db.sqlite3`    |
| Linux   | `~/.config/kutilang/db.sqlite3`                        |

To build for Windows, run `cargo build --release` on Windows with the MSVC
Rust toolchain; the result is `target\release\kutilangv2.exe` (release builds
do not open a console window).

## Upgrading a database of the original application

When a database written by the Go application is opened, before anything is
modified:

1. a complete copy is written next to it, e.g.
   `db.v1-backup-20261109-135138.sqlite3`;
2. the tables are rebuilt in the new schema in a single transaction (if
   anything fails, nothing changes);
3. foreign keys are checked and the file is compacted.

The app shows a notice with the name of the backup the first time. Both layouts
the Go application produced are recognised: dates as `YYYY-MM-DD` text (the
released version) and the earlier Unix timestamps. Files that do not look like
a Kutilang database, or that were written by a newer version, are refused and
left untouched.

Once upgraded, keep using v2 on that file; the old application does not
understand the new schema. To go back, use the backup copy.

### Schema changes

* `PRAGMA user_version` records the schema version (currently 3) and a `meta`
  table where/when the database was migrated from. Later versions are reached
  through small, ordered upgrade steps (`src/store/schema.rs`); version 3 added
  the `settings` table.
* Columns are `NOT NULL` with checks; rows carry `created_at`/`updated_at`.
* `visits.diagnosis_format` is `html` (written with the old editor) or
  `markdown` (written with v2). Old entries are kept byte for byte.
* `patient_keywords(keyword, patient_id)` replaces `patient_indices`. It no
  longer stores a copy of the last visit date, which in the old application
  went stale (adding a visit did not update it, editing a profile blanked it),
  so the search order was wrong for many patients. Search now orders by the
  patient's real last visit.
* `last_visit` is corrected during the upgrade where a visit was re-dated
  after it was written.

## Using it

The window follows the platform's conventions: a title bar drawn by the app, a
sidebar, list-and-detail panes with a draggable divider, dialogs for forms, a
light theme, and menus with keyboard shortcuts (Cmd on
macOS, Ctrl on Windows and Linux).

* **Patients** (Cmd/Ctrl-1): every patient, with the total count. Typing in the
  search field narrows the list at once: each word must start a word of the
  name or head of family name (`nur` finds Nur, Nuri and Nurul; `siti jo` finds
  Siti whose head of family is Joko). Sort by most recent visit or by name.
  Arrow keys move through the list. The selected patient is shown in two
  columns: their details above the list of all their visits (how many, when,
  and a one-line summary of each), and next to it the selected visit's
  diagnosis (Print, Edit, Delete).
  **New Entry** opens the editor, filled with the template.
* **New patient**: the person icon above the list, or Cmd/Ctrl-N.
* **Visits** (Cmd/Ctrl-2): the history of every visit of every patient, newest
  first, grouped by day, each with a one-line summary. Filter by patient name
  and by dates (Hari ini, 7 hari terakhir, 30 hari terakhir, Bulan ini, Tahun
  ini, or any range). **Open Patient** jumps to the patient.
* **Settings** (Cmd/Ctrl-,): backup, restore and export; the template new
  entries start from, edited in Markdown with a live preview (**Reset to
  Default** brings back the original template); which database file is open.
* **Back Up…** (File menu or Settings): saves a copy of everything (patients,
  visits, settings) to a file of your choice. It is an ordinary SQLite file.
* **Restore…**: replaces everything with a backup, after showing how many
  patients and visits it holds. The current data is first saved next to the
  database as `<name>.before-restore-<date>.sqlite3`. A database of the original
  Go application can be restored too (it is upgraded on the way; the chosen file
  itself is never modified).
* **Export to Excel…**: writes an `.xlsx` workbook with a *Pasien* sheet
  (name, head of family, birth date, age, address, last visit, number of
  visits) and a *Kunjungan* sheet (date, patient, and the full diagnosis as
  text), with real date cells, frozen headers and filters.

Differences from the original application:

* **Diagnosis editor.** GPUI has no WYSIWYG rich text editor, so entries are
  written in Markdown (`**bold**`, `- item`, `1. item`) with a Bold button and
  a Preview toggle; every line break typed is kept. Old entries are displayed
  from their HTML as they were. When an old entry is edited, its text is shown
  converted to Markdown; if only the date is changed, the original HTML is
  kept.
* **Search** narrows as you type and ignores punctuation (`Ma'ruf` is found
  with `maruf`). The original only searched on Enter, for whole words, and
  listed patients matching any of the words.
* **Deleting asks for confirmation** (the original deleted entries at once).
  Deleting still only hides records; nothing is erased from the database.
* **Printing** writes the same page the original application printed and opens
  it in the browser's print dialog.
* Dates are shown in Indonesian with day-level relative times
  (`3 hari yang lalu`).

## The original application

The Go application this replaces (a local web server with a React page) is
kept in [`kutilang_old/`](kutilang_old) for reference. Its scripts expect to run
from that folder, e.g. `cd kutilang_old && ./scripts/build.sh`.

## Development

```sh
cargo test                 # unit tests, store and migration tests
cargo clippy --all-targets
```

Checks against a real database (not part of the repository):

```sh
# every old entry converts to Markdown without losing text or bold
KUTILANG_REAL_DB=/path/to/copy.sqlite3 cargo test --test real_data -- --ignored
# render the main screens to PNG files (writes to the database: use a copy)
cargo run --example screenshot -- /path/to/copy.sqlite3 /tmp/shots nur
```

Layout: `src/store` (SQLite, schema, migrations, settings), `src/diagnosis`
(template, HTML to Markdown, summaries), `src/ui` (GPUI views: `app` for the
window and sidebar, `patients` and `patient_detail`, `visits`, `settings`,
`visit_editor`, `profile_form`), `src/model.rs`, `src/dates.rs`,
`src/keywords.rs`, `src/print.rs`.

The icons in `assets/icons` come from [Lucide](https://lucide.dev) (ISC
license, see `assets/icons/LICENSE-LUCIDE`); the rest are bundled with GPUI Kit.
