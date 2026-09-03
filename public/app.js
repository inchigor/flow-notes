const timeline = document.querySelector("#timeline");
const noteForm = document.querySelector("#noteForm");
const noteInput = document.querySelector("#noteInput");
const searchInput = document.querySelector("#searchInput");
const notesCount = document.querySelector("#notesCount");
const navButtons = document.querySelectorAll(".nav-button");
const sidebarToggle = document.querySelector(".sidebar-toggle");
const exportNotesButton = document.querySelector("#exportNotes");
const importNotesButton = document.querySelector("#importNotes");
const backupStatus = document.querySelector("#backupStatus");
const openSettingsButton = document.querySelector("#openSettings");

let notes = [];
let inlineEditingId = null;
let reminderEditingId = null;
let currentView = "all";
const draftKey = "flow-notes:draft";

const dateFormatter = new Intl.DateTimeFormat("ru-RU", {
  day: "numeric",
  month: "long",
  year: "numeric",
});

const timeFormatter = new Intl.DateTimeFormat("ru-RU", {
  hour: "2-digit",
  minute: "2-digit",
});

const reminderFormatter = new Intl.DateTimeFormat("ru-RU", {
  day: "numeric",
  month: "short",
  hour: "2-digit",
  minute: "2-digit",
});

const weekdayFormatter = new Intl.DateTimeFormat("en-US", {
  weekday: "long",
});

function getDateKey(value) {
  const date = new Date(value);
  return `${date.getFullYear()}-${date.getMonth()}-${date.getDate()}`;
}

function getStartOfDay(value) {
  const date = value ? new Date(value) : new Date();
  return new Date(date.getFullYear(), date.getMonth(), date.getDate());
}

function formatDate(value) {
  return dateFormatter.format(new Date(value));
}

function formatDayDivider(value) {
  const date = getStartOfDay(value);
  const today = getStartOfDay();
  const diffDays = Math.round((today - date) / 86400000);

  if (diffDays === 0) {
    return "TODAY";
  }

  if (diffDays === 1) {
    return "YESTERDAY";
  }

  if (diffDays > 1 && diffDays < 7) {
    return weekdayFormatter.format(date).toUpperCase();
  }

  return formatDate(value);
}

function formatTime(value) {
  return timeFormatter.format(new Date(value));
}

function formatReminderTime(value) {
  return `Reminder ${reminderFormatter.format(new Date(value))}`;
}

function toDatetimeLocalValue(value) {
  if (!value) {
    return "";
  }

  const date = new Date(value);
  const offset = date.getTimezoneOffset() * 60000;
  return new Date(date.getTime() - offset).toISOString().slice(0, 16);
}

function fromDatetimeLocalValue(value) {
  if (!value) {
    return null;
  }

  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? null : date.toISOString();
}

function autoGrowTextarea(textarea, maxHeight = 180) {
  textarea.style.height = "auto";
  textarea.style.height = `${Math.min(textarea.scrollHeight, maxHeight)}px`;
}

function autoGrowInput() {
  autoGrowTextarea(noteInput, 180);
}

function getDraft() {
  return localStorage.getItem(draftKey) || "";
}

function saveDraft() {
  localStorage.setItem(draftKey, noteInput.value);
}

function clearDraft() {
  localStorage.removeItem(draftKey);
}

function getVisibleNotes() {
  const query = searchInput.value.trim().toLowerCase();

  return notes.filter((note) => {
    const matchesSearch = !query || note.text.toLowerCase().includes(query);
    const matchesFavorite = currentView !== "favorites" || note.favorite;
    return matchesSearch && matchesFavorite;
  });
}

function setComposerDraft({ restoreDraft = true } = {}) {
  noteInput.value = restoreDraft ? getDraft() : "";
  autoGrowInput();
}

function focusComposer() {
  noteInput.focus();
  noteInput.setSelectionRange(noteInput.value.length, noteInput.value.length);
}

function createButton(label, className, title) {
  const button = document.createElement("button");
  button.type = "button";
  button.className = className;
  button.textContent = label;
  button.title = title;
  return button;
}

function createReminderIcon() {
  const icon = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  icon.setAttribute("class", "reminder-icon");
  icon.setAttribute("viewBox", "0 0 24 24");
  icon.setAttribute("aria-hidden", "true");
  icon.setAttribute("focusable", "false");

  const body = document.createElementNS("http://www.w3.org/2000/svg", "path");
  body.setAttribute("d", "M18 16v-5a6 6 0 0 0-12 0v5l-2 2h16l-2-2Z");

  const clapper = document.createElementNS("http://www.w3.org/2000/svg", "path");
  clapper.setAttribute("d", "M9.5 20a2.7 2.7 0 0 0 5 0");

  icon.append(body, clapper);
  return icon;
}

