// The words of the panel (status-panel.md, "Panel layout"): activity words and groups, the git
// text, the agent lines and the ages. Pure functions of the document and the clock.
import type { Actor, Git, Worktree } from '../generated/status';
import { visible } from '../shared/invisible';

type Activity = Worktree['activity'];
type Reason = NonNullable<Actor['reason']>;

/** The word of each activity value (U1, legend). */
export const WORDS: Readonly<Record<Activity, string>> = {
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
export const GROUPS: Readonly<Record<Activity, 'attention' | 'active' | 'quiet' | 'problem'>> = {
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

/** The words of a waiting reason on an agent line. */
const REASONS: Readonly<Record<Reason, string>> = {
  permission: 'permission',
  question: 'question',
  plan: 'plan approval',
  input: 'input',
};

/** The words of a waiting reason in the detail strip, after "waiting for". */
const WAITING_FOR: Readonly<Record<Reason, string>> = {
  permission: 'permission',
  question: 'an answer',
  plan: 'plan approval',
  input: 'input',
};

/** The separator between the parts of a line. */
export const DOT = ' · ';

/** The order of the row roles (U2). */
const ROLE_ORDER: Readonly<Record<Worktree['role'], number>> = { foreground: 0, slot: 1, unmanaged: 2 };

/** Gives U1's age of an RFC 3339 time at `nowMs`: `now`, `N s`, `N m`, `N h`, `N d`, or empty. */
export function age(time: string | null, nowMs: number): string {
  if (time === null) return '';
  const seconds = Math.floor((nowMs - Date.parse(time)) / 1000);
  if (seconds < 5) return 'now';
  if (seconds < 60) return `${seconds} s`;
  if (seconds < 3600) return `${Math.floor(seconds / 60)} m`;
  if (seconds < 48 * 3600) return `${Math.floor(seconds / 3600)} h`;
  return `${Math.floor(seconds / 86400)} d`;
}

/** Pads a clock field to two digits. */
function twoDigits(value: number): string {
  return String(value).padStart(2, '0');
}

/** Gives the local `HH:MM` of an RFC 3339 time. */
export function clock(time: string): string {
  const date = new Date(time);
  return `${twoDigits(date.getHours())}:${twoDigits(date.getMinutes())}`;
}

/** Gives the Git column: `clean`, else the facts joined by spaces (U1). */
export function gitText(git: Git): string {
  const parts: string[] = [];
  if (git.dirty) parts.push(`${git.dirty} dirty`);
  if (git.ahead) parts.push(`↑${git.ahead}`);
  if (git.behind) parts.push(`↓${git.behind}`);
  if (git.unreferenced) parts.push(`${git.unreferenced} unreferenced`);
  if (git.operation) parts.push(git.operation);
  if (git.unknown) parts.push('unknown');
  return parts.length ? parts.join(' ') : 'clean';
}

/** Gives the git facts of the detail strip, every known count spelt out (U5). */
export function gitFacts(git: Git): string {
  const parts: string[] = [];
  if (git.dirty !== null) parts.push(`${git.dirty} dirty`);
  if (git.ahead !== null) parts.push(`${git.ahead} ahead`);
  if (git.behind !== null) parts.push(`${git.behind} behind`);
  if (git.unreferenced) parts.push(`${git.unreferenced} unreferenced`);
  if (git.operation) parts.push(git.operation);
  if (git.unknown) parts.push('unknown');
  return parts.join(DOT);
}

/**
 * Holds one piece of an agent line: plain text the panel writes, or a string from the document,
 * isolated by the DOM.
 */
export interface Piece {
  text: string;
  from?: 'document';
}

/** Makes a piece of the panel's own words. */
function own(text: string): Piece {
  return { text };
}

/** Makes a piece of a string from the document, as X10 shows it. */
function theirs(text: string): Piece {
  return { text: visible(text), from: 'document' };
}

/** Gives the pieces of an actor's action: the tool, then its target if it has one. */
function actionPieces(actor: Actor): Piece[] {
  if (!actor.action) return [];
  const tool = theirs(actor.action.tool);
  return actor.action.target ? [tool, own(' '), theirs(actor.action.target)] : [tool];
}

/** Gives the pieces after `<label> · ` on an actor line (actor state table). */
function doing(actor: Actor): Piece[] {
  const action = actionPieces(actor);
  switch (actor.state) {
    case 'working':
      return action.length ? action : [own('working')];
    case 'quiet':
      return action.length ? [own(`quiet${DOT}`), ...action] : [own('quiet')];
    case 'waiting': {
      const reason = own(actor.reason === null ? '' : REASONS[actor.reason]);
      return action.length ? [reason, own(': '), ...action] : [reason];
    }
    case 'idle':
      return actor.error ? [own('API error: '), theirs(actor.error)] : [own('idle')];
    default:
      return [own('done')];
  }
}

/** Gives the actors of a row, newest `updated` first (R4). */
export function shownActors(worktree: Worktree): Actor[] {
  return [...worktree.actors].sort((a, b) => Date.parse(b.updated) - Date.parse(a.updated));
}

/** Gives the lines of the Agent · doing column, each a list of pieces. */
export function agentLines(worktree: Worktree): Piece[][] {
  if (worktree.actors.length) {
    return shownActors(worktree).map((actor) => [theirs(actor.label), own(DOT), ...doing(actor)]);
  }
  if (worktree.builds.length) return [[own('build: '), theirs(worktree.builds[0].name)]];
  return [[own('no agent')]];
}

/** Gives the times whose ages the Last column shows, one per agent line (A6). */
export function lastTimes(worktree: Worktree): Array<string | null> {
  if (worktree.actors.length) return shownActors(worktree).map((actor) => actor.updated);
  return [worktree.last_event ?? worktree.last_change];
}

/** Gives an actor's state in the detail strip. */
export function stateText(actor: Actor): Piece[] {
  if (actor.state === 'waiting') return [own(`waiting for ${actor.reason === null ? 'you' : WAITING_FOR[actor.reason]}`)];
  if (actor.state === 'idle' && actor.error) return [own(`idle${DOT}API error: `), theirs(actor.error)];
  return [own(actor.state)];
}

/** Gives an actor's second line in the detail strip: action, placement and local start time. */
export function actorFacts(actor: Actor): Piece[] {
  const action = actionPieces(actor);
  return [
    ...action,
    own(`${action.length ? DOT : ''}placed by ${actor.placed_by}${DOT}since ${clock(actor.since)}`),
  ];
}

/** Gives the text of the live region for worktrees that started to wait. */
export function waitAnnouncement(worktrees: Worktree[]): string {
  return worktrees.map((w) => `${visible(w.name)} is waiting`).join(', ');
}

/** Compares two names by code unit, the order U2 sorts by. */
function byName(a: string, b: string): number {
  if (a < b) return -1;
  return a > b ? 1 : 0;
}

/** Orders rows as U2 does: foreground, slots, unmanaged, each by name. */
export function rowOrder(worktrees: Worktree[]): Worktree[] {
  return [...worktrees].sort((a, b) => ROLE_ORDER[a.role] - ROLE_ORDER[b.role] || byName(a.name, b.name));
}
