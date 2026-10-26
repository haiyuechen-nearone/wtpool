import type { JobSettingsAnswer } from '../shared/jobs';
import type { Clock } from './clock';
import type { JobCall, JobSettingsRequest } from './messages';
import { lastErrorLine, startRun } from './spawn';

/** How long one `wtpool job` call may run before it is stopped. */
const CALL_TIMEOUT_MS = 10_000;

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
  private async call({ setting, args }: JobCall): Promise<string | undefined> {
    const run = startRun(this.context.binary(), args, this.context.cwd());
    let timedOut = false;
    const cancel = this.context.clock.after(CALL_TIMEOUT_MS, () => {
      timedOut = true;
      run.stop();
    });
    const end = await run.ended;
    cancel();
    return timedOut ? `wtpool job ${setting} timed out` : lastErrorLine(end);
  }
}
