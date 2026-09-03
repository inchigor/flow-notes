## Flow Notes app icons

Put the master app icon at:

```text
src-tauri/icons/icon.png
```

Use a square 1024x1024 PNG. Then run:

```text
npm run app:icons
```

The script generates the Tauri icon assets used for the macOS app icon, Dock icon,
DMG icon, and bundle icon, including `icon.icns`, `icon.ico`, and the PNG sizes
referenced by `src-tauri/tauri.conf.json`.
