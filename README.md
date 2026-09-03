# Flow Notes

A local-first notes utility for macOS, built with Tauri.

Flow Notes keeps notes on the device. It has no account system, cloud sync,
analytics, telemetry, or updater configured in the current application code.

## Features

- Fast note capture and chronological note stream
- Search, favorites, inline editing, and deletion
- Quick Capture window with a configurable global shortcut
- Reminders with native notifications
- Tray menu, background mode, and launch-at-login setting
- JSON import and export
- Automatic local backups
- Settings for launch behavior, Quick Capture, note text size, and backups
- Local SQLite storage

## Screenshots

Screenshots will be added later.

## Privacy

Notes, settings, and automatic backups are stored locally in the app's macOS
application-data location. Notes are not sent to a remote service by the
current application code. Flow Notes has no account system, cloud sync,
analytics, telemetry, or updater configured.

## Requirements

- macOS
- Node.js and npm for development
- Rust toolchain
- Tauri development prerequisites for macOS

The configured release target is macOS. No Windows or Linux release support is
claimed here.

## Development

Install dependencies and start the Tauri development workflow:

```sh
npm install
npm run tauri:dev
```

The project uses Tauri's static frontend workflow with the files in `public/`.

## Build

```sh
npm run tauri:build
```

The macOS app bundle is created under
`src-tauri/target/release/bundle/macos/`, and the DMG under
`src-tauri/target/release/bundle/dmg/`.

## Data and backups

Flow Notes stores its SQLite database in its macOS application-data location.
Automatic backups are written locally alongside app data. Notes can also be
exported to and imported from JSON files through the app.

## License

[MIT](LICENSE)
