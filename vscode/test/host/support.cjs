// Plumbing of the host checks: the extension's verify API, the fake wtpool's configuration and
// log, the steps that show a document in the tab, and wrappers of the VS Code calls the checks
// watch. Requiring this file wraps `createWebviewPanel` and `registerWebviewPanelSerializer`
// before the extension activates.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vscode = require('vscode');

const EXTENSION_ID = 'haiyuechen-nearone.wtpool';
const VIEW_TYPE = 'wtpool.status';
const TEST = path.dirname(__dirname);
const REPO = path.dirname(path.dirname(TEST));
const SCENARIOS = path.join(REPO, 'verify', 'scenarios');
const HOST_DIR = process.env.WTPOOL_VERIFY_HOST_DIR;
const LOG = path.join(HOST_DIR ?? '/nonexistent', 'fake-wtpool.jsonl');
const CONTROL = path.join(HOST_DIR ?? '/nonexistent', 'fake-control.txt');
/** The `machine.jobs.config` of the documents the checks feed. */
const CONFIG = '/Users/verify/.config/wtpool/machine.toml';

const created = [];
const serializers = [];
const originalCreate = vscode.window.createWebviewPanel;
vscode.window.createWebviewPanel = function createWebviewPanel(viewType, title, show, options) {
  const panel = originalCreate.call(this, viewType, title, show, options);
  created.push({ viewType, options: options ?? {}, panel });
  return panel;
};
const originalRegister = vscode.window.registerWebviewPanelSerializer;
vscode.window.registerWebviewPanelSerializer = function registerWebviewPanelSerializer(viewType, serializer) {
  serializers.push({ viewType, serializer });
  return originalRegister.call(this, viewType, serializer);
};

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

/** Waits in real time until `check` returns a truthy value, and gives it, or undefined. */
async function waitFor(check, timeoutMs = 10_000) {
  const deadline = Date.now() + timeoutMs;
  for (;;) {
    const value = await check();
    if (value) return value;
    if (Date.now() >= deadline) return undefined;
    await sleep(50);
  }
}

let api;

/** The object `activate` returns under WTPOOL_VERIFY=1. */
async function verifyApi() {
  if (api) return api;
  const extension = vscode.extensions.getExtension(EXTENSION_ID);
  // The checks hash the manifest, so an extension that does not load or activate is the product's
  // failure, an assertion.
  assert.ok(extension, `the extension ${EXTENSION_ID} is not loaded`);
  try {
    api = await extension.activate();
  } catch (error) {
    assert.fail(`the extension failed to activate: ${error}`);
  }
  assert.ok(api && typeof api === 'object', 'activate returned no verify API under WTPOOL_VERIFY=1');
  for (const name of ['fromWebview', 'toWebview', 'advance']) {
    assert.equal(typeof api[name], 'function', `the verify API has no ${name}()`);
  }
  return api;
}

/** Every message the extension posted to the webview so far. */
async function posted() {
  const messages = (await verifyApi()).toWebview();
  assert.ok(Array.isArray(messages), 'toWebview() returns no array');
  return messages;
}

async function views() {
  return (await posted()).filter((m) => m && m.type === 'view');
}

/** The views among the messages posted after message `from`. */
async function viewsSince(from) {
  return (await posted()).slice(from).filter((m) => m && m.type === 'view');
}

/**
 * The status documents a scenario file expects, `{pool}` and `{common}` replaced by fixed paths.
 * The ESM support/docs.mjs `scenarioDocuments` gives the scenario with them, but the extension host
 * cannot require an ES module.
 */
function expectedDocuments(name) {
  const scenario = JSON.parse(fs.readFileSync(path.join(SCENARIOS, `${name}.json`), 'utf8'));
  const docs = scenario.steps.filter((s) => s.expect).map((s) => s.expect);
  docs.push(scenario.expect);
  return JSON.parse(
    JSON.stringify(docs)
      .replaceAll('{common}', '/Users/verify/pool/fg/.git')
      .replaceAll('{pool}', '/Users/verify/pool'),
  );
}

/** Starts a check from a closed tab, an empty log and this fake configuration. */
async function resetFake(config) {
  await vscode.commands.executeCommand('workbench.action.closeAllEditors');
  await waitFor(() => !openStatusTab(), 5_000);
  await sleep(300);
  fs.writeFileSync(LOG, '');
  fs.writeFileSync(CONTROL, '');
  fs.writeFileSync(
    process.env.WTPOOL_FAKE_CONFIG,
    JSON.stringify({ log: LOG, control: CONTROL, scenarioDir: SCENARIOS, aliveMs: 1000, ...config }),
  );
}

function fakeLog() {
  if (!fs.existsSync(LOG)) return [];
  return fs
    .readFileSync(LOG, 'utf8')
    .split('\n')
    .filter(Boolean)
    .map((line) => JSON.parse(line));
}

function starts() {
  return fakeLog().filter((entry) => entry.event === 'start');
}

