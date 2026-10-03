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
    this.scrollHeight = 32;
    this.clientHeight = 32;
    this.scrollTop = 0;
    this.classList = { toggle() {}, remove() {}, contains() { return false; } };
  }
  addEventListener(name, callback) { this.listeners[name] = callback; }
  append() {}
  replaceChildren() {}
  setAttribute() {}
  removeAttribute() {}
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
  return { elements, calls, rejectSave() { rejectSave = true; } };
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
