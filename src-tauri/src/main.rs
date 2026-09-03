use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
    time::Duration,
};

use chrono::{DateTime, Local, SecondsFormat, Utc};
use rfd::FileDialog;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use tauri::{
    menu::{Menu, MenuItem, Submenu},
    tray::{TrayIcon, TrayIconBuilder},
    AppHandle, Emitter, Manager, RunEvent, State, WindowEvent,
};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};
use tauri_plugin_global_shortcut::{
    Builder as GlobalShortcutBuilder, GlobalShortcutExt, ShortcutState, ShortcutWrapper,
};
use tauri_plugin_notification::NotificationExt;
use uuid::Uuid;

const REMINDER_SOUND_NAME: &str = "Glass";
const DEFAULT_QUICK_CAPTURE_SHORTCUT: &str = "CommandOrControl+Shift+Space";
const QUICK_CAPTURE_WINDOW: &str = "quick-capture";
const SETTINGS_WINDOW: &str = "settings";
const NOTES_CHANGED_EVENT: &str = "notes-changed";
const QUICK_CAPTURE_FOCUS_EVENT: &str = "quick-capture-focus";
const NOTE_TEXT_SIZE_CHANGED_EVENT: &str = "note-text-size-changed";
const LOGIN_LAUNCH_ARG: &str = "--login-launch";
const LAUNCH_AGENT_LABEL: &str = "Flow Notes";
const LEGACY_LAUNCH_AGENT_LABEL: &str = "flow-notes";
const SETTING_LAUNCH_AT_LOGIN: &str = "launch_at_login";
const SETTING_LAUNCH_BEHAVIOR: &str = "launch_behavior";
const SETTING_NOTE_TEXT_SIZE: &str = "note_text_size";
const SETTING_QUICK_CAPTURE_SHORTCUT: &str = "quick_capture_shortcut";
const SETTING_HIDE_QUICK_CAPTURE_ON_BLUR: &str = "hide_quick_capture_on_blur";
const SETTING_AUTOMATIC_BACKUPS: &str = "automatic_backups";
const SETTING_LAST_AUTOMATIC_BACKUP_AT: &str = "last_automatic_backup_at";
const DEFAULT_LAUNCH_BEHAVIOR: &str = "background";
const DEFAULT_NOTE_TEXT_SIZE: &str = "default";
const AUTOMATIC_BACKUP_FORMAT: &str = "flow-notes-backup";
const AUTOMATIC_BACKUP_VERSION: u32 = 1;
const AUTOMATIC_BACKUP_RETENTION: usize = 14;
const AUTOMATIC_BACKUP_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);
const AUTOMATIC_BACKUP_CHECK_INTERVAL: Duration = Duration::from_secs(15 * 60);
const REMINDER_CHECK_INTERVAL: Duration = Duration::from_secs(45);
const ENV_FORCE_AUTOMATIC_BACKUP: &str = "FLOW_NOTES_FORCE_AUTOMATIC_BACKUP";

struct AppState {
    db: Mutex<Connection>,
    is_quitting: AtomicBool,
    quick_capture_shortcut: Mutex<String>,
    hide_quick_capture_on_blur: AtomicBool,
    reminder_notify_pending_clear: Mutex<HashSet<String>>,
}

struct TrayState {
    _tray: TrayIcon,
}

#[derive(Serialize)]
struct Note {
    id: String,
    text: String,
    created_at: String,
    updated_at: String,
    favorite: bool,
    remind_at: Option<String>,
}

#[derive(Deserialize, Serialize)]
struct BackupNote {
    id: String,
    text: String,
    created_at: String,
    updated_at: String,
    favorite: i64,
    #[serde(default)]
    remind_at: Option<String>,
}

#[derive(Serialize)]
struct ExportResult {
    exported: bool,
    count: usize,
    path: Option<String>,
}

#[derive(Serialize)]
struct ImportResult {
    imported: bool,
    added: usize,
    skipped: usize,
}

#[derive(Serialize)]
struct AppSettings {
    launch_at_login: bool,
    launch_behavior: String,
    note_text_size: String,
    quick_capture_shortcut: String,
    hide_quick_capture_on_blur: bool,
    automatic_backups: bool,
    last_automatic_backup_at: Option<String>,
}

#[derive(Deserialize, Serialize)]
struct AutomaticBackupFile {
    format: String,
    version: u32,
    created_at: String,
    notes: Vec<BackupNote>,
    settings: HashMap<String, String>,
}

fn now_iso() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}

fn normalize_remind_at(value: Option<String>) -> Result<Option<String>, String> {
    let Some(value) = value else {
        return Ok(None);
    };

    let value = value.trim();

    if value.is_empty() {
        return Ok(None);
    }

    let parsed = DateTime::parse_from_rfc3339(value)
        .map_err(|_| "Reminder must be a valid date".to_string())?;

    Ok(Some(
        parsed
            .with_timezone(&Utc)
            .to_rfc3339_opts(SecondsFormat::Millis, true),
    ))
}

fn default_backup_file_name() -> String {
    format!("flow-notes-backup-{}.json", Local::now().format("%Y-%m-%d"))
}

fn create_schema(db: &Connection) -> Result<(), String> {
    db.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS notes (
          id TEXT PRIMARY KEY,
          text TEXT NOT NULL,
          created_at TEXT NOT NULL,
          updated_at TEXT NOT NULL,
          favorite INTEGER DEFAULT 0,
          remind_at TEXT
        );

        CREATE TABLE IF NOT EXISTS settings (
          key TEXT PRIMARY KEY,
          value TEXT NOT NULL
        );
        ",
    )
    .map_err(|error| error.to_string())?;

    let columns = db
        .prepare("PRAGMA table_info(notes)")
        .map_err(|error| error.to_string())?
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;

    if !columns.iter().any(|column| column == "remind_at") {
        db.execute("ALTER TABLE notes ADD COLUMN remind_at TEXT", [])
            .map_err(|error| error.to_string())?;
    }

    Ok(())
}

fn bool_to_setting(value: bool) -> &'static str {
    if value {
        "1"
    } else {
        "0"
    }
}

fn setting_to_bool(value: &str) -> bool {
    matches!(value.trim(), "1" | "true" | "TRUE" | "True")
}

fn get_setting(db: &Connection, key: &str) -> Result<Option<String>, String> {
    db.query_row(
        "SELECT value FROM settings WHERE key = ?1",
        params![key],
        |row| row.get(0),
    )
    .optional()
    .map_err(|error| error.to_string())
}

fn set_setting(db: &Connection, key: &str, value: &str) -> Result<(), String> {
    db.execute(
        "
        INSERT INTO settings (key, value) VALUES (?1, ?2)
        ON CONFLICT(key) DO UPDATE SET value = excluded.value
        ",
        params![key, value],
    )
    .map_err(|error| error.to_string())?;

    Ok(())
}

