import type { Actor, Git, Worktree } from '../generated/status';
import { visible } from '../shared/invisible';
import { isUnqueued } from '../shared/jobs';

type Activity = Worktree['activity'];
type Reason = NonNullable<Actor['reason']>;

/** The word of each activity value. */
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

/** The colour group of each activity value. */
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

/** The order of the row roles. */
const ROLE_ORDER: Readonly<Record<Worktree['role'], number>> = {
  foreground: 0,
  slot: 1,
  unmanaged: 2,
};

/** Gives the age of an RFC 3339 time at `nowMs`, or empty for no time. */
export function age(time: string | null, nowMs: number): string {
  if (time === null) return '';
  const seconds = Math.floor((nowMs - Date.parse(time)) / 1000);
  if (seconds < 5) return 'now';
  if (seconds < 60) return `${seconds} s`;
  if (seconds < 3600) return `${Math.floor(seconds / 60)} m`;
  if (seconds < 48 * 3600) return `${Math.floor(seconds / 3600)} h`;
  return `${Math.floor(seconds / 86400)} d`;
}

/** Gives `count unit`, then ` rest restUnit` unless `rest` is 0. */
function twoUnits(count: number, unit: string, rest: number, restUnit: string): string {
  return rest ? `${count} ${unit} ${rest} ${restUnit}` : `${count} ${unit}`;
}

/** Gives the run time from an RFC 3339 `started` to `nowMs`, `0 s` while `started` is ahead. */
export function runTime(started: string, nowMs: number): string {
  const seconds = Math.max(0, Math.floor((nowMs - Date.parse(started)) / 1000));
  if (seconds < 60) return `${seconds} s`;
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes} m`;
  const hours = Math.floor(minutes / 60);
  if (hours < 48) return twoUnits(hours, 'h', minutes % 60, 'm');
  return twoUnits(Math.floor(hours / 24), 'd', hours % 24, 'h');
}

function twoDigits(value: number): string {
  return String(value).padStart(2, '0');
}

/** Gives the local `HH:MM` of an RFC 3339 time. */
export function clock(time: string): string {
  const date = new Date(time);
  return `${twoDigits(date.getHours())}:${twoDigits(date.getMinutes())}`;
}

/** Gives the Git column: `clean`, else the facts joined by spaces. */
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

/** Gives the git facts of the detail strip, every known count spelt out. */
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
 * Holds one piece of an agent line: the panel's own words, a string from the document, isolated
 * by the DOM, or a run time that the clock keeps current.
 */
export type Piece =
  | {
      kind: 'own';
      text: string;
      /** The class of a span that holds the text, for a part with a colour of its own. */
      className?: string;
    }
  | { kind: 'document'; text: string }
  | {
      kind: 'runTime';
      /** The `started` of the task whose run time the piece shows. */
      since: string;
    };

/** Makes a piece of the panel's own words. */
function own(text: string): Piece {
  return { kind: 'own', text };
}

/** Makes a piece of a string from the document, its invisible characters made visible. */
function theirs(text: string): Piece {
  return { kind: 'document', text: visible(text) };
}

function actionPieces(actor: Actor): Piece[] {
  if (!actor.action) return [];
  const tool = theirs(actor.action.tool);
  return actor.action.target ? [tool, own(' '), theirs(actor.action.target)] : [tool];
}

/** Gives the pieces after the label on an actor line. */
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

/** Gives the run time after a `background:` action, of the task it names: the first listed. */
function runTimePieces(actor: Actor): Piece[] {
  const [task] = actor.outputs ?? [];
  if (actor.action?.tool !== 'background:' || !task) return [];
  return [own(DOT), { kind: 'runTime', since: task.started }];
}

/** Gives the actors of a row, newest `updated` first. */
export function shownActors(worktree: Worktree): Actor[] {
  return [...worktree.actors].sort((a, b) => Date.parse(b.updated) - Date.parse(a.updated));
}

/** The mark after a build that no job holds. */
const NOT_QUEUED: readonly Piece[] = [
  own(' '),
  { kind: 'own', text: '(not queued)', className: 'not-queued' },
];

/**
 * Gives the lines of the agent column, each a list of pieces. With `flagUnqueued`, a build that no
 * job holds is marked, on an actor's line by the build its action names.
 */
export function agentLines(worktree: Worktree, flagUnqueued = false): Piece[][] {
  const mark = (unqueued: boolean): readonly Piece[] =>
    flagUnqueued && unqueued ? NOT_QUEUED : [];
  if (worktree.actors.length) {
    return shownActors(worktree).map((actor) => {
      const building =
        actor.action?.tool === 'build:' &&
        worktree.builds.some((b) => b.name === actor.action?.target && isUnqueued(b));
      return [
        theirs(actor.label),
        own(DOT),
        ...doing(actor),
        ...runTimePieces(actor),
        ...mark(building),
      ];
    });
  }
  const [build] = worktree.builds;
  if (build) return [[own('build: '), theirs(build.name), ...mark(isUnqueued(build))]];
  return [[own('no agent')]];
}

/** Gives the times whose ages the Last column shows, one per agent line. */
export function lastTimes(worktree: Worktree): Array<string | null> {
  if (worktree.actors.length) return shownActors(worktree).map((actor) => actor.updated);
  return [worktree.last_event ?? worktree.last_change];
}

/** Gives an actor's state in the detail strip. */
export function stateText(actor: Actor): Piece[] {
  if (actor.state === 'waiting') {
    return [own(`waiting for ${actor.reason === null ? 'you' : WAITING_FOR[actor.reason]}`)];
  }
  if (actor.state === 'idle' && actor.error) {
    return [own(`idle${DOT}API error: `), theirs(actor.error)];
  }
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

/** Compares two names by UTF-16 code unit, not by locale. */
function byName(a: string, b: string): number {
  if (a < b) return -1;
  return a > b ? 1 : 0;
}

/** Orders rows: foreground, slots, unmanaged, each by name. */
export function rowOrder(worktrees: Worktree[]): Worktree[] {
  return [...worktrees].sort(
    (a, b) => ROLE_ORDER[a.role] - ROLE_ORDER[b.role] || byName(a.name, b.name),
  );
}
