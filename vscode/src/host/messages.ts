import type { Output, StatusDocument, Worktree } from '../generated/status';
import type { ConfirmedActionType } from '../shared/confirmation';
import {
  isPattern,
  jobsOf,
  POLICIES,
  scopePatterns,
  type Jobs,
  type Scope,
} from '../shared/jobs';
import {
  holdsBranch,
  isActionType,
  isRecord,
  offeredActions,
  type ActionType,
  type BranchSlot,
} from '../shared/protocol';
import { isTaskOutputPath } from './outputs';

type DirectActionType = Exclude<ActionType, ConfirmedActionType>;

/** Asks for `open`, `terminal` or `copyPath` on a row. */
export interface DirectAction {
  type: DirectActionType;
  worktree: Worktree;
}

/** Asks for `free` or `fg` on a slot that holds a branch. */
interface ConfirmedAction {
  type: ConfirmedActionType;
  worktree: BranchSlot;
}

interface Selection {
  type: 'select';
  /** The row's path, or null when no row is selected. */
  row: string | null;
}

/** Asks for the tab's view, from a page that just started. */
interface Ready {
  type: 'ready';
}

/** Asks for a Follow terminal of an output the document lists. */
interface FollowRequest {
  type: 'follow';
  output: Output;
  /** The label of the actor that lists the output, the terminal's name. */
  label: string;
}

/** One `wtpool job` call: the setting it changes, named in a timeout, and its arguments. */
export interface JobCall {
  setting: 'limit' | 'enforce' | 'heavy';
  args: string[];
}

/**
 * A `jobSettings` message the tab accepted: its calls, in order, and the list a heavy edit names.
 */
export interface JobSettingsRequest {
  type: 'jobSettings';
  calls: JobCall[];
  scope?: Scope;
}

/** A request of the webview, by its message's `type`. */
export type WebviewRequest =
  | DirectAction
  | ConfirmedAction
  | Selection
  | FollowRequest
  | JobSettingsRequest
  | Ready;

type Fields = Readonly<Record<string, unknown>>;

function isPlain(value: unknown): value is Fields {
  return isRecord(value) && Object.getPrototypeOf(value) === Object.prototype;
}

/** Tells `fields` with exactly `keys`, sorted and joined by commas. */
function hasKeys(fields: Fields, keys: string): boolean {
  return Object.keys(fields).sort().join() === keys;
}

/** Finds the worktree at `path` in the document's first pool, the only one the tab shows. */
export function worktreeAt(document: StatusDocument | null, path: unknown): Worktree | undefined {
  return document?.pools[0].worktrees.find((row) => row.path === path);
}

function isConfirmed(type: ActionType): type is ConfirmedActionType {
  return type === 'free' || type === 'fg';
}

function actionOf(
  type: ActionType,
  id: string,
  document: StatusDocument | null,
): DirectAction | ConfirmedAction | undefined {
  const worktree = worktreeAt(document, id);
  if (!worktree) {
    return undefined;
  }
  if (isConfirmed(type)) {
    return holdsBranch(worktree) ? { type, worktree } : undefined;
  }
  return offeredActions(worktree).includes(type) ? { type, worktree } : undefined;
}

function selectionOf(id: string, document: StatusDocument | null): Selection | undefined {
  if (id === '') {
    return { type: 'select', row: null };
  }
  return worktreeAt(document, id) ? { type: 'select', row: id } : undefined;
}

function followOf(id: string, document: StatusDocument | null): FollowRequest | undefined {
  if (!isTaskOutputPath(id)) {
    return undefined;
  }
  for (const worktree of document?.pools[0].worktrees ?? []) {
    for (const actor of worktree.actors) {
      const output = actor.outputs?.find((listed) => listed.path === id);
      if (output) {
        return { type: 'follow', output, label: actor.label };
      }
    }
  }
  return undefined;
}

function limitArg(value: unknown, jobs: Jobs): string | undefined {
  if (value === null) return 'default';
  const whole = typeof value === 'number' && Number.isInteger(value);
  return whole && value >= jobs.min && value <= jobs.max ? String(value) : undefined;
}