fn validate_shortcut(accelerator: &str) -> Result<String, String> {
    let accelerator = accelerator.trim();

    if accelerator.is_empty() {
        return Err("Shortcut cannot be empty".to_string());
    }

    ShortcutWrapper::try_from(accelerator)
        .map_err(|error| format!("Invalid shortcut: {error}"))?;

    Ok(accelerator.to_string())
}

fn normalize_launch_behavior(value: &str) -> Result<String, String> {
    match value.trim() {
        "open" | "background" => Ok(value.trim().to_string()),
        _ => Err("Launch behavior must be open or background".to_string()),
    }
}

fn normalize_note_text_size(value: &str) -> Result<String, String> {
    match value.trim() {
        "small" | "default" | "large" => Ok(value.trim().to_string()),
        _ => Err("Note text size must be small, default, or large".to_string()),
    }
}

fn is_login_launch() -> bool {
    std::env::args().any(|arg| arg == LOGIN_LAUNCH_ARG)
}

#[cfg(target_os = "macos")]
fn build_autostart_plugin() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    tauri_plugin_autostart::Builder::new()
        .macos_launcher(MacosLauncher::LaunchAgent)
        .app_name(LAUNCH_AGENT_LABEL)
        .args([LOGIN_LAUNCH_ARG])
        .build()
}

#[cfg(not(target_os = "macos"))]
fn build_autostart_plugin() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    tauri_plugin_autostart::Builder::new()
        .args([LOGIN_LAUNCH_ARG])
        .build()
}

fn ensure_settings(app: &tauri::App, db: &Connection) -> Result<(String, bool), String> {
    if get_setting(db, SETTING_LAUNCH_AT_LOGIN)?.is_none() {
        let enabled = app
            .autolaunch()
            .is_enabled()
            .map_err(|error| error.to_string())?;
        set_setting(db, SETTING_LAUNCH_AT_LOGIN, bool_to_setting(enabled))?;
    }

    if get_setting(db, SETTING_QUICK_CAPTURE_SHORTCUT)?.is_none() {
        set_setting(
            db,
            SETTING_QUICK_CAPTURE_SHORTCUT,
            DEFAULT_QUICK_CAPTURE_SHORTCUT,
        )?;
    }

    if get_setting(db, SETTING_HIDE_QUICK_CAPTURE_ON_BLUR)?.is_none() {
        set_setting(db, SETTING_HIDE_QUICK_CAPTURE_ON_BLUR, "1")?;
    }

    if get_setting(db, SETTING_LAUNCH_BEHAVIOR)?.is_none() {
        set_setting(db, SETTING_LAUNCH_BEHAVIOR, DEFAULT_LAUNCH_BEHAVIOR)?;
    }

    if get_setting(db, SETTING_NOTE_TEXT_SIZE)?.is_none() {
        set_setting(db, SETTING_NOTE_TEXT_SIZE, DEFAULT_NOTE_TEXT_SIZE)?;
    }

    if get_setting(db, SETTING_AUTOMATIC_BACKUPS)?.is_none() {
        set_setting(db, SETTING_AUTOMATIC_BACKUPS, "1")?;
    }

    let shortcut = get_setting(db, SETTING_QUICK_CAPTURE_SHORTCUT)?
        .unwrap_or_else(|| DEFAULT_QUICK_CAPTURE_SHORTCUT.to_string());
    let hide_on_blur = setting_to_bool(
        &get_setting(db, SETTING_HIDE_QUICK_CAPTURE_ON_BLUR)?.unwrap_or_else(|| "1".to_string()),
    );

    Ok((shortcut, hide_on_blur))
}

fn read_settings(db: &Connection) -> Result<AppSettings, String> {
    let launch_at_login = setting_to_bool(
        &get_setting(db, SETTING_LAUNCH_AT_LOGIN)?.unwrap_or_else(|| "0".to_string()),
    );
    let quick_capture_shortcut = get_setting(db, SETTING_QUICK_CAPTURE_SHORTCUT)?
        .unwrap_or_else(|| DEFAULT_QUICK_CAPTURE_SHORTCUT.to_string());
    let hide_quick_capture_on_blur = setting_to_bool(
        &get_setting(db, SETTING_HIDE_QUICK_CAPTURE_ON_BLUR)?.unwrap_or_else(|| "1".to_string()),
    );
    let launch_behavior = get_setting(db, SETTING_LAUNCH_BEHAVIOR)?
        .unwrap_or_else(|| DEFAULT_LAUNCH_BEHAVIOR.to_string());
    let note_text_size = get_setting(db, SETTING_NOTE_TEXT_SIZE)?
        .unwrap_or_else(|| DEFAULT_NOTE_TEXT_SIZE.to_string());
    let automatic_backups = setting_to_bool(
        &get_setting(db, SETTING_AUTOMATIC_BACKUPS)?.unwrap_or_else(|| "1".to_string()),
    );
    let last_automatic_backup_at = get_setting(db, SETTING_LAST_AUTOMATIC_BACKUP_AT)?;

    Ok(AppSettings {
        launch_at_login,
        launch_behavior,
        note_text_size,
        quick_capture_shortcut,
        hide_quick_capture_on_blur,
        automatic_backups,
        last_automatic_backup_at,
    })
}

fn row_to_note(row: &rusqlite::Row<'_>) -> rusqlite::Result<Note> {
    let favorite: i64 = row.get(4)?;

    Ok(Note {
        id: row.get(0)?,
        text: row.get(1)?,
        created_at: row.get(2)?,
        updated_at: row.get(3)?,
        favorite: favorite != 0,
        remind_at: row.get(5)?,
    })
}

fn row_to_backup_note(row: &rusqlite::Row<'_>) -> rusqlite::Result<BackupNote> {
    let favorite: i64 = row.get(4)?;

    Ok(BackupNote {
        id: row.get(0)?,
        text: row.get(1)?,
        created_at: row.get(2)?,
        updated_at: row.get(3)?,
        favorite: if favorite == 0 { 0 } else { 1 },
        remind_at: row.get(5)?,
    })
}

fn select_backup_notes(db: &Connection) -> Result<Vec<BackupNote>, String> {
    let mut statement = db
        .prepare(
            "
            SELECT id, text, created_at, updated_at, favorite, remind_at
            FROM notes
            ORDER BY created_at ASC
            ",
        )
        .map_err(|error| error.to_string())?;

    let notes = statement
        .query_map([], row_to_backup_note)
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;

    Ok(notes)
}

fn validate_backup_notes(notes: &[BackupNote]) -> Result<(), String> {
    for note in notes {
        if note.id.trim().is_empty()
            || note.text.trim().is_empty()
            || note.created_at.trim().is_empty()
            || note.updated_at.trim().is_empty()
        {
            return Err("Backup JSON has invalid note data".to_string());
        }
    }

    Ok(())
}

fn export_notes_to_path(db: &Connection, path: &Path) -> Result<usize, String> {
    let notes = select_backup_notes(db)?;
    let json = serde_json::to_string_pretty(&notes).map_err(|error| error.to_string())?;

    fs::write(path, json).map_err(|error| error.to_string())?;
    Ok(notes.len())
}

