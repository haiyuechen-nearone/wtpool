import type { Build, Heavy as HeavyPattern, Jobs, StatusDocument } from '../generated/status';

export type { Heavy as HeavyPattern, Job, Jobs, Suggestion } from '../generated/status';

/** The policies for heavy commands outside a job, in the order the panel offers them. */
export const POLICIES = ['off', 'warn', 'require'] as const;

export type Policy = (typeof POLICIES)[number];

/** The list a heavy pattern belongs to: this machine's, or this repository's. */
export type Scope = 'machine' | 'repository';

/** Gives the heavy jobs of `document`, or undefined from a wtpool without them. */
export function jobsOf(document: StatusDocument | null): Jobs | undefined {
  return document?.machine.jobs;
}

/** Tells a build that no job holds. A wtpool without heavy jobs never says so. */
export function isUnqueued(build: Build): boolean {
  return build.queued === false;
}

/** Tells a policy that flags builds outside a job: `warn` and `require`. */
export function flagsUnqueued(jobs: Jobs | undefined): boolean {
  return jobs !== undefined && jobs.enforce !== 'off';
}

/** Gives the entries of the list `scope`: the repository's own and its `build_processes` names. */
export function scopeEntries(jobs: Jobs, scope: Scope): HeavyPattern[] {
  return jobs.heavy.filter((entry) => (entry.source === 'machine') === (scope === 'machine'));
}

/** Gives the patterns of the list `scope`, as {@link scopeEntries} orders them. */
export function scopePatterns(jobs: Jobs, scope: Scope): string[] {
  return scopeEntries(jobs, scope).map((entry) => entry.pattern);
}

/** The words {@link isPattern} checks, shown under an input whose text breaks them. */
export const PATTERN_RULE =
  'Not a pattern: 1 to 3 words of letters, digits and . _ + -, and : after the first, none starting with - or +; only the first may end in *.';

/** A first word: letters, digits and `. _ + -`, not starting with `-` or `+`, and maybe a final `*`. */
const FIRST_WORD = /^(?![-+])[A-Za-z0-9._+-]{1,32}$|^(?![-+])[A-Za-z0-9._+-]{1,31}\*$/;
/** A later word: as a first word with `:` too, and no `*`. */
const LATER_WORD = /^(?![-+])[A-Za-z0-9._+:-]{1,32}$/;

/**
 * Tells a heavy pattern: one to three words split by single spaces. The CLI owns the grammar;
 * this answers inline before anything is sent.
 */
export function isPattern(text: string): boolean {
  const [first, ...later] = text.split(' ');
  return (
    later.length <= 2 && FIRST_WORD.test(first) && later.every((word) => LATER_WORD.test(word))
  );
}

/** An add or a removal of one pattern of the list `scope`. */
export interface PatternEdit {
  op: 'add' | 'rm';
  scope: Scope;
  pattern: string;
}

/** An add of each of this repository's suggestions. */
export interface SuggestedEdit {
  op: 'add-suggested';
  scope: 'repository';
  pattern: null;
}

/** An edit of one heavy list. */
export type HeavyEdit = PatternEdit | SuggestedEdit;

/** Asks for a new limit, and a new policy too when `enforce` is set; null asks for the default. */
export interface LimitMessage {
  type: 'jobSettings';
  limit: number | null;
  enforce?: Policy | null;
  heavy?: never;
}

/** Asks for a new policy alone; null asks for the default. */
export interface PolicyMessage {
  type: 'jobSettings';
  limit?: never;
  enforce: Policy | null;
  heavy?: never;
}

/** Asks for one heavy edit, alone. */
export interface HeavyMessage {
  type: 'jobSettings';
  limit?: never;
  enforce?: never;
  heavy: HeavyEdit;
}

/** Asks for new settings, in the shapes the extension accepts. */
export type JobSettingsMessage = LimitMessage | PolicyMessage | HeavyMessage;

/** The extension's answer to a {@link JobSettingsMessage}, `scope` naming the list of a heavy edit. */
export interface JobSettingsAnswer {
  type: 'jobSettings';
  version: 1;
  ok: boolean;
  error?: string;
  scope?: Scope;
}

/** The time the extension lets one `wtpool job` call run before it stops it. */
export const CALL_TIMEOUT_MS = 10_000;

/**
 * The time an answer may take past its calls' timeouts: a stopped call still has to end, and the
 * strip reads the clock once a second.
 */
const ANSWER_SLACK_MS = 2_000;

/**
 * The time a message whose effect a document shows still waits for its answer. The extension
 * answers no message it refuses, and it refuses one its document already shows; the watch can
 * also show an accepted change before the call that made it ends.
 */
const SHOWN_WAIT_MS = 1_000;

/** Gives the number of `wtpool job` calls the extension makes for `message`. */
function callCount(message: JobSettingsMessage): number {
  if (message.heavy) return 1;
  return Number(message.limit !== undefined) + Number(message.enforce !== undefined);
}

/**
 * Gives the time the answer to `message`, sent at `nowMs`, is due. The extension carries out one
 * message at a time, so its answer also waits for those of the messages before it, due at `before`.
 */
export function answerDue(message: JobSettingsMessage, nowMs: number, before: number): number {
  return Math.max(nowMs, before) + callCount(message) * CALL_TIMEOUT_MS + ANSWER_SLACK_MS;
}

/** Tells `jobs` already showing the settings or the list `message` asks for. */
export function jobsShow(jobs: Jobs, message: JobSettingsMessage): boolean {
  if (message.heavy) return listShows(jobs, message.heavy);
  const limit =
    message.limit === undefined ||
    (message.limit === null ? jobs.source.limit === 'default' : jobs.limit === message.limit);
  const enforce =
    message.enforce === undefined ||
    (message.enforce === null
      ? jobs.source.enforce === 'default'
      : jobs.enforce === message.enforce);
  return limit && enforce;
}

function listShows(jobs: Jobs, edit: HeavyEdit): boolean {
  if (edit.op === 'add-suggested') return !jobs.suggested.some((entry) => isPattern(entry.pattern));
  return scopePatterns(jobs, edit.scope).includes(edit.pattern) === (edit.op === 'add');
}

/** A message waiting for its answer until `due`. */
export interface Awaiting {
  due: number;
}

/**
 * Tells the wait of `awaiting` over at `nowMs`, and once `jobs` shows `message`, shortens it to
 * {@link SHOWN_WAIT_MS}.
 */
export function waitEnds(
  awaiting: Awaiting,
  message: JobSettingsMessage,
  jobs: Jobs | undefined,
  nowMs: number,
): boolean {
  if (jobs && jobsShow(jobs, message)) awaiting.due = Math.min(awaiting.due, nowMs + SHOWN_WAIT_MS);
  return awaiting.due <= nowMs;
}
