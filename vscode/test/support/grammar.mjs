// The oracle's model of the panel text, written from status-panel.md "Panel layout" (U1, U5,
// U6) and the normative parts of the mockups. Tests compare the product's DOM with it.

/** The word of each activity value (U1, legend). */
export const WORDS = {
  foreground: 'foreground',
  waiting: 'waiting',
  working: 'working',
  building: 'building',
  idle: 'idle',
  free: 'free',
  not_reusable: 'not reusable',
  unused: 'unused',
  unknown: 'unknown',
  missing: 'missing',
};

/** The colour group of each activity value (U6). */
export const GROUPS = {
  foreground: 'active',
  waiting: 'attention',
  working: 'active',
  building: 'active',
  idle: 'quiet',
  free: 'quiet',
  not_reusable: 'problem',
  unused: 'quiet',
  unknown: 'problem',
  missing: 'problem',
};

/** The one token of each group (U6, status-panel.md revision 14). */
export const GROUP_TOKENS = {
  attention: '--vscode-list-warningForeground',
  active: '--vscode-textLink-foreground',
  quiet: '--vscode-descriptionForeground',
  problem: '--vscode-list-errorForeground',
};

/** How a waiting reason reads (actor state table). */
export const REASONS = {
  permission: 'permission',
  question: 'question',
  plan: 'plan approval',
  input: 'input',
};

export const DOT = ' · ';

const INVISIBLE =
  /[\u0000-\u001f\u007f-\u009f؜​-‏‪-‮⁠⁦-⁩﻿]/gu;

/** A string as X10 shows it: control, bidi and zero width characters as `\u{XXXX}`. */
export function visible(text) {
  return String(text).replace(
    INVISIBLE,
    (c) => `\\u{${c.codePointAt(0).toString(16).toUpperCase().padStart(4, '0')}}`,
  );
}

/** Normalises the hex digits of `\u{...}` escapes, so case and padding do not matter. */
export function normalizeEscapes(text) {
  return text.replace(
    /\\u\{([0-9a-fA-F]+)\}/g,
    (_, hex) => `\\u{${hex.toUpperCase().replace(/^0+(?=.)/, '').padStart(4, '0')}}`,
  );
}

/** U1's age of an RFC 3339 time at `nowMs`: `now`, `N s`, `N m`, `N h`, `N d`, or empty. */
export function age(time, nowMs) {
  if (time === null || time === undefined) return '';
  const seconds = Math.floor((nowMs - Date.parse(time)) / 1000);
  if (seconds < 5) return 'now';
  if (seconds < 60) return `${seconds} s`;
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes} m`;
  const hours = Math.floor(seconds / 3600);
  if (hours < 48) return `${hours} h`;
  return `${Math.floor(seconds / 86400)} d`;
}

export function worktreeText(worktree) {
  const name = visible(worktree.name);
  if (worktree.role === 'foreground') return `${name} fg`;
  if (worktree.role === 'unmanaged') return `${name} unmanaged`;
  return name;
}

export function branchText(worktree) {
  return worktree.branch === null ? '(detached)' : visible(worktree.branch);
}

export function gitText(git) {
  const parts = [];
  if (git.dirty) parts.push(`${git.dirty} dirty`);
  if (git.ahead) parts.push(`↑${git.ahead}`);
  if (git.behind) parts.push(`↓${git.behind}`);
  if (git.unreferenced) parts.push(`${git.unreferenced} unreferenced`);
  if (git.operation) parts.push(git.operation);
  if (git.unknown) parts.push('unknown');
  return parts.length ? parts.join(' ') : 'clean';
}

function actionText(action) {
  if (!action) return null;
  return action.target ? `${visible(action.tool)} ${visible(action.target)}` : visible(action.tool);
}

/** What follows `<label> · ` on an actor line. */
export function doing(actor) {
  const action = actionText(actor.action);
  switch (actor.state) {
    case 'working':
      return action ?? 'working';
    case 'quiet':
      return action ? `quiet${DOT}${action}` : 'quiet';
    case 'waiting': {
      const reason = REASONS[actor.reason] ?? visible(actor.reason ?? '');
      return action ? `${reason}: ${action}` : reason;
    }
    case 'idle':
      return actor.error ? `API error: ${visible(actor.error)}` : 'idle';
    case 'done':
      return 'done';
    default:
      throw new Error(`setup: unknown actor state ${actor.state}`);
  }
}

/** The actors of a row in the order the panel lists them: newest `updated` first (R4). */
export function shownActors(worktree) {
  return [...worktree.actors].sort((a, b) => Date.parse(b.updated) - Date.parse(a.updated));
}

/**
 * The agent lines of a row. With the document's `machine.jobs`, a build outside a job reads
 * ` (not queued)` after its name while `enforce` is not `off`.
 */
export function agentLines(worktree, jobs) {
  if (worktree.actors.length) {
    return shownActors(worktree).map((actor) => `${visible(actor.label)}${DOT}${doing(actor)}`);
  }
  if (worktree.builds.length) {
    const [build] = worktree.builds;
    const flagged = jobs !== undefined && jobs.enforce !== 'off' && build.queued === false;
    return [`build: ${visible(build.name)}${flagged ? ' (not queued)' : ''}`];
  }
  return ['no agent'];
}

export function lastLines(worktree, nowMs) {
  if (worktree.actors.length) {
    return shownActors(worktree).map((actor) => age(actor.updated, nowMs));
  }
  return [age(worktree.last_event ?? worktree.last_change, nowMs)];
}

const ROLE_ORDER = { foreground: 0, slot: 1, unmanaged: 2 };

/** U2: foreground, slots, unmanaged, each by name. */
export function rowOrder(worktrees) {
  return [...worktrees].sort(
    (a, b) =>
      ROLE_ORDER[a.role] - ROLE_ORDER[b.role] ||
      (a.name < b.name ? -1 : a.name > b.name ? 1 : 0),
  );
}

/**
 * U8: a pool's heading, `repository.github`, or `repository.name` when that is null, then
 * ` · ` and the machine name.
 */
export function headingText(pool, machine) {
  const repository = pool.repository.github ?? pool.repository.name;
  return `${visible(repository)}${DOT}${visible(machine.name)}`;
}

/** The text of every cell of a worktree's row. */
export function expectedRow(worktree, nowMs, jobs) {
  return {
    activity: WORDS[worktree.activity],
    worktree: worktreeText(worktree),
    branch: branchText(worktree),
    git: gitText(worktree.git),
    agent: agentLines(worktree, jobs),
    last: lastLines(worktree, nowMs),
  };
}

/** U3: how many worktrees each chip counts. */
export function chipCounts(worktrees) {
  const count = (activity) => worktrees.filter((w) => w.activity === activity).length;
  return { free: count('free'), working: count('working'), waiting: count('waiting') };
}