/** Tells if the fake with `pid` has ended: an exit or signal logged, or no such process. */
function ended(pid) {
  const logged = fakeLog().some((e) => e.pid === pid && (e.event === 'exit' || e.event === 'signal'));
  if (logged) return true;
  try {
    process.kill(pid, 0);
    return false;
  } catch {
    return true;
  }
}

function control(command) {
  fs.appendFileSync(CONTROL, `${command}\n`);
}

/** Waits until a view of `doc`, by its `generated`, reached the webview after view `from`. */
async function reached(doc, from) {
  const seen = await waitFor(async () => (await views()).slice(from).some((v) => v.status?.generated === doc.generated), 10_000);
  assert.ok(seen, `setup: the document of ${doc.generated} never reached the webview`);
}

/** Shows `doc` in the open tab through the fake's control file. */
async function show(doc) {
  const from = (await views()).length;
  control(`emit ${JSON.stringify(doc)}`);
  await reached(doc, from);
}

/** Opens the tab on `doc` alone, the fake configured with `config`. */
async function showingAlone(doc, config = {}) {
  await resetFake({ documents: [doc], aliveMs: 0, ...config });
  const from = (await views()).length;
  await openStatus();
  await reached(doc, from);
  // The fake follows its control file once it printed its documents.
  await sleep(300);
}

/** Opens the tab on idle_pool, then shows `doc` and waits until the webview got it. */
async function showingAfterIdle(doc, config = {}) {
  await resetFake({ scenario: 'idle_pool', aliveMs: 0, ...config });
  // Earlier checks posted idle_pool too, so only views of this tab count.
  const from = (await views()).length;
  await openStatus();
  const last = expectedDocuments('idle_pool').at(-1);
  const first = await waitFor(async () => (await views()).slice(from).some((v) => JSON.stringify(v.status) === JSON.stringify(last)), 10_000);
  assert.ok(first, 'setup: idle_pool never reached the webview');
  // The fake follows its control file once it printed the scenario.
  await sleep(300);
  await show(doc);
}

function openStatusTab() {
  for (const group of vscode.window.tabGroups.all) {
    for (const tab of group.tabs) {
      if (tab.input instanceof vscode.TabInputWebview && tab.input.viewType.endsWith(VIEW_TYPE)) {
        return { group, tab };
      }
    }
  }
  return undefined;
}

function statusTabs() {
  return vscode.window.tabGroups.all.flatMap((group) =>
    group.tabs.filter((tab) => tab.input instanceof vscode.TabInputWebview && tab.input.viewType.endsWith(VIEW_TYPE)),
  );
}

async function openStatus() {
  await verifyApi();
  await vscode.commands.executeCommand('wtpool.openStatus');
  const tab = await waitFor(openStatusTab, 10_000);
  assert.ok(tab, 'Worktrees: Open Status opened no wtpool.status tab');
  return tab;
}

/**
 * Advances the extension's fake clock by `ms` in steps of `step`, waiting `realMs` of real time
 * between steps so lines of the fake wtpool reach the extension.
 */
async function advance(ms, { step = ms, realMs = 0 } = {}) {
  const verify = await verifyApi();
  for (let done = 0; done < ms; done += step) {
    await verify.advance(Math.min(step, ms - done));
    if (realMs) await sleep(realMs);
  }
}

/**
 * Replaces `object[name]` with a recorder until `restore` runs. The original runs only for the
 * calls `through` accepts.
 */
function wrap(object, name, through = () => false, result = () => undefined) {
  const original = object[name];
  const calls = [];
  object[name] = function wrapped(...args) {
    calls.push(args);
    return through(...args) ? original.apply(this, args) : result(...args);
  };
  return {
    calls,
    restore() {
      object[name] = original;
    },
  };
}

/**
 * Records `clipboard.writeText` until `restore` runs. `vscode.env.clipboard` is a frozen object,
 * so its method cannot be replaced. `vscode.env`, shared by the extension under development and
 * these checks, gets a recording clipboard instead. Nothing reaches the system clipboard.
 */
function wrapClipboard() {
  const original = Object.getOwnPropertyDescriptor(vscode.env, 'clipboard');
  const real = vscode.env.clipboard;
  const calls = [];
  const fake = Object.freeze({
    readText: () => real.readText(),
    writeText: (...args) => {
      calls.push(args);
      return Promise.resolve();
    },
  });
  Object.defineProperty(vscode.env, 'clipboard', { configurable: true, enumerable: true, get: () => fake });
  assert.equal(vscode.env.clipboard, fake, 'setup: vscode.env.clipboard cannot be replaced');
  return {
    calls,
    restore() {
      if (original) Object.defineProperty(vscode.env, 'clipboard', original);
    },
  };
}

module.exports = {
  CONFIG,
  CONTROL,
  LOG,
  REPO,
  VIEW_TYPE,
  advance,
  control,
  created,
  ended,
  expectedDocuments,
  fakeLog,
  openStatus,
  openStatusTab,
  posted,
  resetFake,
  serializers,
  show,
  showingAfterIdle,
  showingAlone,
  sleep,
  starts,
  statusTabs,
  verifyApi,
  views,
  viewsSince,
  waitFor,
  wrap,
  wrapClipboard,
};
