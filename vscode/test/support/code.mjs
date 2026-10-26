// What every VS Code the oracle starts shares: the flags that keep it off the owner's keychain and
// away from system dialogs, short socket paths, the scan for keychain errors, and the machine
// config of its sandbox home.
import { createHash } from 'node:crypto';
import { chmodSync, lstatSync, mkdirSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import path from 'node:path';

import { cacheRoot } from './vscode-app.mjs';

/**
 * Flags for every VS Code launch. `--password-store=basic` and `--use-mock-keychain` keep secret
 * storage off the macOS keychain: a sandbox HOME has no keychain, and macOS would ask on screen
 * to create one. The rest skip first run pages, trust prompts, telemetry and updates.
 */
export const CODE_FLAGS = [
  '--password-store=basic',
  '--use-mock-keychain',
  '--disable-workspace-trust',
  '--skip-welcome',
  '--skip-release-notes',
  '--disable-telemetry',
  '--disable-updates',
];

/**
 * VS Code's accessibility signals (1.143): most play their sound only while a screen reader runs,
 * some always, so a run would sound while the owner uses VoiceOver, or at all.
 */
const SIGNALS = [
  'chatEditModifiedFile', 'chatRequestSent', 'chatResponsePending', 'chatResponseReceived',
  'chatUserActionRequired', 'clear', 'codeActionApplied', 'codeActionTriggered', 'confetti',
  'diffLineDeleted', 'diffLineInserted', 'diffLineModified', 'editsKept', 'editsUndone', 'format',
  'lineHasBreakpoint', 'lineHasError', 'lineHasFoldedArea', 'lineHasInlineSuggestion',
  'lineHasWarning', 'nextEditSuggestion', 'noInlayHints', 'notebookCellCompleted',
  'notebookCellFailed', 'onDebugBreak', 'positionHasError', 'positionHasWarning', 'progress',
  'save', 'taskCompleted', 'taskFailed', 'terminalBell', 'terminalCommandFailed',
  'terminalCommandSucceeded', 'terminalQuickFix', 'voiceModeStarted', 'voiceModeStopped',
  'voiceRecordingStarted', 'voiceRecordingStopped',
];

/** Settings that keep a sandbox VS Code silent: every signal's sound off, and the volume at 0 for any later one. */
const SILENT = {
  'accessibility.signalOptions.volume': 0,
  ...Object.fromEntries(SIGNALS.map((signal) => [`accessibility.signals.${signal}`, { sound: 'off' }])),
  'terminal.integrated.enableVisualBell': false,
};

/** User settings that keep a sandbox VS Code from prompting, updating, reaching out, sounding or raising system dialogs. */
export const QUIET_SETTINGS = {
  ...SILENT,
  'workbench.startupEditor': 'none',
  'security.workspace.trust.enabled': false,
  'telemetry.telemetryLevel': 'off',
  'update.mode': 'none',
  'extensions.autoUpdate': false,
  'extensions.autoCheckUpdates': false,
  'extensions.ignoreRecommendations': true,
  'workbench.enableExperiments': false,
  'workbench.tips.enabled': false,
  'git.enabled': false,
  'npm.autoDetect': 'off',
  'chat.disableAIFeatures': true,
  'workbench.secondarySideBar.defaultVisibility': 'hidden',
  'window.commandCenter': false,
  'window.confirmBeforeClose': 'never',
  'terminal.integrated.enablePersistentSessions': false,
  'terminal.integrated.confirmOnExit': 'never',
  'terminal.integrated.confirmOnKill': 'never',
  // A modal (X12) draws inside the window, never as a macOS system dialog.
  'window.dialogStyle': 'custom',
};

/**
 * The folder of the short links, inside ~/.cache/wtpool-verify/ (`XDG_CACHE_HOME` when set), an
 * outside path the run may use.
 */
const LINKS = path.join(cacheRoot(), 's');

/**
 * A short path for `dir`. macOS limits a Unix socket path to 103 bytes, and VS Code puts its
 * main socket in the user data folder and the extension host's in TMPDIR, while a run folder
 * path is far longer. The link points into the run folder, so the files stay there.
 */
export function shortLink(dir) {
  mkdirSync(dir, { recursive: true });
  mkdirSync(LINKS, { recursive: true });
  const link = path.join(LINKS, createHash('sha256').update(dir).digest('hex').slice(0, 8));
  removeLink(link);
  symlinkSync(dir, link);
  return link;
}

/**
 * Removes what sits at `link`: the link itself, never its target, or the real folder a killed
 * run left in its place, so the next run can make the link again.
 */
export function removeLink(link) {
  // lstat describes the link, not its target, so only a real folder goes with its files.
  const folder = lstatSync(link, { throwIfNoEntry: false })?.isDirectory() ?? false;
  rmSync(link, { recursive: folder, force: true });
}

/** The lines of VS Code output that report a keychain or secret storage error. */
export function keychainErrors(text) {
  return String(text)
    .split('\n')
    .filter((line) => /keychain|OSCrypt|safeStorage/i.test(line))
    .filter((line) => !/'use-mock-keychain' is not in the list of known options/.test(line));
}

/**
 * The machine config every sandbox home starts with: processes named `cargo`, `cargo-*` or
 * `rustc`, the builds of phases 1 to 3, as heavy patterns of the machine. From phase 4 only a
 * process that matches a heavy pattern is a build, so the checks of the earlier phases keep their
 * builds through this list, and a wtpool before phase 4 reads no such file. A check of heavy jobs
 * writes its own.
 */
export const EARLIER_BUILDS = '# The builds of phases 1 to 3 as heavy patterns.\n[jobs]\nheavy = ["cargo", "cargo-*", "rustc"]\n';

/** Writes {@link EARLIER_BUILDS} as the `machine.toml` of `home`, folder 0700 and file 0600. */
export function writeEarlierBuilds(home) {
  const folder = path.join(home, '.config', 'wtpool');
  mkdirSync(folder, { recursive: true });
  chmodSync(folder, 0o700);
  const file = path.join(folder, 'machine.toml');
  writeFileSync(file, EARLIER_BUILDS, { mode: 0o600 });
  chmodSync(file, 0o600);
}