fn import_notes_from_path(db: &mut Connection, path: &Path) -> Result<ImportResult, String> {
    let contents = fs::read_to_string(path).map_err(|error| error.to_string())?;
    let notes: Vec<BackupNote> = serde_json::from_str(&contents)
        .map_err(|error| format!("Malformed backup JSON: {error}"))?;

    validate_backup_notes(&notes)?;

    let total = notes.len();
    let transaction = db.transaction().map_err(|error| error.to_string())?;
    let mut added = 0usize;

    {
        let mut statement = transaction
            .prepare(
                "
                INSERT OR IGNORE INTO notes (id, text, created_at, updated_at, favorite, remind_at)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                ",
            )
            .map_err(|error| error.to_string())?;

        for note in notes {
            let changed = statement
                .execute(params![
                    note.id,
                    note.text,
                    note.created_at,
                    note.updated_at,
                    if note.favorite == 0 { 0 } else { 1 },
                    note.remind_at
                ])
                .map_err(|error| error.to_string())?;
            added += changed;
        }
    }

    transaction.commit().map_err(|error| error.to_string())?;

    Ok(ImportResult {
        imported: true,
        added,
        skipped: total.saturating_sub(added),
    })
}

fn automatic_backup_dir(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join("backups")
}

fn automatic_backup_file_name() -> String {
    format!(
        "flow-notes-backup-{}.json",
        Local::now().format("%Y-%m-%d-%H%M%S")
    )
}

fn is_automatic_backup_file_name(name: &str) -> bool {
    let Some(stem) = name
        .strip_prefix("flow-notes-backup-")
        .and_then(|value| value.strip_suffix(".json"))
    else {
        return false;
    };

    stem.len() == 17 && stem.as_bytes().get(10) == Some(&b'-')
}

fn select_all_settings(db: &Connection) -> Result<HashMap<String, String>, String> {
    let mut statement = db
        .prepare("SELECT key, value FROM settings ORDER BY key ASC")
        .map_err(|error| error.to_string())?;

    let rows = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|error| error.to_string())?;

    let mut settings = HashMap::new();

    for row in rows {
        let (key, value) = row.map_err(|error| error.to_string())?;
        settings.insert(key, value);
    }

    Ok(settings)
}

fn build_automatic_backup(db: &Connection) -> Result<AutomaticBackupFile, String> {
    Ok(AutomaticBackupFile {
        format: AUTOMATIC_BACKUP_FORMAT.to_string(),
        version: AUTOMATIC_BACKUP_VERSION,
        created_at: now_iso(),
        notes: select_backup_notes(db)?,
        settings: select_all_settings(db)?,
    })
}

fn write_file_atomically(path: &Path, contents: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }

    let temp_path = path.with_extension("json.part");
    fs::write(&temp_path, contents).map_err(|error| error.to_string())?;

    if let Err(error) = fs::rename(&temp_path, path) {
        let _ = fs::remove_file(&temp_path);
        return Err(error.to_string());
    }

    Ok(())
}

fn prune_automatic_backups(backups_dir: &Path) -> Result<(), String> {
    let mut names = fs::read_dir(backups_dir)
        .map_err(|error| error.to_string())?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| is_automatic_backup_file_name(name))
        .collect::<Vec<_>>();

    names.sort_unstable();
    names.reverse();

    for name in names.iter().skip(AUTOMATIC_BACKUP_RETENTION) {
        let path = backups_dir.join(name);
        if let Err(error) = fs::remove_file(&path) {
            eprintln!(
                "Failed to remove old automatic backup {}: {error}",
                path.display()
            );
        }
    }

    Ok(())
}

fn should_run_automatic_backup(db: &Connection) -> Result<bool, String> {
    if !setting_to_bool(
        &get_setting(db, SETTING_AUTOMATIC_BACKUPS)?.unwrap_or_else(|| "1".to_string()),
    ) {
        return Ok(false);
    }

    if std::env::var(ENV_FORCE_AUTOMATIC_BACKUP).ok().as_deref() == Some("1") {
        return Ok(true);
    }

    let Some(last) = get_setting(db, SETTING_LAST_AUTOMATIC_BACKUP_AT)? else {
        return Ok(true);
    };

    let Some(parsed) = DateTime::parse_from_rfc3339(&last)
        .ok()
        .map(|value| value.with_timezone(&Utc))
    else {
        return Ok(true);
    };

    Ok(Utc::now().signed_duration_since(parsed)
        >= chrono::Duration::seconds(AUTOMATIC_BACKUP_INTERVAL.as_secs() as i64))
}

fn run_automatic_backup(app_data_dir: &Path, db: &Connection) -> Result<PathBuf, String> {
    let backups_dir = automatic_backup_dir(app_data_dir);
    fs::create_dir_all(&backups_dir).map_err(|error| error.to_string())?;

    let backup = build_automatic_backup(db)?;
    let created_at = backup.created_at.clone();
    let json = serde_json::to_string_pretty(&backup).map_err(|error| error.to_string())?;
    let path = backups_dir.join(automatic_backup_file_name());

    write_file_atomically(&path, &json)?;
    set_setting(db, SETTING_LAST_AUTOMATIC_BACKUP_AT, &created_at)?;
    prune_automatic_backups(&backups_dir)?;

    Ok(path)
}

fn maybe_run_automatic_backup(app: &AppHandle) {
    let app_data_dir = match app.path().app_data_dir() {
        Ok(dir) => dir,
        Err(error) => {
            eprintln!("Automatic backup skipped: could not resolve app data dir: {error}");
            return;
        }
    };

    let Some(state) = app.try_state::<AppState>() else {
        return;
    };

    let result = (|| -> Result<(), String> {
        let db = state.db.lock().map_err(|error| error.to_string())?;

        if !should_run_automatic_backup(&db)? {
            return Ok(());
        }

        let path = run_automatic_backup(&app_data_dir, &db)?;
        eprintln!("Automatic backup created at {}", path.display());
        Ok(())
    })();

    if let Err(error) = result {
        eprintln!("Automatic backup failed: {error}");
    }
}

fn start_automatic_backup_scheduler(app: &AppHandle) {
    let app_handle = app.clone();
    maybe_run_automatic_backup(&app_handle);

    std::thread::spawn(move || {
        loop {
            std::thread::sleep(AUTOMATIC_BACKUP_CHECK_INTERVAL);

            if app_handle
                .try_state::<AppState>()
                .map(|state| state.is_quitting.load(Ordering::SeqCst))
                .unwrap_or(true)
            {
                break;
            }

            maybe_run_automatic_backup(&app_handle);
        }
    });
}

fn select_due_reminder_notes(db: &Connection) -> Result<Vec<Note>, String> {
    let now = now_iso();
    let mut stmt = db
        .prepare(
            "
            SELECT id, text, created_at, updated_at, favorite, remind_at
            FROM notes
            WHERE remind_at IS NOT NULL AND remind_at <= ?1
            ORDER BY remind_at ASC
            ",
        )
        .map_err(|error| error.to_string())?;

    let notes = stmt
        .query_map(params![now], row_to_note)
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;

    Ok(notes)
}

