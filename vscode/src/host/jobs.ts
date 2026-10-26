import type { StatusDocument } from '../generated/status';
import {
  isPattern,
  jobsOf,
  POLICIES,
  scopePatterns,
  type JobSettingsAnswer,
  type Jobs,
  type Scope,
} from '../shared/jobs';
import { isRecord } from '../shared/protocol';
import type { Clock } from './clock';
import { startRun } from './spawn';
import { endError } from './stream';

/** How long one `wtpool job` call may run before it is stopped. */
const CALL_TIMEOUT_MS = 10_000;

/** One `wtpool job` call: the setting it changes, named in a timeout, and its arguments. */
interface Call {
  setting: 'limit' | 'enforce' | 'heavy';
  args: string[];
}

/** A `jobSettings` message the tab accepted: its calls, in order, and the list a heavy edit names. */
export interface JobSettingsRequest {
  type: 'jobSettings';
  calls: Call[];
  scope?: Scope;
}

type Fields = Readonly<Record<string, unknown>>;

function isPlain(value: unknown): value is Fields {
  return isRecord(value) && Object.getPrototypeOf(value) === Object.prototype;
}

/** Tells `fields` with exactly `keys`, sorted and joined by commas. */
function hasKeys(fields: Fields, keys: string): boolean {
  return Object.keys(fields).sort().join() === keys;
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
function settingsCalls(message: Fields, jobs: Jobs): Call[] | undefined {
  if (!['limit,type', 'enforce,type', 'enforce,limit,type'].some((keys) => hasKeys(message, keys))) return undefined;
  const calls: Call[] = [];
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
 * The call of one heavy edit, else undefined: an add of a pattern its list lacks, a removal of one
 * it holds, or this repository's suggestions, each a pattern again, as the CLI refuses a whole call
 * for one bad pattern.
 */
function heavyRequest(edit: unknown, jobs: Jobs): JobSettingsRequest | undefined {
  if (!isPlain(edit) || !hasKeys(edit, 'op,pattern,scope')) return undefined;
  const { op, scope, pattern } = edit;
  if (scope !== 'machine' && scope !== 'repository') return undefined;
  const repo = scope === 'repository' ? ['--repo'] : [];
  let patterns: string[] = [];
  if (op === 'add-suggested') {
    if (scope === 'repository' && pattern === null) patterns = jobs.suggested.map((entry) => entry.pattern).filter(isPattern);
  } else if ((op === 'add' || op === 'rm') && typeof pattern === 'string' && isPattern(pattern)) {
    if (scopePatterns(jobs, scope).includes(pattern) === (op === 'rm')) patterns = [pattern];
  }
  if (!patterns.length) return undefined;
  const verb = op === 'rm' ? 'rm' : 'add';
  return { type: 'jobSettings', calls: [{ setting: 'heavy', args: ['job', 'heavy', verb, ...repo, '--', ...patterns] }], scope };
}

/**
 * Reads a `jobSettings` message against the tab's last document: the limit, the policy or both,
 * each null for its default, or one heavy edit alone. Nothing counts while the document has no
 * heavy jobs.
 */
export function readJobSettings(message: Fields, document: StatusDocument | null): JobSettingsRequest | undefined {
  const jobs = jobsOf(document);
  if (!jobs) return undefined;
  if (Object.hasOwn(message, 'heavy')) {
    return hasKeys(message, 'heavy,type') ? heavyRequest(message.heavy, jobs) : undefined;
  }
  const calls = settingsCalls(message, jobs);
  return calls && { type: 'jobSettings', calls };
}

/** The parts of a tab that a {@link JobSettingsRunner} uses. */
export interface JobSettingsContext {
  clock: Clock;
  binary(): string;
  cwd(): string | undefined;
  answer(answer: JobSettingsAnswer): void;
}

/**
 * Carries out accepted settings messages one at a time, in the order they came: each call after
 * the one before it ended, none after one that failed, and one answer per message.
 */
export class JobSettingsRunner {
  private queue = Promise.resolve();

  constructor(private readonly context: JobSettingsContext) {}

  run(request: JobSettingsRequest): void {
    this.queue = this.queue.then(() => this.carryOut(request));
  }

  private async carryOut(request: JobSettingsRequest): Promise<void> {
    let error: string | undefined;
    for (const call of request.calls) {
      error = await this.call(call);
      if (error !== undefined) break;
    }
    const answer: JobSettingsAnswer = { type: 'jobSettings', version: 1, ok: error === undefined };
    if (error !== undefined) answer.error = error;
    if (request.scope) answer.scope = request.scope;
    this.context.answer(answer);
  }

  /** Runs one call and gives its error, or undefined once it succeeded. */
  private async call({ setting, args }: Call): Promise<string | undefined> {
    const run = startRun(this.context.binary(), args, this.context.cwd());
    let timedOut = false;
    const cancel = this.context.clock.after(CALL_TIMEOUT_MS, () => {
      timedOut = true;
      run.stop();
    });
    const end = await run.ended;
    cancel();
    return timedOut ? `wtpool job ${setting} timed out` : endError(end);
  }
}
