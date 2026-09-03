const launchAtLoginInput = document.querySelector("#launchAtLogin");
const launchBehaviorGroup = document.querySelector("#launchBehaviorGroup");
const launchBehaviorInputs = document.querySelectorAll('input[name="launchBehavior"]');
const quickCaptureShortcutInput = document.querySelector("#quickCaptureShortcut");
const saveShortcutButton = document.querySelector("#saveShortcut");
const resetShortcutButton = document.querySelector("#resetShortcut");
const hideQuickCaptureOnBlurInput = document.querySelector("#hideQuickCaptureOnBlur");
const noteTextSizeInputs = document.querySelectorAll('input[name="noteTextSize"]');
const automaticBackupsInput = document.querySelector("#automaticBackups");
const lastBackupLabel = document.querySelector("#lastBackupLabel");
const openBackupFolderButton = document.querySelector("#openBackupFolder");
const settingsStatus = document.querySelector("#settingsStatus");

let settingsBusy = false;
let settingsShortcutAccelerator = "";

function getTauriInvoke() {
  return window.__TAURI__?.core?.invoke || null;
}

async function invokeTauri(command, args = {}) {
  const invoke = getTauriInvoke();

  if (!invoke) {
    throw new Error("Tauri invoke is not available");
  }

  return invoke(command, args);
}

function setSettingsStatus(message, { error = false } = {}) {
  if (!settingsStatus) {
    return;
  }

  settingsStatus.textContent = message;
  settingsStatus.classList.toggle("error", error);
}

function setSettingsBusy(isBusy) {
  settingsBusy = isBusy;

  if (launchAtLoginInput) {
    launchAtLoginInput.disabled = isBusy;
  }

  if (quickCaptureShortcutInput) {
    quickCaptureShortcutInput.disabled = isBusy;
  }

  if (saveShortcutButton) {
    saveShortcutButton.disabled = isBusy;
  }

  if (resetShortcutButton) {
    resetShortcutButton.disabled = isBusy;
  }

  if (hideQuickCaptureOnBlurInput) {
    hideQuickCaptureOnBlurInput.disabled = isBusy;
  }

  noteTextSizeInputs.forEach((input) => {
    input.disabled = isBusy;
  });

  if (automaticBackupsInput) {
    automaticBackupsInput.disabled = isBusy;
  }

  if (openBackupFolderButton) {
    openBackupFolderButton.disabled = isBusy;
  }

  updateLaunchBehaviorAvailability();
}

function updateLaunchBehaviorAvailability() {
  if (!launchBehaviorGroup || !launchAtLoginInput) {
    return;
  }

  const enabled = launchAtLoginInput.checked && !settingsBusy;
  launchBehaviorGroup.disabled = !enabled;

  launchBehaviorInputs.forEach((input) => {
    input.disabled = !enabled;
  });
}

function formatShortcutDisplay(accelerator) {
  if (!accelerator) {
    return "";
  }

  return accelerator
    .split("+")
    .map((part) => {
      const token = part.trim();
      const normalized = token.toLowerCase();

      if (
        normalized === "commandorcontrol"
        || normalized === "cmdorctrl"
        || normalized === "command"
        || normalized === "cmd"
        || normalized === "super"
        || normalized === "meta"
      ) {
        return "⌘";
      }

      if (normalized === "control" || normalized === "ctrl") {
        return "⌃";
      }

      if (normalized === "option" || normalized === "alt") {
        return "⌥";
      }

      if (normalized === "shift") {
        return "⇧";
      }

      return token;
    })
    .join("");
}

function shortcutInputToAccelerator(value) {
  const raw = (value || "").trim();

  if (!raw) {
    return "";
  }

  if (raw.includes("+")) {
    return raw.replace(/\s+/g, "");
  }

  const modifiers = [];
  let index = 0;

  while (index < raw.length) {
    const symbol = raw[index];

    if (symbol === "⌘") {
      modifiers.push("CommandOrControl");
      index += 1;
      continue;
    }

    if (symbol === "⌃") {
      modifiers.push("Control");
      index += 1;
      continue;
    }

    if (symbol === "⌥") {
      modifiers.push("Option");
      index += 1;
      continue;
    }

    if (symbol === "⇧") {
      modifiers.push("Shift");
      index += 1;
      continue;
    }

    break;
  }

  const key = raw.slice(index).trim();

  if (!key) {
    return raw;
  }

  return [...modifiers, key].join("+");
}

function showShortcutDisplay() {
  quickCaptureShortcutInput.value = formatShortcutDisplay(settingsShortcutAccelerator);
  quickCaptureShortcutInput.dataset.mode = "display";
}