fn clear_note_reminder(db: &Connection, id: &str) -> Result<Note, String> {
    let updated_at = now_iso();
    let changed = db
        .execute(
            "
            UPDATE notes
            SET remind_at = NULL, updated_at = ?1
            WHERE id = ?2 AND remind_at IS NOT NULL
            ",
            params![updated_at, id],
        )
        .map_err(|error| error.to_string())?;

    if changed == 0 {
        return Err("Reminder already cleared or note not found".to_string());
    }

    select_note(db, id)?.ok_or_else(|| "Note not found".to_string())
}

fn send_reminder_notification(app: &AppHandle, title: &str, body: &str) -> Result<(), String> {
    let title = title.to_string();
    let body = body.to_string();
    let app_for_main = app.clone();
    let (tx, rx) = std::sync::mpsc::sync_channel(1);

    app.run_on_main_thread(move || {
        let result = app_for_main
            .notification()
            .builder()
            .title(title)
            .body(body)
            .sound(REMINDER_SOUND_NAME)
            .show()
            .map_err(|error| error.to_string());
        let _ = tx.send(result);
    })
    .map_err(|error| error.to_string())?;

    rx.recv().map_err(|error| error.to_string())?
}

fn process_due_reminders(app: &AppHandle) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };

    if state.is_quitting.load(Ordering::SeqCst) {
        return;
    }

    let result = (|| -> Result<(), String> {
        let pending_ids: Vec<String> = {
            let pending = state
                .reminder_notify_pending_clear
                .lock()
                .map_err(|error| error.to_string())?;
            pending.iter().cloned().collect()
        };

        if !pending_ids.is_empty() {
            let db = state.db.lock().map_err(|error| error.to_string())?;
            for id in pending_ids {
                match clear_note_reminder(&db, &id) {
                    Ok(updated) => {
                        let _ = app.emit(NOTES_CHANGED_EVENT, &updated);
                        if let Ok(mut pending) = state.reminder_notify_pending_clear.lock() {
                            pending.remove(&id);
                        }
                    }
                    Err(error) => {
                        eprintln!("Failed to clear reminder for {id}: {error}");
                    }
                }
            }
        }

        let due_notes = {
            let db = state.db.lock().map_err(|error| error.to_string())?;
            select_due_reminder_notes(&db)?
        };

        if due_notes.is_empty() {
            return Ok(());
        }

        let pending_snapshot: HashSet<String> = state
            .reminder_notify_pending_clear
            .lock()
            .map_err(|error| error.to_string())?
            .clone();

        for note in due_notes {
            if pending_snapshot.contains(&note.id) {
                continue;
            }

            match send_reminder_notification(app, "Flow Notes Reminder", &note.text) {
                Ok(()) => {
                    let db = state.db.lock().map_err(|error| error.to_string())?;
                    match clear_note_reminder(&db, &note.id) {
                        Ok(updated) => {
                            let _ = app.emit(NOTES_CHANGED_EVENT, &updated);
                        }
                        Err(error) => {
                            eprintln!(
                                "Failed to clear reminder for {} after notification: {error}",
                                note.id
                            );
                            if let Ok(mut pending) = state.reminder_notify_pending_clear.lock() {
                                pending.insert(note.id);
                            }
                        }
                    }
                }
                Err(error) => {
                    eprintln!(
                        "Failed to show reminder notification for {}: {error}",
                        note.id
                    );
                }
            }
        }

        Ok(())
    })();

    if let Err(error) = result {
        eprintln!("Reminder check failed: {error}");
    }
}

fn start_reminder_scheduler(app: &AppHandle) {
    let app_handle = app.clone();
    process_due_reminders(&app_handle);

    std::thread::spawn(move || {
        loop {
            std::thread::sleep(REMINDER_CHECK_INTERVAL);

            if app_handle
                .try_state::<AppState>()
                .map(|state| state.is_quitting.load(Ordering::SeqCst))
                .unwrap_or(true)
            {
                break;
            }

            process_due_reminders(&app_handle);
        }
    });
}

#[cfg(target_os = "macos")]
fn open_path_in_file_manager(path: &Path) -> Result<(), String> {
    std::process::Command::new("open")
        .arg(path)
        .spawn()
        .map_err(|error| error.to_string())?;
    Ok(())
}

#[cfg(all(not(target_os = "macos"), target_os = "linux"))]
fn open_path_in_file_manager(path: &Path) -> Result<(), String> {
    std::process::Command::new("xdg-open")
        .arg(path)
        .spawn()
        .map_err(|error| error.to_string())?;
    Ok(())
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn open_path_in_file_manager(path: &Path) -> Result<(), String> {
    Err(format!(
        "Opening folders is not supported on this platform: {}",
        path.display()
    ))
}

fn delete_note_by_id(db: &Connection, id: &str) -> Result<(), String> {
    let changed = db
        .execute("DELETE FROM notes WHERE id = ?1", params![id])
        .map_err(|error| error.to_string())?;

    if changed == 0 {
        return Err("Note not found".to_string());
    }

    Ok(())
}

fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

fn show_settings_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(SETTINGS_WINDOW) {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

fn apply_login_launch_visibility(app: &tauri::App, db: &Connection) -> Result<(), String> {
    if !is_login_launch() {
        return Ok(());
    }

    let behavior = get_setting(db, SETTING_LAUNCH_BEHAVIOR)?
        .unwrap_or_else(|| DEFAULT_LAUNCH_BEHAVIOR.to_string());

    if behavior == "background" {
        if let Some(window) = app.get_webview_window("main") {
            let _ = window.hide();
        }
    }

    Ok(())
}

fn hide_quick_capture_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(QUICK_CAPTURE_WINDOW) {
        let _ = window.hide();
    }
}

fn show_quick_capture_window(app: &AppHandle) {
    let Some(window) = app.get_webview_window(QUICK_CAPTURE_WINDOW) else {
        return;
    };

    let _ = window.show();
    let _ = window.unminimize();
    let _ = window.set_focus();
    let _ = window.emit(QUICK_CAPTURE_FOCUS_EVENT, ());
}

fn mark_quitting(app: &AppHandle) {
    if let Some(state) = app.try_state::<AppState>() {
        state.is_quitting.store(true, Ordering::SeqCst);
    }
}

fn quit_application(app: &AppHandle) {
    mark_quitting(app);
    app.exit(0);
}

fn setup_close_to_hide(app: &tauri::App) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };

    let app_handle = app.handle().clone();
    let window_to_hide = window.clone();

    window.on_window_event(move |event| {
        if let WindowEvent::CloseRequested { api, .. } = event {
            let should_quit = app_handle
                .try_state::<AppState>()
                .map(|state| state.is_quitting.load(Ordering::SeqCst))
                .unwrap_or(false);

            if should_quit {
                return;
            }

            api.prevent_close();
            let _ = window_to_hide.hide();
        }
    });
}