function isTimelineAtBottom(threshold = 80) {
  return timeline.scrollHeight - timeline.clientHeight - timeline.scrollTop <= threshold;
}

function scrollTimelineToBottom({ smooth = false } = {}) {
  timeline.scrollTo({
    top: timeline.scrollHeight,
    behavior: smooth ? "smooth" : "auto",
  });
}

function renderNotes({ scrollMode = "preserve", smooth = false } = {}) {
  const visibleNotes = getVisibleNotes();
  const distanceFromBottom = Math.max(0, timeline.scrollHeight - timeline.clientHeight - timeline.scrollTop);
  const wasAtBottom = isTimelineAtBottom();

  timeline.replaceChildren();
  notesCount.textContent = `${visibleNotes.length} ${getPlural(visibleNotes.length)}`;

  if (visibleNotes.length === 0) {
    const empty = document.createElement("div");
    empty.className = "empty-state";

    if (notes.length === 0) {
      const title = document.createElement("p");
      title.className = "empty-title";
      title.textContent = "No notes yet";

      const subtitle = document.createElement("p");
      subtitle.className = "empty-subtitle";
      subtitle.textContent = "Start typing below";

      empty.append(title, subtitle);
    } else {
      empty.textContent = "Ничего не найдено";
    }

    timeline.append(empty);
    return;
  }

  let currentDate = "";

  visibleNotes.forEach((note) => {
    const dateKey = getDateKey(note.created_at);

    if (dateKey !== currentDate) {
      currentDate = dateKey;
      const divider = document.createElement("div");
      divider.className = "date-divider";
      divider.textContent = formatDayDivider(note.created_at);
      timeline.append(divider);
    }

    const item = document.createElement("article");
    item.className = "note";

    const time = document.createElement("time");
    time.className = "note-time";
    time.dateTime = note.created_at;
    time.textContent = formatTime(note.created_at);

    const body = createNoteBody(note);
    const actions = document.createElement("div");
    actions.className = "note-actions";

    const reminder = createButton("", "icon-button reminder", "Напоминание");
    reminder.setAttribute("aria-label", "Напоминание");
    reminder.append(createReminderIcon());
    reminder.classList.toggle("active", Boolean(note.remind_at));
    reminder.addEventListener("click", (event) => {
      event.preventDefault();
      event.stopPropagation();
      toggleReminderPicker(note);
    });

    const favorite = createButton(note.favorite ? "★" : "☆", "icon-button", "Избранное");
    favorite.setAttribute("aria-label", "Избранное");
    favorite.classList.toggle("active", note.favorite);
    favorite.addEventListener("click", (event) => {
      event.preventDefault();
      event.stopPropagation();
      toggleFavorite(note);
    });

    const edit = createButton("✎", "icon-button", "Редактировать");
    edit.setAttribute("aria-label", "Редактировать");
    edit.addEventListener("click", (event) => {
      event.preventDefault();
      event.stopPropagation();
      startInlineEdit(note);
    });

    const remove = createButton("×", "icon-button danger", "Удалить");
    remove.setAttribute("aria-label", "Удалить");
    remove.addEventListener("click", (event) => {
      event.preventDefault();
      event.stopPropagation();
      removeNote(note);
    });

    actions.append(reminder, favorite, edit, remove);
    item.append(time, body, actions);
    timeline.append(item);
  });

  requestAnimationFrame(() => {
    if (scrollMode === "always") {
      scrollTimelineToBottom({ smooth });
      return;
    }

    if (scrollMode === "if-bottom" && wasAtBottom) {
      scrollTimelineToBottom({ smooth });
      return;
    }

    if (scrollMode === "preserve") {
      timeline.scrollTop = Math.max(0, timeline.scrollHeight - timeline.clientHeight - distanceFromBottom);
    }
  });
}

function createNoteBody(note) {
  const body = document.createElement("div");
  body.className = "note-body";
  body.append(createNoteText(note));

  if (note.remind_at) {
    const reminder = document.createElement("p");
    reminder.className = "note-reminder";
    reminder.textContent = formatReminderTime(note.remind_at);
    body.append(reminder);
  }

  if (reminderEditingId === note.id) {
    body.append(createReminderPicker(note));
  }

  return body;
}