function showShortcutAccelerator() {
  quickCaptureShortcutInput.value = settingsShortcutAccelerator;
  quickCaptureShortcutInput.dataset.mode = "edit";
  quickCaptureShortcutInput.select();
}

function setShortcutAccelerator(accelerator) {
  settingsShortcutAccelerator = accelerator || "";
  showShortcutDisplay();
}

function setLaunchBehaviorValue(value) {
  launchBehaviorInputs.forEach((input) => {
    input.checked = input.value === value;
  });
}

function setNoteTextSizeValue(value) {
  noteTextSizeInputs.forEach((input) => {
    input.checked = input.value === value;
  });
}

function formatLastBackup(value) {
  if (!value) {
    return "Never";
  }

  const parsed = new Date(value);

  if (Number.isNaN(parsed.getTime())) {
    return value;
  }

  return new Intl.DateTimeFormat(undefined, {
    dateStyle: "medium",
    timeStyle: "short",
  }).format(parsed);
}

function updateLastBackupLabel(value) {
  if (!lastBackupLabel) {
    return;
  }

  lastBackupLabel.textContent = `Last backup: ${formatLastBackup(value)}`;
}

function applySettingsToForm(settings) {
  launchAtLoginInput.checked = Boolean(settings.launch_at_login);
  setLaunchBehaviorValue(settings.launch_behavior || "background");
  setShortcutAccelerator(settings.quick_capture_shortcut || "");
  hideQuickCaptureOnBlurInput.checked = Boolean(settings.hide_quick_capture_on_blur);
  setNoteTextSizeValue(settings.note_text_size || "default");
  if (automaticBackupsInput) {
    automaticBackupsInput.checked = Boolean(settings.automatic_backups);
  }
  updateLastBackupLabel(settings.last_automatic_backup_at || null);
  updateLaunchBehaviorAvailability();
}

async function loadSettings() {
  const settings = await invokeTauri("get_settings");
  applySettingsToForm(settings);
}

async function handleLaunchAtLoginChange() {
  if (settingsBusy) {
    return;
  }

  setSettingsBusy(true);
  setSettingsStatus("");

  try {
    const enabled = await invokeTauri("set_launch_at_login", {
      enabled: launchAtLoginInput.checked,
    });
    launchAtLoginInput.checked = Boolean(enabled);
    setSettingsStatus(enabled ? "Launch at login enabled" : "Launch at login disabled");
  } catch (error) {
    console.error(error);
    launchAtLoginInput.checked = !launchAtLoginInput.checked;
    setSettingsStatus(error.message || "Could not update launch at login", { error: true });
  } finally {
    setSettingsBusy(false);
  }
}

async function handleLaunchBehaviorChange(event) {
  if (settingsBusy || !event.target.checked) {
    return;
  }

  setSettingsBusy(true);
  setSettingsStatus("");

  try {
    const behavior = await invokeTauri("set_launch_behavior", {
      behavior: event.target.value,
    });
    setLaunchBehaviorValue(behavior);
    setSettingsStatus(
      behavior === "background"
        ? "Login launch will start in background"
        : "Login launch will open the main window",
    );
  } catch (error) {
    console.error(error);
    try {
      const settings = await invokeTauri("get_settings");
      setLaunchBehaviorValue(settings.launch_behavior || "background");
    } catch (reloadError) {
      console.error(reloadError);
    }
    setSettingsStatus(error.message || "Could not update launch behavior", { error: true });
  } finally {
    setSettingsBusy(false);
  }
}

async function handleSaveShortcut() {
  if (settingsBusy) {
    return;
  }

  setSettingsBusy(true);
  setSettingsStatus("");

  const accelerator = shortcutInputToAccelerator(quickCaptureShortcutInput.value);

  try {
    const shortcut = await invokeTauri("set_quick_capture_shortcut", {
      shortcut: accelerator,
    });
    setShortcutAccelerator(shortcut);
    setSettingsStatus("Shortcut updated");
  } catch (error) {
    console.error(error);
    setSettingsStatus(error.message || "Could not update shortcut", { error: true });

    try {
      const settings = await invokeTauri("get_settings");
      setShortcutAccelerator(settings.quick_capture_shortcut || "");
    } catch (reloadError) {
      console.error(reloadError);
      showShortcutDisplay();
    }
  } finally {
    setSettingsBusy(false);
  }
}

async function handleResetShortcut() {
  if (settingsBusy) {
    return;
  }

  setSettingsBusy(true);
  setSettingsStatus("");

  try {
    const shortcut = await invokeTauri("reset_quick_capture_shortcut");
    setShortcutAccelerator(shortcut);
    setSettingsStatus("Shortcut reset to default");
  } catch (error) {
    console.error(error);
    setSettingsStatus(error.message || "Could not reset shortcut", { error: true });
    showShortcutDisplay();
  } finally {
    setSettingsBusy(false);
  }
}

