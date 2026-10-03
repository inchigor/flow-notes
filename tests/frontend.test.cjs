const test = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");

class Element {
  constructor() {
    this.value = "";
    this.style = {};
    this.dataset = {};
    this.listeners = {};
    this.children = [];
    this.attributes = new Map();
    this.className = "";
    this.scrollHeight = 32;
    this.clientHeight = 32;
    this.scrollTop = 0;
    this.classList = {
      toggle: (name, force) => {
        const classes = new Set(this.className.split(/\s+/u).filter(Boolean));
        const enabled = force === undefined ? !classes.has(name) : Boolean(force);
        if (enabled) classes.add(name);
        else classes.delete(name);
        this.className = [...classes].join(" ");
        return enabled;
      },
      remove: (name) => this.classList.toggle(name, false),
      contains: (name) => this.className.split(/\s+/u).includes(name),
    };
  }
  addEventListener(name, callback) { this.listeners[name] = callback; }
  append(...children) { this.children.push(...children); }
  replaceChildren(...children) { this.children = children; }
  setAttribute(name, value) { this.attributes.set(name, String(value)); }
  removeAttribute(name) { this.attributes.delete(name); }
  focus() {}
  setSelectionRange() {}
  scrollTo() {}
  requestSubmit() { return this.listeners.submit({ preventDefault() {} }); }
}

async function load(name) {
  const elements = new Map();
  const calls = [];
  let rejectSave = false;
  const document = {
    querySelector(selector) {
      if (!elements.has(selector)) elements.set(selector, new Element());
      return elements.get(selector);
    },
    querySelectorAll() { return []; },
    createElement() { return new Element(); },
    createElementNS() { return new Element(); },
    documentElement: new Element(),
    body: new Element(),
    addEventListener() {},
  };
  const window = { __TAURI__: {
    event: { listen: async () => () => {} },
    core: { invoke: async (command, args) => {
      calls.push({ command, args });
      if (command === "get_settings") return {};
      if (command === "get_notes") return [];
      if (command === "create_note") {
        if (rejectSave) throw new Error("Test save failure");
        const now = new Date().toISOString();
        return { id: "test", text: args.text, created_at: now, updated_at: now };
      }
    } },
  } };
  const context = vm.createContext({
    document, window, HTMLElement: Element,
    console: { error() {} },
    localStorage: { getItem: () => "", setItem() {}, removeItem() {} },
    requestAnimationFrame: (callback) => callback(),
  });
  const source = path.join(__dirname, "..", "public", `${name}.js`);
  vm.runInContext(fs.readFileSync(source, "utf8"), context, { filename: source });
  await new Promise((resolve) => setImmediate(resolve));
  return {
    elements,
    calls,
    rejectSave() { rejectSave = true; },
    renderNotes(sampleNotes) {
      vm.runInContext(`notes = ${JSON.stringify(sampleNotes)}.map(normalizeNote); renderNotes();`, context);
    },
  };
}

for (const name of ["app", "quick-capture"]) {
  test(`${name}: word count and save availability`, async () => {
    const { elements } = await load(name);
    const input = elements.get(name === "app" ? "#noteInput" : "#captureInput");
    const count = elements.get(name === "app" ? "#composerWordCount" : "#captureWordCount");
    const save = elements.get(name === "app" ? "#saveNote" : "#saveCapture");
    assert.equal(count.textContent, "0 words");
    assert.equal(save.disabled, true);
    input.value = "one";
    input.listeners.input();
    assert.equal(count.textContent, "1 word");
    assert.equal(save.disabled, false);
    input.value = "  one\n two\tthree  ";
    input.listeners.input();
    assert.equal(count.textContent, "3 words");
    input.value = " \n\t ";
    input.listeners.input();
    assert.equal(count.textContent, "0 words");
    assert.equal(save.disabled, true);
  });
}

test("Quick Capture: save button submits once, clears draft and counter", async () => {
  const { elements, calls } = await load("quick-capture");
  const input = elements.get("#captureInput");
  input.value = "A short note";
  input.listeners.input();
  await elements.get("#captureForm").requestSubmit();
  assert.equal(calls.filter((call) => call.command === "create_note").length, 1);
  assert.equal(input.value, "");
  assert.equal(elements.get("#captureWordCount").textContent, "0 words");
  assert.equal(elements.get("#saveCapture").disabled, true);
  assert.ok(calls.some((call) => call.command === "hide_quick_capture"));
});

test("Quick Capture: dismiss preserves draft and failed save can be retried", async () => {
  const harness = await load("quick-capture");
  const input = harness.elements.get("#captureInput");
  input.value = "Keep this draft";
  input.listeners.input();
  await harness.elements.get("#dismissCapture").listeners.click();
  assert.equal(input.value, "Keep this draft");
  harness.rejectSave();
  await harness.elements.get("#captureForm").requestSubmit();
  assert.equal(input.value, "Keep this draft");
  assert.equal(harness.elements.get("#saveCapture").disabled, false);
});

test("Quick Capture: only the non-button header starts window dragging", async () => {
  const { elements, calls } = await load("quick-capture");
  const header = elements.get("#captureHeader");
  header.listeners.mousedown({ button: 0, target: { closest: () => null }, preventDefault() {} });
  header.listeners.mousedown({ button: 0, target: { closest: () => ({}) }, preventDefault() {} });
  header.listeners.mousedown({ button: 2, target: { closest: () => null }, preventDefault() {} });
  assert.equal(calls.filter((call) => call.command === "start_quick_capture_drag").length, 1);
});

test("Favorite button: state and action label distinguish adding from removing", async () => {
  const harness = await load("app");
  const now = new Date().toISOString();
  for (const favorite of [false, true]) {
    harness.renderNotes([{ id: "sample", text: "A test note", created_at: now, favorite }]);
    const note = harness.elements.get("#timeline").children.find((item) => item.className === "note");
    const actions = note.children.find((item) => item.className === "note-actions");
    const button = actions.children.find((item) => item.classList.contains("favorite"));
    const title = favorite ? "Remove from favorites" : "Add to favorites";
    assert.equal(button.title, title);
    assert.equal(button.attributes.get("aria-label"), title);
    assert.equal(button.attributes.get("aria-pressed"), String(favorite));
    assert.equal(button.classList.contains("active"), favorite);
    assert.equal(harness.elements.get("#favoritesCount").textContent, favorite ? 1 : 0);
  }
});

test("Stylesheet: note actions hide by default and reveal on hover, focus, or touch", () => {
  const style = fs.readFileSync(path.join(__dirname, "..", "public", "style.css"), "utf8");
  assert.match(style, /\.note-actions\s*\{[^}]*opacity:\s*0;[^}]*pointer-events:\s*none;/u);
  assert.match(style, /\.note:hover \.note-actions,\s*\.note:focus-within \.note-actions\s*\{[^}]*opacity:\s*1;[^}]*pointer-events:\s*auto;/u);
  assert.match(style, /@media \(hover: none\), \(pointer: coarse\)\s*\{\s*\.note-actions\s*\{[^}]*opacity:\s*1;[^}]*pointer-events:\s*auto;/u);
  assert.doesNotMatch(style, /\.note-actions\s*\{[^}]*opacity:\s*0\.8;/u);
});

test("Stylesheet: selected favorite uses a solid fill", () => {
  const style = fs.readFileSync(path.join(__dirname, "..", "public", "style.css"), "utf8");
  assert.match(style, /\.icon-button\.favorite\.active \.ui-icon\s*\{\s*fill:\s*currentColor;/u);
});