function createNoteText(note) {
  if (inlineEditingId !== note.id) {
    const text = document.createElement("p");
    text.className = "note-text";
    text.textContent = note.text;
    text.addEventListener("dblclick", () => startInlineEdit(note));
    return text;
  }

  const editor = document.createElement("textarea");
  editor.className = "inline-editor";
  editor.value = note.text;
  editor.rows = 1;
  editor.addEventListener("input", () => autoGrowTextarea(editor, 260));
  editor.addEventListener("keydown", (event) => handleInlineEditKeydown(event, note, editor));

  requestAnimationFrame(() => {
    autoGrowTextarea(editor, 260);
    editor.focus();
    editor.setSelectionRange(editor.value.length, editor.value.length);
  });

  return editor;
}

function getReminderPresetIso(preset) {
  const now = new Date();

  if (preset === "10m") {
    return new Date(now.getTime() + 10 * 60 * 1000).toISOString();
  }

  if (preset === "1h") {
    return new Date(now.getTime() + 60 * 60 * 1000).toISOString();
  }

  if (preset === "tomorrow") {
    const tomorrow = new Date(now);
    tomorrow.setDate(tomorrow.getDate() + 1);
    tomorrow.setHours(9, 0, 0, 0);
    return tomorrow.toISOString();
  }

  return null;
}

function createReminderPicker(note) {
  const picker = document.createElement("form");
  picker.className = "reminder-picker";

  const presets = document.createElement("div");
  presets.className = "reminder-presets";

  const customSection = document.createElement("div");
  customSection.className = "reminder-custom";
  customSection.hidden = true;

  const input = document.createElement("input");
  input.className = "reminder-input";
  input.type = "datetime-local";
  input.value = toDatetimeLocalValue(note.remind_at);

  const set = createButton("Set", "reminder-action primary", "Поставить напоминание");
  const clear = createButton("Clear", "reminder-action", "Удалить напоминание");

  const presetOptions = [
    { id: "10m", label: "10 min" },
    { id: "1h", label: "1 hour" },
    { id: "tomorrow", label: "Tomorrow 9:00" },
    { id: "custom", label: "Custom" },
  ];

  picker.addEventListener("click", (event) => event.stopPropagation());
  picker.addEventListener("submit", (event) => {
    event.preventDefault();
    event.stopPropagation();
    setReminder(note, fromDatetimeLocalValue(input.value));
  });

  set.addEventListener("click", (event) => {
    event.preventDefault();
    event.stopPropagation();
    picker.requestSubmit();
  });

  clear.addEventListener("click", (event) => {
    event.preventDefault();
    event.stopPropagation();
    setReminder(note, null);
  });

  presetOptions.forEach(({ id, label }) => {
    const button = createButton(label, "reminder-preset", label);
    button.type = "button";
    button.dataset.preset = id;

    button.addEventListener("click", (event) => {
      event.preventDefault();
      event.stopPropagation();

      if (id === "custom") {
        customSection.hidden = false;
        presets.querySelectorAll(".reminder-preset").forEach((element) => {
          element.classList.toggle("active", element === button);
        });
        requestAnimationFrame(() => input.focus());
        return;
      }

      setReminder(note, getReminderPresetIso(id));
    });

    presets.append(button);
  });

  customSection.append(input, set, clear);
  picker.append(presets, customSection);

  return picker;
}

function getPlural(count) {
  const mod10 = count % 10;
  const mod100 = count % 100;

  if (mod10 === 1 && mod100 !== 11) {
    return "запись";
  }

  if (mod10 >= 2 && mod10 <= 4 && (mod100 < 12 || mod100 > 14)) {
    return "записи";
  }

  return "записей";
}

function getTauriInvoke() {
  return window.__TAURI__?.core?.invoke || null;
}

function applyNoteTextSize(size) {
  const allowed = new Set(["small", "default", "large"]);
  const normalized = allowed.has(size) ? size : "default";
  document.documentElement.dataset.noteTextSize = normalized;
}

async function loadNoteTextSize() {
  if (!getTauriInvoke()) {
    return;
  }

  const settings = await invokeTauri("get_settings");
  applyNoteTextSize(settings.note_text_size || "default");
}

async function setupNoteTextSizeListener() {
  const listen = window.__TAURI__?.event?.listen;

  if (!listen) {
    return;
  }

  await listen("note-text-size-changed", (event) => {
    applyNoteTextSize(event.payload);
  });
}