async function handleHideOnBlurChange() {
  if (settingsBusy) {
    return;
  }

  setSettingsBusy(true);
  setSettingsStatus("");

  try {
    const enabled = await invokeTauri("set_hide_quick_capture_on_blur", {
      enabled: hideQuickCaptureOnBlurInput.checked,
    });
    hideQuickCaptureOnBlurInput.checked = Boolean(enabled);
    setSettingsStatus(enabled ? "Hide on focus loss enabled" : "Hide on focus loss disabled");
  } catch (error) {
    console.error(error);
    hideQuickCaptureOnBlurInput.checked = !hideQuickCaptureOnBlurInput.checked;
    setSettingsStatus(error.message || "Could not update setting", { error: true });
  } finally {
    setSettingsBusy(false);
  }
}

async function handleNoteTextSizeChange(event) {
  if (settingsBusy || !event.target.checked) {
    return;
  }

  setSettingsBusy(true);
  setSettingsStatus("");

  try {
    const size = await invokeTauri("set_note_text_size", {
      size: event.target.value,
    });
    setNoteTextSizeValue(size);
    setSettingsStatus("Note text size updated");
  } catch (error) {
    console.error(error);
    try {
      const settings = await invokeTauri("get_settings");
      setNoteTextSizeValue(settings.note_text_size || "default");
    } catch (reloadError) {
      console.error(reloadError);
    }
    setSettingsStatus(error.message || "Could not update note text size", { error: true });
  } finally {
    setSettingsBusy(false);
  }
}

async function handleAutomaticBackupsChange() {
  if (settingsBusy || !automaticBackupsInput) {
    return;
  }

  setSettingsBusy(true);
  setSettingsStatus("");

  try {
    const enabled = await invokeTauri("set_automatic_backups", {
      enabled: automaticBackupsInput.checked,
    });
    automaticBackupsInput.checked = Boolean(enabled);
    setSettingsStatus(enabled ? "Automatic backups enabled" : "Automatic backups disabled");
  } catch (error) {
    console.error(error);
    automaticBackupsInput.checked = !automaticBackupsInput.checked;
    setSettingsStatus(error.message || "Could not update automatic backups", { error: true });
  } finally {
    setSettingsBusy(false);
  }
}

async function handleOpenBackupFolder() {
  if (settingsBusy) {
    return;
  }

  setSettingsBusy(true);
  setSettingsStatus("");

  try {
    await invokeTauri("open_backup_folder");
    setSettingsStatus("Backup folder opened");
  } catch (error) {
    console.error(error);
    setSettingsStatus(error.message || "Could not open backup folder", { error: true });
  } finally {
    setSettingsBusy(false);
  }
}

if (getTauriInvoke()) {
  launchAtLoginInput.addEventListener("change", handleLaunchAtLoginChange);
  launchBehaviorInputs.forEach((input) => {
    input.addEventListener("change", handleLaunchBehaviorChange);
  });
  saveShortcutButton.addEventListener("click", handleSaveShortcut);
  resetShortcutButton.addEventListener("click", handleResetShortcut);
  hideQuickCaptureOnBlurInput.addEventListener("change", handleHideOnBlurChange);
  noteTextSizeInputs.forEach((input) => {
    input.addEventListener("change", handleNoteTextSizeChange);
  });
  if (automaticBackupsInput) {
    automaticBackupsInput.addEventListener("change", handleAutomaticBackupsChange);
  }
  if (openBackupFolderButton) {
    openBackupFolderButton.addEventListener("click", handleOpenBackupFolder);
  }
  quickCaptureShortcutInput.addEventListener("focus", () => {
    if (!settingsBusy) {
      showShortcutAccelerator();
    }
  });
  quickCaptureShortcutInput.addEventListener("blur", () => {
    if (settingsBusy) {
      return;
    }

    showShortcutDisplay();
  });
  saveShortcutButton.addEventListener("mousedown", (event) => {
    event.preventDefault();
  });
  resetShortcutButton.addEventListener("mousedown", (event) => {
    event.preventDefault();
  });
  quickCaptureShortcutInput.addEventListener("keydown", (event) => {
    if (event.key === "Enter") {
      event.preventDefault();
      handleSaveShortcut();
    }
  });

  loadSettings().catch((error) => {
    console.error(error);
    setSettingsStatus(error.message || "Could not load settings", { error: true });
  });
}
