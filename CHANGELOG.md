# Changelog

## 1.1.0 - 2026-10-03

- Refreshed the desktop interface with a charcoal sidebar, a light note stream,
  restrained green accents, and consistent monochrome action icons.
- Added a draggable header and a dismiss button to Quick Capture. Its position
  is retained while the app is running, and dismissing it keeps the current draft.
- Added word counts and explicit save buttons to the main composer and Quick Capture.
- Moved JSON import and export into Settings. Imported notes refresh the main
  stream without restarting the app.
- Updated the Settings interface and accessibility labels.
- Restored note actions appearing on hover or keyboard focus. Touch controls
  remain visible, and selected favorites use a solid star with a clear action label.
- Added frontend interaction tests using a mock Tauri bridge.
- Added a regression test for preserving notes, favorites, reminders, and
  settings when an existing database is reopened.

The app identifier, SQLite schema, and reminder API are unchanged. Notes and
settings remain in the existing local application-data location.

## 1.0.0 - 2026-09-03

- First public release for macOS on Apple Silicon.
- Local note stream, search, favorites, inline editing, and reminders.
- Quick Capture with a configurable global shortcut.
- Tray menu, background mode, and launch at login.
- JSON import and export, automatic local backups, and local SQLite storage.