function normalizeNote(note) {
  return {
    ...note,
    favorite: Boolean(note.favorite),
    remind_at: note.remind_at || null,
  };
}

function upsertNote(note) {
  const index = notes.findIndex((item) => item.id === note.id);

  if (index === -1) {
    notes.push(note);
  } else {
    notes[index] = note;
  }
}

function setBackupStatus(message, { error = false } = {}) {
  if (!backupStatus) {
    return;
  }

  backupStatus.textContent = message;
  backupStatus.classList.toggle("error", error);
}

function setBackupBusy(isBusy) {
  exportNotesButton.disabled = isBusy;
  importNotesButton.disabled = isBusy;
}

async function invokeTauri(command, args = {}) {
  const invoke = getTauriInvoke();

  if (!invoke) {
    throw new Error("Tauri invoke is not available");
  }

  return invoke(command, args);
}

async function getNotes() {
  const result = await invokeTauri("get_notes");
  return result.map(normalizeNote);
}

async function createNote(text) {
  return normalizeNote(await invokeTauri("create_note", { text }));
}

async function updateNote(id, patch) {
  return normalizeNote(await invokeTauri("update_note", {
    id,
    text: Object.prototype.hasOwnProperty.call(patch, "text") ? patch.text : null,
    favorite: Object.prototype.hasOwnProperty.call(patch, "favorite") ? patch.favorite : null,
    remindAt: Object.prototype.hasOwnProperty.call(patch, "remind_at") ? patch.remind_at : null,
    clearReminder: Object.prototype.hasOwnProperty.call(patch, "remind_at") && !patch.remind_at,
  }));
}

async function deleteNote(id) {
  await invokeTauri("delete_note", { id });
}

async function exportNotesBackup() {
  try {
    setBackupBusy(true);
    setBackupStatus("");

    const result = await invokeTauri("export_notes", { path: null });

    if (!result.exported) {
      setBackupStatus("Export cancelled");
      return;
    }

    setBackupStatus(`Exported ${result.count} notes`);
  } catch (error) {
    console.error(error);
    setBackupStatus("Export failed", { error: true });
  } finally {
    setBackupBusy(false);
    requestAnimationFrame(() => focusComposer());
  }
}

async function importNotesBackup() {
  try {
    setBackupBusy(true);
    setBackupStatus("");

    const result = await invokeTauri("import_notes", { path: null });

    if (!result.imported) {
      setBackupStatus("Import cancelled");
      return;
    }

    notes = await getNotes();
    renderNotes({ scrollMode: "always", smooth: true });
    setBackupStatus(`Imported ${result.added}, skipped ${result.skipped}`);
  } catch (error) {
    console.error(error);
    setBackupStatus(error.message || "Import failed", { error: true });
  } finally {
    setBackupBusy(false);
    requestAnimationFrame(() => focusComposer());
  }
}

async function loadNotes() {
  notes = await getNotes();
  renderNotes({ scrollMode: "always" });
  requestAnimationFrame(() => focusComposer());
}

async function saveNote(event) {
  event.preventDefault();

  const text = noteInput.value.trim();
  if (!text) {
    return;
  }

  const created = await createNote(text);

  upsertNote(created);
  clearDraft();
  setComposerDraft({ restoreDraft: false });
  renderNotes({ scrollMode: "always", smooth: true });
  requestAnimationFrame(() => focusComposer());
}

async function toggleFavorite(note) {
  const updated = await updateNote(note.id, { favorite: !note.favorite });

  notes = notes.map((item) => item.id === updated.id ? updated : item);
  renderNotes({ scrollMode: "if-bottom" });
}

function startInlineEdit(note) {
  inlineEditingId = note.id;
  reminderEditingId = null;
  renderNotes({ scrollMode: "preserve" });
}

function cancelInlineEdit() {
  inlineEditingId = null;
  renderNotes({ scrollMode: "preserve" });
  requestAnimationFrame(() => focusComposer());
}

async function saveInlineEdit(note, textarea) {
  const text = textarea.value.trim();

  if (!text) {
    return;
  }

  if (text === note.text) {
    cancelInlineEdit();
    return;
  }

  const updated = await updateNote(note.id, { text });

  notes = notes.map((item) => item.id === updated.id ? updated : item);
  inlineEditingId = null;
  renderNotes({ scrollMode: "if-bottom" });
  requestAnimationFrame(() => focusComposer());
}

