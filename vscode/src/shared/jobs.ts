// Both tsconfigs include this file, so it imports only types of the generated schema: no Node,
// VS Code or DOM API.
import type { Build, Jobs, StatusDocument } from '../generated/status';

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

/** Gives the patterns of the list `scope`: the repository's own and its `build_processes` names. */
export function scopePatterns(jobs: Jobs, scope: Scope): string[] {
  return jobs.heavy.filter((entry) => (entry.source === 'machine') === (scope === 'machine')).map((entry) => entry.pattern);
}

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
  return later.length <= 2 && FIRST_WORD.test(first) && later.every((word) => LATER_WORD.test(word));
}

/** An edit of one heavy list. `add-suggested` adds this repository's suggestions. */
export interface HeavyEdit {
  op: 'add' | 'rm' | 'add-suggested';
  scope: Scope;
  pattern: string | null;
}

/** Asks for new settings: the limit and the policy, null for the default, or one heavy edit. */
export interface JobSettingsMessage {
  type: 'jobSettings';
  limit?: number | null;
  enforce?: Policy | null;
  heavy?: HeavyEdit;
}

/** The extension's answer to a {@link JobSettingsMessage}, `scope` naming the list of a heavy edit. */
export interface JobSettingsAnswer {
  type: 'jobSettings';
  version: 1;
  ok: boolean;
  error?: string;
  scope?: Scope;
}