fn setup_settings_close_to_hide(app: &tauri::App) {
    let Some(window) = app.get_webview_window(SETTINGS_WINDOW) else {
        return;
    };

    let app_handle = app.handle().clone();
    let window_to_hide = window.clone();

    window.on_window_event(move |event| {
        if let WindowEvent::CloseRequested { api, .. } = event {
            let should_quit = app_handle
                .try_state::<AppState>()
                .map(|state| state.is_quitting.load(Ordering::SeqCst))
                .unwrap_or(false);

            if should_quit {
                return;
            }

            api.prevent_close();
            let _ = window_to_hide.hide();
        }
    });
}

fn setup_quick_capture_close_to_hide(app: &tauri::App) {
    let Some(window) = app.get_webview_window(QUICK_CAPTURE_WINDOW) else {
        return;
    };

    let app_handle = app.handle().clone();
    let window_to_hide = window.clone();

    window.on_window_event(move |event| match event {
        WindowEvent::CloseRequested { api, .. } => {
            let should_quit = app_handle
                .try_state::<AppState>()
                .map(|state| state.is_quitting.load(Ordering::SeqCst))
                .unwrap_or(false);

            if should_quit {
                return;
            }

            api.prevent_close();
            let _ = window_to_hide.hide();
        }
        WindowEvent::Focused(false) => {
            let should_quit = app_handle
                .try_state::<AppState>()
                .map(|state| state.is_quitting.load(Ordering::SeqCst))
                .unwrap_or(false);

            if should_quit {
                return;
            }

            let hide_on_blur = app_handle
                .try_state::<AppState>()
                .map(|state| state.hide_quick_capture_on_blur.load(Ordering::SeqCst))
                .unwrap_or(true);

            if hide_on_blur {
                let _ = window_to_hide.hide();
            }
        }
        _ => {}
    });
}

fn setup_tray(app: &mut tauri::App) -> tauri::Result<TrayIcon> {
    let open = MenuItem::with_id(app, "open", "Open Flow Notes", true, None::<&str>)?;
    let settings =
        MenuItem::with_id(app, "settings", "Settings…", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Flow Notes", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &settings, &quit])?;

    let mut tray = TrayIconBuilder::with_id("flow-notes")
        .menu(&menu)
        .tooltip("Flow Notes")
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "open" => show_main_window(app),
            "settings" => show_settings_window(app),
            "quit" => quit_application(app),
            _ => {}
        });

    tray = tray
        .icon(tauri::include_image!("./icons/trayTemplate.png"))
        .icon_as_template(true);

    tray.build(app)
}

#[cfg(target_os = "macos")]
fn setup_app_menu(app: &mut tauri::App) -> tauri::Result<()> {
    let settings =
        MenuItem::with_id(app, "settings", "Settings…", true, Some("Cmd+,"))?;
    let quit = MenuItem::with_id(app, "quit", "Quit Flow Notes", true, Some("Cmd+Q"))?;
    let app_menu = Submenu::with_items(
        app,
        &app.package_info().name,
        true,
        &[&settings, &quit],
    )?;
    let menu = Menu::with_items(app, &[&app_menu])?;
    app.set_menu(menu)?;

    app.on_menu_event(|app, event| match event.id().as_ref() {
        "settings" => show_settings_window(app),
        "quit" => quit_application(app),
        _ => {}
    });

    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn setup_app_menu(_app: &mut tauri::App) -> tauri::Result<()> {
    Ok(())
}

fn set_launch_at_login_enabled(app: &AppHandle, enabled: bool) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        remove_legacy_login_item()?;
        remove_launch_agent_plist(LEGACY_LAUNCH_AGENT_LABEL)?;
    }

    let autostart = app.autolaunch();

    if enabled {
        if !autostart.is_enabled().map_err(|error| error.to_string())? {
            autostart.enable().map_err(|error| error.to_string())?;
        }

        prefer_installed_launch_agent_path()?;
    } else {
        if autostart.is_enabled().map_err(|error| error.to_string())? {
            autostart.disable().map_err(|error| error.to_string())?;
        }

        #[cfg(target_os = "macos")]
        remove_launch_agent_plist(LAUNCH_AGENT_LABEL)?;
    }

    Ok(())
}

#[cfg(target_os = "macos")]
fn launch_agents_dir() -> Result<PathBuf, String> {
    let home = std::env::var_os("HOME").ok_or_else(|| "HOME is not set".to_string())?;
    let dir = PathBuf::from(home).join("Library/LaunchAgents");
    fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    Ok(dir)
}

#[cfg(target_os = "macos")]
fn remove_launch_agent_plist(label: &str) -> Result<(), String> {
    let path = launch_agents_dir()?.join(format!("{label}.plist"));

    if path.exists() {
        fs::remove_file(&path).map_err(|error| error.to_string())?;
    }

    Ok(())
}

/// Removes only the legacy System Events Login Item named "Flow Notes".
/// Current autostart uses LaunchAgent only; this cleans Dock/System Settings leftovers.
#[cfg(target_os = "macos")]
fn remove_legacy_login_item() -> Result<(), String> {
    let script = format!(
        r#"tell application "System Events"
  if exists login item "{LAUNCH_AGENT_LABEL}" then
    delete login item "{LAUNCH_AGENT_LABEL}"
  end if
end tell"#
    );

    let output = std::process::Command::new("osascript")
        .args(["-e", &script])
        .output()
        .map_err(|error| error.to_string())?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        eprintln!("Failed to remove legacy Login Item '{LAUNCH_AGENT_LABEL}': {stderr}");
    }

    Ok(())
}

#[cfg(target_os = "macos")]
fn prefer_installed_launch_agent_path() -> Result<(), String> {
    let installed_executable = Path::new("/Applications/Flow Notes.app/Contents/MacOS/flow-notes");

    if !installed_executable.exists() {
        return Ok(());
    }

    let current_exe = std::env::current_exe().map_err(|error| error.to_string())?;

    if current_exe == installed_executable {
        return Ok(());
    }

    let plist_path = launch_agents_dir()?.join(format!("{LAUNCH_AGENT_LABEL}.plist"));
    let executable = installed_executable.to_string_lossy();
    let login_launch_arg = LOGIN_LAUNCH_ARG;
    let label = LAUNCH_AGENT_LABEL;
    let plist = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>{label}</string>
  <key>ProgramArguments</key>
  <array>
    <string>{executable}</string>
    <string>{login_launch_arg}</string>
  </array>
  <key>RunAtLoad</key>
  <true/>
</dict>
</plist>
"#
    );

    fs::write(plist_path, plist).map_err(|error| error.to_string())
}

#[cfg(not(target_os = "macos"))]
fn prefer_installed_launch_agent_path() -> Result<(), String> {
    Ok(())
}