function enforceArg(value: unknown): string | undefined {
  return value === null ? 'default' : POLICIES.find((policy) => policy === value);
}

/** The calls of a message that sets the limit, the policy or both, else undefined. */
function settingsCalls(message: Fields, jobs: Jobs): JobCall[] | undefined {
  const keySets = ['limit,type', 'enforce,type', 'enforce,limit,type'];
  if (!keySets.some((keys) => hasKeys(message, keys))) return undefined;
  const calls: JobCall[] = [];
  if (Object.hasOwn(message, 'limit')) {
    const arg = limitArg(message.limit, jobs);
    if (arg === undefined) return undefined;
    calls.push({ setting: 'limit', args: ['job', 'limit', arg] });
  }
  if (Object.hasOwn(message, 'enforce')) {
    const arg = enforceArg(message.enforce);
    if (arg === undefined) return undefined;
    calls.push({ setting: 'enforce', args: ['job', 'enforce', arg] });
  }
  return calls;
}

/**
 * The patterns of one heavy edit, none when it changes nothing: an add of a pattern its list
 * lacks, a removal of one it holds, or this repository's suggestions, each a pattern again, as the
 * CLI refuses a whole call for one bad pattern.
 */
function patternsOf(op: unknown, scope: Scope, pattern: unknown, jobs: Jobs): string[] {
  if (op === 'add-suggested') {
    if (scope !== 'repository' || pattern !== null) return [];
    return jobs.suggested.map((entry) => entry.pattern).filter(isPattern);
  }
  if (op !== 'add' && op !== 'rm') return [];
  if (typeof pattern !== 'string' || !isPattern(pattern)) return [];
  return scopePatterns(jobs, scope).includes(pattern) === (op === 'rm') ? [pattern] : [];
}

/** The call of one heavy edit, else undefined. */
function heavyRequest(edit: unknown, jobs: Jobs): JobSettingsRequest | undefined {
  if (!isPlain(edit) || !hasKeys(edit, 'op,pattern,scope')) return undefined;
  const { op, scope, pattern } = edit;
  if (scope !== 'machine' && scope !== 'repository') return undefined;
  const patterns = patternsOf(op, scope, pattern, jobs);
  if (!patterns.length) return undefined;
  const verb = op === 'rm' ? 'rm' : 'add';
  const repo = scope === 'repository' ? ['--repo'] : [];
  const args = ['job', 'heavy', verb, ...repo, '--', ...patterns];
  return { type: 'jobSettings', calls: [{ setting: 'heavy', args }], scope };
}

/**
 * Reads a `jobSettings` message against the tab's last document: the limit, the policy or both,
 * each null for its default, or one heavy edit alone. Nothing counts while the document has no
 * heavy jobs.
 */
function readJobSettings(
  message: Fields,
  document: StatusDocument | null,
): JobSettingsRequest | undefined {
  const jobs = jobsOf(document);
  if (!jobs) return undefined;
  if (Object.hasOwn(message, 'heavy')) {
    return hasKeys(message, 'heavy,type') ? heavyRequest(message.heavy, jobs) : undefined;
  }
  const calls = settingsCalls(message, jobs);
  return calls && { type: 'jobSettings', calls };
}

/** Reads `message` as a request to the tab with `document`, if it is a valid one. */
export function readRequest(
  message: unknown,
  document: StatusDocument | null,
): WebviewRequest | undefined {
  if (!isPlain(message)) {
    return undefined;
  }
  const { type, id } = message;
  if (type === 'jobSettings') {
    return readJobSettings(message, document);
  }
  if (type === 'ready') {
    return hasKeys(message, 'type') ? { type } : undefined;
  }
  if (!hasKeys(message, 'id,type') || typeof id !== 'string') {
    return undefined;
  }
  if (type === 'select') {
    return selectionOf(id, document);
  }
  if (type === 'follow') {
    return followOf(id, document);
  }
  return isActionType(type) ? actionOf(type, id, document) : undefined;
}