function toggleReminderPicker(note) {
  reminderEditingId = reminderEditingId === note.id ? null : note.id;
  inlineEditingId = null;
  renderNotes({ scrollMode: "preserve" });
}

async function setReminder(note, remindAt) {
  const updated = await updateNote(note.id, { remind_at: remindAt });

  notes = notes.map((item) => item.id === updated.id ? updated : item);
  reminderEditingId = null;
  renderNotes({ scrollMode: "if-bottom" });
  requestAnimationFrame(() => focusComposer());
}

function handleInlineEditKeydown(event, note, textarea) {
  const shouldSave =
    event.key === "Enter" && (event.metaKey || event.ctrlKey || !event.shiftKey);

  if (shouldSave) {
    event.preventDefault();
    event.stopPropagation();
    saveInlineEdit(note, textarea);
    return;
  }

  if (event.key === "Escape") {
    event.preventDefault();
    event.stopPropagation();
    cancelInlineEdit();
  }
}

async function removeNote(note) {
  try {
    await deleteNote(note.id);
    notes = notes.filter((item) => item.id !== note.id);

    if (inlineEditingId === note.id) {
      inlineEditingId = null;
    }

    if (reminderEditingId === note.id) {
      reminderEditingId = null;
    }

    renderNotes({ scrollMode: "if-bottom" });
    requestAnimationFrame(() => focusComposer());
  } catch (error) {
    console.error(error);
  }
}

noteForm.addEventListener("submit", saveNote);
searchInput.addEventListener("input", () => renderNotes({ scrollMode: "if-bottom" }));
exportNotesButton.addEventListener("click", exportNotesBackup);
importNotesButton.addEventListener("click", importNotesBackup);
sidebarToggle.addEventListener("click", () => {
  const isOpen = document.body.classList.toggle("sidebar-open");
  sidebarToggle.setAttribute("aria-expanded", String(isOpen));
});

navButtons.forEach((button) => {
  button.addEventListener("click", () => {
    currentView = button.dataset.view;
    navButtons.forEach((item) => item.classList.toggle("active", item === button));
    document.body.classList.remove("sidebar-open");
    sidebarToggle.setAttribute("aria-expanded", "false");
    renderNotes({ scrollMode: "if-bottom" });
  });
});

noteInput.addEventListener("input", () => {
  autoGrowInput();
  saveDraft();
});

noteInput.addEventListener("keydown", (event) => {
  const shouldSave =
    event.key === "Enter" && (event.metaKey || event.ctrlKey || !event.shiftKey);

  if (shouldSave) {
    event.preventDefault();
    noteForm.requestSubmit();
  }
});

document.addEventListener("keydown", (event) => {
  const key = event.key.toLowerCase();

  if ((event.metaKey || event.ctrlKey) && key === "n") {
    event.preventDefault();
    focusComposer();
    return;
  }

  if ((event.metaKey || event.ctrlKey) && (key === "k" || key === "f")) {
    event.preventDefault();
    searchInput.focus();
    searchInput.select();
    return;
  }

  if ((event.metaKey || event.ctrlKey) && event.key === "Enter" && event.target !== noteInput) {
    event.preventDefault();
    focusComposer();
    noteForm.requestSubmit();
    return;
  }

  if (event.key === "Escape") {
    if (document.body.classList.contains("sidebar-open")) {
      document.body.classList.remove("sidebar-open");
      sidebarToggle.setAttribute("aria-expanded", "false");
      return;
    }

    if (searchInput.value) {
      searchInput.value = "";
      renderNotes({ scrollMode: "if-bottom" });
      return;
    }

    if (document.activeElement instanceof HTMLElement) {
      document.activeElement.blur();
    }
  }
});

setComposerDraft();
loadNotes().catch((error) => {
  timeline.textContent = "Не удалось загрузить заметки";
  console.error(error);
});

if (openSettingsButton) {
  openSettingsButton.addEventListener("click", () => {
    invokeTauri("show_settings").catch((error) => {
      console.error(error);
    });
  });
}

loadNoteTextSize().catch((error) => {
  console.error(error);
});

setupNotesChangedListener().catch((error) => {
  console.error(error);
});

setupNoteTextSizeListener().catch((error) => {
  console.error(error);
});

async function setupNotesChangedListener() {
  const listen = window.__TAURI__?.event?.listen;

  if (!listen) {
    return;
  }

  await listen("notes-changed", (event) => {
    const note = normalizeNote(event.payload);
    upsertNote(note);

    renderNotes({ scrollMode: "if-bottom", smooth: true });
  });
}