#[cfg(target_os = "macos")]
fn migrate_macos_autostart(app: &AppHandle, db: &Connection) -> Result<(), String> {
    remove_legacy_login_item()?;
    remove_launch_agent_plist(LEGACY_LAUNCH_AGENT_LABEL)?;

    let enabled = setting_to_bool(
        &get_setting(db, SETTING_LAUNCH_AT_LOGIN)?.unwrap_or_else(|| "0".to_string()),
    );
    set_launch_at_login_enabled(app, enabled)
}

#[cfg(not(target_os = "macos"))]
fn migrate_macos_autostart(_app: &AppHandle, _db: &Connection) -> Result<(), String> {
    Ok(())
}

fn prepare_notifications(app: &tauri::App) {
    let _ = app.notification().request_permission();
}

fn pick_export_path(path: Option<String>) -> Option<PathBuf> {
    if let Some(path) = path {
        return Some(PathBuf::from(path));
    }

    FileDialog::new()
        .add_filter("JSON", &["json"])
        .set_file_name(default_backup_file_name())
        .save_file()
}

fn pick_import_path(path: Option<String>) -> Option<PathBuf> {
    if let Some(path) = path {
        return Some(PathBuf::from(path));
    }

    FileDialog::new().add_filter("JSON", &["json"]).pick_file()
}

#[tauri::command]
fn get_notes(state: State<'_, AppState>) -> Result<Vec<Note>, String> {
    let db = state.db.lock().map_err(|error| error.to_string())?;
    let mut statement = db
        .prepare(
            "
            SELECT id, text, created_at, updated_at, favorite, remind_at
            FROM notes
            ORDER BY created_at ASC
            ",
        )
        .map_err(|error| error.to_string())?;

    let notes = statement
        .query_map([], row_to_note)
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;

    Ok(notes)
}

#[tauri::command]
fn create_note(
    app: AppHandle,
    state: State<'_, AppState>,
    text: String,
    favorite: Option<bool>,
    remind_at: Option<String>,
) -> Result<Note, String> {
    let text = text.trim().to_string();

    if text.is_empty() {
        return Err("Text is required".to_string());
    }

    let db = state.db.lock().map_err(|error| error.to_string())?;
    let now = now_iso();
    let id = Uuid::new_v4().to_string();
    let favorite = favorite.unwrap_or(false);
    let remind_at = normalize_remind_at(remind_at)?;

    db.execute(
        "
        INSERT INTO notes (id, text, created_at, updated_at, favorite, remind_at)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6)
        ",
        params![id, text, now, now, if favorite { 1 } else { 0 }, remind_at],
    )
    .map_err(|error| error.to_string())?;

    let note = select_note(&db, &id)?.ok_or_else(|| "Note not found".to_string())?;
    let _ = app.emit(NOTES_CHANGED_EVENT, &note);
    Ok(note)
}

#[tauri::command]
fn hide_quick_capture(app: AppHandle) -> Result<(), String> {
    hide_quick_capture_window(&app);
    Ok(())
}

#[tauri::command]
fn get_settings(state: State<'_, AppState>) -> Result<AppSettings, String> {
    let db = state.db.lock().map_err(|error| error.to_string())?;
    read_settings(&db)
}

