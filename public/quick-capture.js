const captureForm = document.querySelector("#captureForm");
const captureInput = document.querySelector("#captureInput");
const captureHeader = document.querySelector("#captureHeader");
const dismissCaptureButton = document.querySelector("#dismissCapture");
const captureWordCount = document.querySelector("#captureWordCount");
const saveCaptureButton = document.querySelector("#saveCapture");

let isSaving = false;

function updateCaptureControls() {
  const text = captureInput.value.trim();
  const count = text ? text.split(/\s+/u).length : 0;
  captureWordCount.textContent = `${count} ${count === 1 ? "word" : "words"}`;
  saveCaptureButton.disabled = isSaving || !text;
}

function getTauriInvoke() {
  return window.__TAURI__?.core?.invoke || null;
}

function getTauriListen() {
  return window.__TAURI__?.event?.listen || null;
}

async function invokeTauri(command, args = {}) {
  const invoke = getTauriInvoke();

  if (!invoke) {
    throw new Error("Tauri invoke is not available");
  }

  return invoke(command, args);
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
  const listen = getTauriListen();

  if (!listen) {
    return;
  }

  await listen("note-text-size-changed", (event) => {
    applyNoteTextSize(event.payload);
  });
}

function focusCaptureInput({ clear = false } = {}) {
  if (clear) {
    captureInput.value = "";
  }

  captureInput.focus();
  updateCaptureControls();
  const cursor = captureInput.value.length;
  captureInput.setSelectionRange(cursor, cursor);
}

async function hideCaptureWindow({ clearDraft = false } = {}) {
  if (clearDraft) {
    captureInput.value = "";
    updateCaptureControls();
  }

  try {
    await invokeTauri("hide_quick_capture");
  } catch (error) {
    console.error(error);
  }
}

async function saveCapture(event) {
  event.preventDefault();

  if (isSaving) {
    return;
  }

  const text = captureInput.value.trim();

  if (!text) {
    await hideCaptureWindow({ clearDraft: true });
    return;
  }

  isSaving = true;
  updateCaptureControls();

  try {
    await invokeTauri("create_note", { text });
    await hideCaptureWindow({ clearDraft: true });
  } catch (error) {
    console.error(error);
  } finally {
    isSaving = false;
    updateCaptureControls();
  }
}

captureForm.addEventListener("submit", saveCapture);
captureInput.addEventListener("input", updateCaptureControls);

captureHeader.addEventListener("mousedown", (event) => {
  if (event.button !== 0 || event.target.closest("button")) {
    return;
  }

  event.preventDefault();
  invokeTauri("start_quick_capture_drag").catch((error) => {
    console.error(error);
  });
});

dismissCaptureButton.addEventListener("click", () => {
  hideCaptureWindow({ clearDraft: false });
});

captureInput.addEventListener("keydown", (event) => {
  if (event.key === "Escape") {
    event.preventDefault();
    hideCaptureWindow({ clearDraft: true });
    return;
  }

  if (event.key === "Enter" && !event.shiftKey) {
    event.preventDefault();
    captureForm.requestSubmit();
  }
});

document.addEventListener("keydown", (event) => {
  if (event.key === "Escape") {
    event.preventDefault();
    hideCaptureWindow({ clearDraft: true });
  }
});

async function setupFocusListener() {
  const listen = getTauriListen();

  if (!listen) {
    focusCaptureInput();
    return;
  }

  await listen("quick-capture-focus", () => {
    focusCaptureInput({ clear: false });
  });

  focusCaptureInput({ clear: false });
}

loadNoteTextSize().catch((error) => {
  console.error(error);
});

setupNoteTextSizeListener().catch((error) => {
  console.error(error);
});

setupFocusListener().catch((error) => {
  console.error(error);
  focusCaptureInput({ clear: false });
});