#[tauri::command]
fn set_launch_at_login(
    app: AppHandle,
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<bool, String> {
    set_launch_at_login_enabled(&app, enabled)?;

    let db = state.db.lock().map_err(|error| error.to_string())?;
    set_setting(&db, SETTING_LAUNCH_AT_LOGIN, bool_to_setting(enabled))?;

    Ok(enabled)
}

fn apply_quick_capture_shortcut(
    app: &AppHandle,
    state: &AppState,
    shortcut: String,
) -> Result<String, String> {
    let next = validate_shortcut(&shortcut)?;
    let current = state
        .quick_capture_shortcut
        .lock()
        .map_err(|error| error.to_string())?
        .clone();

    if next == current {
        return Ok(current);
    }

    app.global_shortcut()
        .register(next.as_str())
        .map_err(|error| format!("Could not register shortcut: {error}"))?;

    if let Err(error) = app.global_shortcut().unregister(current.as_str()) {
        let _ = app.global_shortcut().unregister(next.as_str());
        return Err(format!("Could not update shortcut: {error}"));
    }

    {
        let db = state.db.lock().map_err(|error| error.to_string())?;
        set_setting(&db, SETTING_QUICK_CAPTURE_SHORTCUT, &next)?;
    }

    *state
        .quick_capture_shortcut
        .lock()
        .map_err(|error| error.to_string())? = next.clone();

    Ok(next)
}

#[tauri::command]
fn set_quick_capture_shortcut(
    app: AppHandle,
    state: State<'_, AppState>,
    shortcut: String,
) -> Result<String, String> {
    apply_quick_capture_shortcut(&app, &state, shortcut)
}

#[tauri::command]
fn reset_quick_capture_shortcut(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<String, String> {
    apply_quick_capture_shortcut(&app, &state, DEFAULT_QUICK_CAPTURE_SHORTCUT.to_string())
}

#[tauri::command]
fn set_hide_quick_capture_on_blur(
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<bool, String> {
    {
        let db = state.db.lock().map_err(|error| error.to_string())?;
        set_setting(
            &db,
            SETTING_HIDE_QUICK_CAPTURE_ON_BLUR,
            bool_to_setting(enabled),
        )?;
    }

    state
        .hide_quick_capture_on_blur
        .store(enabled, Ordering::SeqCst);

    Ok(enabled)
}

#[tauri::command]
fn set_launch_behavior(state: State<'_, AppState>, behavior: String) -> Result<String, String> {
    let next = normalize_launch_behavior(&behavior)?;
    let db = state.db.lock().map_err(|error| error.to_string())?;
    set_setting(&db, SETTING_LAUNCH_BEHAVIOR, &next)?;

    Ok(next)
}

#[tauri::command]
fn set_note_text_size(
    app: AppHandle,
    state: State<'_, AppState>,
    size: String,
) -> Result<String, String> {
    let next = normalize_note_text_size(&size)?;
    {
        let db = state.db.lock().map_err(|error| error.to_string())?;
        set_setting(&db, SETTING_NOTE_TEXT_SIZE, &next)?;
    }

    let _ = app.emit(NOTE_TEXT_SIZE_CHANGED_EVENT, &next);

    Ok(next)
}

#[tauri::command]
fn show_settings(app: AppHandle) -> Result<(), String> {
    show_settings_window(&app);
    Ok(())
}

#[tauri::command]
fn set_automatic_backups(state: State<'_, AppState>, enabled: bool) -> Result<bool, String> {
    let db = state.db.lock().map_err(|error| error.to_string())?;
    set_setting(
        &db,
        SETTING_AUTOMATIC_BACKUPS,
        bool_to_setting(enabled),
    )?;

    Ok(enabled)
}

#[tauri::command]
fn open_backup_folder(app: AppHandle) -> Result<String, String> {
    let app_data_dir = app.path().app_data_dir().map_err(|error| error.to_string())?;
    let backups_dir = automatic_backup_dir(&app_data_dir);
    fs::create_dir_all(&backups_dir).map_err(|error| error.to_string())?;
    open_path_in_file_manager(&backups_dir)?;

    Ok(backups_dir.display().to_string())
}

#[tauri::command]
fn run_automatic_backup_now(app: AppHandle, state: State<'_, AppState>) -> Result<Option<String>, String> {
    let app_data_dir = app.path().app_data_dir().map_err(|error| error.to_string())?;
    let db = state.db.lock().map_err(|error| error.to_string())?;

    if !setting_to_bool(
        &get_setting(&db, SETTING_AUTOMATIC_BACKUPS)?.unwrap_or_else(|| "1".to_string()),
    ) {
        return Ok(None);
    }

    let path = run_automatic_backup(&app_data_dir, &db)?;

    Ok(Some(path.display().to_string()))
}

#[tauri::command]
fn update_note(
    state: State<'_, AppState>,
    id: String,
    text: Option<String>,
    favorite: Option<bool>,
    remind_at: Option<String>,
    clear_reminder: Option<bool>,
) -> Result<Note, String> {
    let db = state.db.lock().map_err(|error| error.to_string())?;
    let current = select_note(&db, &id)?.ok_or_else(|| "Note not found".to_string())?;
    let next_text = text.unwrap_or(current.text).trim().to_string();

    if next_text.is_empty() {
        return Err("Text is required".to_string());
    }

    let next_favorite = favorite.unwrap_or(current.favorite);
    let next_remind_at = if clear_reminder.unwrap_or(false) {
        None
    } else {
        normalize_remind_at(remind_at)?.or(current.remind_at)
    };
    let updated_at = now_iso();

    db.execute(
        "
        UPDATE notes
        SET text = ?1, favorite = ?2, remind_at = ?3, updated_at = ?4
        WHERE id = ?5
        ",
        params![
            next_text,
            if next_favorite { 1 } else { 0 },
            next_remind_at,
            updated_at,
            id
        ],
    )
    .map_err(|error| error.to_string())?;

    select_note(&db, &id)?.ok_or_else(|| "Note not found".to_string())
}

#[tauri::command]
fn delete_note(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let db = state.db.lock().map_err(|error| error.to_string())?;
    delete_note_by_id(&db, &id)
}

#[tauri::command]
fn export_notes(state: State<'_, AppState>, path: Option<String>) -> Result<ExportResult, String> {
    let Some(path) = pick_export_path(path) else {
        return Ok(ExportResult {
            exported: false,
            count: 0,
            path: None,
        });
    };

    let db = state.db.lock().map_err(|error| error.to_string())?;
    let count = export_notes_to_path(&db, &path)?;

    Ok(ExportResult {
        exported: true,
        count,
        path: Some(path.display().to_string()),
    })
}

#[tauri::command]
fn import_notes(state: State<'_, AppState>, path: Option<String>) -> Result<ImportResult, String> {
    let Some(path) = pick_import_path(path) else {
        return Ok(ImportResult {
            imported: false,
            added: 0,
            skipped: 0,
        });
    };

    let mut db = state.db.lock().map_err(|error| error.to_string())?;
    import_notes_from_path(&mut db, &path)
}

#[tauri::command]
fn is_launch_at_login_enabled(app: AppHandle) -> Result<bool, String> {
    app.autolaunch()
        .is_enabled()
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn show_reminder_notification(
    app: AppHandle,
    title: Option<String>,
    body: String,
) -> Result<bool, String> {
    send_reminder_notification(
        &app,
        &title.unwrap_or_else(|| "Flow Notes Reminder".to_string()),
        &body,
    )?;

    Ok(true)
}

fn select_note(db: &Connection, id: &str) -> Result<Option<Note>, String> {
    db.query_row(
        "
        SELECT id, text, created_at, updated_at, favorite, remind_at
        FROM notes
        WHERE id = ?1
        ",
        params![id],
        row_to_note,
    )
    .optional()
    .map_err(|error| error.to_string())
}

fn main() {
    let global_shortcut_plugin = GlobalShortcutBuilder::new()
        .with_handler(|app, _shortcut, event| {
            if event.state == ShortcutState::Pressed {
                show_quick_capture_window(app);
            }
        })
        .build();

    tauri::Builder::default()
        .plugin(build_autostart_plugin())
        .plugin(tauri_plugin_notification::init())
        .plugin(global_shortcut_plugin)
        .setup(|app| {
            let app_data_dir = app.path().app_data_dir()?;
            fs::create_dir_all(&app_data_dir)?;

            let db = Connection::open(app_data_dir.join("notes.db"))?;
            create_schema(&db).map_err(std::io::Error::other)?;
            let (shortcut, hide_on_blur) =
                ensure_settings(app, &db).map_err(std::io::Error::other)?;

            if let Err(error) = migrate_macos_autostart(app.handle(), &db) {
                eprintln!("Failed to migrate macOS autostart: {error}");
            }

            if let Err(error) = app.global_shortcut().register(shortcut.as_str()) {
                eprintln!("Failed to register Quick Capture shortcut '{shortcut}': {error}");
            }

            apply_login_launch_visibility(app, &db).map_err(std::io::Error::other)?;

            app.manage(AppState {
                db: Mutex::new(db),
                is_quitting: AtomicBool::new(false),
                quick_capture_shortcut: Mutex::new(shortcut),
                hide_quick_capture_on_blur: AtomicBool::new(hide_on_blur),
                reminder_notify_pending_clear: Mutex::new(HashSet::new()),
            });

            setup_close_to_hide(app);
            setup_settings_close_to_hide(app);
            setup_quick_capture_close_to_hide(app);
            setup_app_menu(app)?;
            let tray = setup_tray(app)?;
            app.manage(TrayState { _tray: tray });
            prepare_notifications(app);
            start_automatic_backup_scheduler(app.handle());
            start_reminder_scheduler(app.handle());

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_notes,
            create_note,
            update_note,
            delete_note,
            export_notes,
            import_notes,
            is_launch_at_login_enabled,
            show_reminder_notification,
            hide_quick_capture,
            get_settings,
            set_launch_at_login,
            set_launch_behavior,
            set_note_text_size,
            set_quick_capture_shortcut,
            reset_quick_capture_shortcut,
            set_hide_quick_capture_on_blur,
            set_automatic_backups,
            open_backup_folder,
            run_automatic_backup_now,
            show_settings
        ])
        .build(tauri::generate_context!())
        .expect("error while building Flow Notes")
        .run(|app, event| match event {
            RunEvent::Reopen { .. } => show_main_window(app),
            RunEvent::ExitRequested { api, .. } => {
                let should_quit = app
                    .try_state::<AppState>()
                    .map(|state| state.is_quitting.load(Ordering::SeqCst))
                    .unwrap_or(false);

                if !should_quit {
                    api.prevent_exit();
                }
            }
            _ => {}
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_backup_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("flow-notes-{name}-{}.json", Uuid::new_v4()))
    }

    fn insert_sample_note(db: &Connection, id: &str, favorite: i64) {
        db.execute(
            "
            INSERT INTO notes (id, text, created_at, updated_at, favorite, remind_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            ",
            params![
                id,
                "A calm local note",
                "2026-05-27T10:00:00.000Z",
                "2026-05-27T10:00:00.000Z",
                favorite,
                Option::<String>::None
            ],
        )
        .expect("sample note should insert");
    }

    #[test]
    fn export_writes_backup_json_with_integer_favorites() {
        let db = Connection::open_in_memory().expect("db should open");
        create_schema(&db).expect("schema should create");
        insert_sample_note(&db, "note-1", 1);

        let path = temp_backup_path("export");
        let count = export_notes_to_path(&db, &path).expect("export should succeed");
        let contents = fs::read_to_string(&path).expect("backup should exist");
        let parsed: Vec<BackupNote> = serde_json::from_str(&contents).expect("json should parse");

        assert_eq!(count, 1);
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].id, "note-1");
        assert_eq!(parsed[0].favorite, 1);

        fs::remove_file(path).ok();
    }

    #[test]
    fn import_merges_without_duplicate_ids() {
        let source = Connection::open_in_memory().expect("source db should open");
        create_schema(&source).expect("source schema should create");
        insert_sample_note(&source, "note-1", 0);

        let path = temp_backup_path("import");
        export_notes_to_path(&source, &path).expect("export should succeed");

        let mut target = Connection::open_in_memory().expect("target db should open");
        create_schema(&target).expect("target schema should create");

        let first =
            import_notes_from_path(&mut target, &path).expect("first import should succeed");
        let second =
            import_notes_from_path(&mut target, &path).expect("second import should succeed");
        let count: i64 = target
            .query_row("SELECT COUNT(*) FROM notes", [], |row| row.get(0))
            .expect("count should query");

        assert!(first.imported);
        assert_eq!(first.added, 1);
        assert_eq!(first.skipped, 0);
        assert_eq!(second.added, 0);
        assert_eq!(second.skipped, 1);
        assert_eq!(count, 1);

        fs::remove_file(path).ok();
    }

    #[test]
    fn delete_removes_note_and_errors_when_missing() {
        let db = Connection::open_in_memory().expect("db should open");
        create_schema(&db).expect("schema should create");
        insert_sample_note(&db, "note-to-delete", 0);

        delete_note_by_id(&db, "note-to-delete").expect("delete should succeed");
        let count: i64 = db
            .query_row(
                "SELECT COUNT(*) FROM notes WHERE id = ?1",
                params!["note-to-delete"],
                |row| row.get(0),
            )
            .expect("count should query");

        assert_eq!(count, 0);
        assert!(delete_note_by_id(&db, "note-to-delete").is_err());
    }

    #[test]
    fn automatic_backup_filename_is_recognized() {
        assert!(is_automatic_backup_file_name(
            "flow-notes-backup-2026-08-22-162500.json"
        ));
        assert!(!is_automatic_backup_file_name("flow-notes-backup-2026-08-22.json"));
        assert!(!is_automatic_backup_file_name("other-backup.json"));
    }

    #[test]
    fn automatic_backup_writes_snapshot_with_settings() {
        let db = Connection::open_in_memory().expect("db should open");
        create_schema(&db).expect("schema should create");
        set_setting(&db, SETTING_AUTOMATIC_BACKUPS, "1").expect("setting should save");
        insert_sample_note(&db, "note-1", 1);

        let dir = std::env::temp_dir().join(format!("flow-notes-auto-{}", Uuid::new_v4()));
        fs::create_dir_all(&dir).expect("temp dir should create");

        let path = run_automatic_backup(&dir, &db).expect("automatic backup should succeed");
        let contents = fs::read_to_string(&path).expect("backup should exist");
        let parsed: AutomaticBackupFile =
            serde_json::from_str(&contents).expect("json should parse");

        assert_eq!(parsed.format, AUTOMATIC_BACKUP_FORMAT);
        assert_eq!(parsed.version, AUTOMATIC_BACKUP_VERSION);
        assert_eq!(parsed.notes.len(), 1);
        assert_eq!(
            parsed.settings.get(SETTING_AUTOMATIC_BACKUPS),
            Some(&"1".to_string())
        );

        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn select_due_reminder_notes_returns_only_overdue() {
        let db = Connection::open_in_memory().expect("db should open");
        create_schema(&db).expect("schema should create");

        db.execute(
            "
            INSERT INTO notes (id, text, created_at, updated_at, favorite, remind_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6), (?7, ?8, ?9, ?10, ?11, ?12)
            ",
            params![
                "due-note",
                "Due",
                "2026-05-27T10:00:00.000Z",
                "2026-05-27T10:00:00.000Z",
                0,
                "2026-05-27T10:00:00.000Z",
                "future-note",
                "Future",
                "2026-05-27T10:00:00.000Z",
                "2026-05-27T10:00:00.000Z",
                0,
                "2099-05-27T10:00:00.000Z",
            ],
        )
        .expect("notes should insert");

        let due = select_due_reminder_notes(&db).expect("due notes should query");

        assert_eq!(due.len(), 1);
        assert_eq!(due[0].id, "due-note");
    }

    #[test]
    fn clear_note_reminder_clears_remind_at() {
        let db = Connection::open_in_memory().expect("db should open");
        create_schema(&db).expect("schema should create");

        db.execute(
            "
            INSERT INTO notes (id, text, created_at, updated_at, favorite, remind_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            ",
            params![
                "reminder-note",
                "Reminder",
                "2026-05-27T10:00:00.000Z",
                "2026-05-27T10:00:00.000Z",
                0,
                "2026-05-27T11:00:00.000Z",
            ],
        )
        .expect("note should insert");

        let updated = clear_note_reminder(&db, "reminder-note").expect("clear should succeed");

        assert_eq!(updated.id, "reminder-note");
        assert_eq!(updated.remind_at, None);
        assert!(clear_note_reminder(&db, "reminder-note").is_err());
    }

    #[test]
    fn schema_migrates_existing_notes_without_reminders() {
        let db = Connection::open_in_memory().expect("db should open");
        db.execute_batch(
            "
            CREATE TABLE notes (
              id TEXT PRIMARY KEY,
              text TEXT NOT NULL,
              created_at TEXT NOT NULL,
              updated_at TEXT NOT NULL,
              favorite INTEGER DEFAULT 0
            );
            INSERT INTO notes (id, text, created_at, updated_at, favorite)
            VALUES ('legacy-note', 'Legacy', '2026-05-27T10:00:00.000Z', '2026-05-27T10:00:00.000Z', 0);
            ",
        )
        .expect("legacy schema should create");

        create_schema(&db).expect("schema should migrate");

        let note = select_note(&db, "legacy-note")
            .expect("note should select")
            .expect("note should exist");

        assert_eq!(note.id, "legacy-note");
        assert_eq!(note.remind_at, None);
    }
}
