import type { Worktree } from '../generated/status';
import { visible } from '../shared/invisible';
import {
  answerDue,
  jobsShow,
  POLICIES,
  waitEnds,
  type Awaiting,
  type JobSettingsAnswer,
  type JobSettingsMessage,
  type Jobs,
  type LimitMessage,
  type Policy,
  type PolicyMessage,
} from '../shared/jobs';
import {
  bdi,
  button,
  cell,
  div,
  label,
  paragraph,
  setAttr,
  setShownText,
  setText,
  span,
} from './dom';
import { HeavyEditor } from './heavy_editor';
import { JobList } from './job_list';
import { DOT } from './text';

/** Sends the strip's settings, speaks its answers, and closes it. */
export interface StripHost {
  send(message: JobSettingsMessage): void;
  announce(text: string): void;
  close(): void;
}

/** The machine as the strip shows it: its name, heavy jobs, the pool's worktrees and its folder. */
interface MachineView {
  name: string;
  jobs?: Jobs;
  worktrees: Worktree[];
  /** The tab's folder is not on this computer. */
  remote: boolean;
}

const MEANINGS: Readonly<Record<Policy, string>> = {
  off: 'Heavy work outside a job is not flagged.',
  warn: 'Heavy work outside a job shows as not queued.',
  require: "Agents' heavy commands outside a job are refused, with the wrapped command to run.",
};

/** A save sent and not yet answered: the field and the menu as it sent them. */
interface Saving extends Awaiting {
  limit: string;
  enforce: string;
  message: LimitMessage | PolicyMessage;
}

/** The sentence that names the command form, with that form as code. */
function hint(): HTMLParagraphElement {
  const element = paragraph('limit-hint');
  const code = document.createElement('code');
  code.textContent = 'wtpool job -- <command>';
  element.append(
    document.createTextNode('Agents and scripts take a place when they run a command as '),
    code,
    document.createTextNode('.'),
  );
  return element;
}

/** The note of a wtpool without heavy jobs. */
function oldWtpool(): HTMLParagraphElement {
  const element = paragraph('limit-note old-wtpool');
  const code = document.createElement('code');
  code.textContent = 'wtpool job';
  element.append(
    document.createTextNode('This wtpool has no heavy job limit. Update wtpool to a version with '),
    code,
    document.createTextNode(' to set one.'),
  );
  return element;
}

/**
 * The machine strip under the heading: the limit, the policy and the heavy patterns on the left,
 * the running, queued and not queued jobs on the right. It is made once and follows each document
 * in place and keeps the user's unsaved edits.
 */
export class MachineStrip {
  readonly element = document.createElement('tr');
  private readonly box = div('strip machine-strip');
  private readonly heading = div('agents-heading');
  private readonly machineName = bdi('');
  private readonly fault = paragraph('limit-fault');
  private readonly limit = document.createElement('input');
  private readonly enforce = document.createElement('select');
  private readonly meaning = span('policy-meaning');
  private readonly limitError = paragraph('limit-error');
  private readonly status = span('limit-status');
  private readonly over = paragraph('limit-over');
  private readonly save = button('job-button job-save', 'Save');
  private readonly defaults = button('job-button secondary job-default', 'Use defaults');
  private readonly saved = paragraph('limit-note');
  /** The parts of the saved note: its start, the config's path, and its end. */
  private readonly savedParts = [
    document.createTextNode(''),
    bdi(''),
    document.createTextNode(''),
  ] as const;
  private readonly remote = paragraph('limit-note');
  private readonly editor: HeavyEditor;
  private readonly left = div('jobs-limit');
  private readonly right = div('jobs-lists');
  private readonly old = oldWtpool();
  private readonly running = new JobList('jobs-running', false);
  private readonly waiting = new JobList('jobs-waiting', true);
  private readonly outside = new JobList('jobs-outside', false);
  private jobs?: Jobs;
  /** The field and the menu hold edits the user did not save. */
  private edited = { limit: false, enforce: false };
  /** The last save, until its answer comes or the wait for it ends. */
  private saving?: Saving;
  /** The due time of the answer to the last message sent. */
  private answersDue = 0;
  private nowMs = 0;

  constructor(
    columns: number,
    private readonly host: StripHost,
  ) {
    this.element.className = 'machine-detail';
    const detailCell = cell('detail-cell');
    detailCell.colSpan = columns;
    this.box.setAttribute('role', 'region');
    detailCell.append(this.box);
    this.element.append(detailCell);
    this.editor = new HeavyEditor({
      send: (message) => this.post(message),
      announce: (text) => void host.announce(text),
    });
    this.buildForm();
    this.element.addEventListener('keydown', (event) => {
      if (event.key !== 'Escape') return;
      event.preventDefault();
      host.close();
    });
  }

  private buildForm(): void {
    this.limit.id = 'job-limit';
    this.limit.className = 'job-limit';
    this.limit.type = 'text';
    this.limit.inputMode = 'numeric';
    this.limit.setAttribute('aria-describedby', 'limit-error');
    this.limitError.id = 'limit-error';
    this.limitError.hidden = true;
    this.enforce.id = 'job-enforce';
    this.enforce.className = 'job-enforce';
    for (const policy of POLICIES) {
      const option = document.createElement('option');
      option.value = policy;
      option.textContent = policy;
      this.enforce.append(option);
    }
    const fields = div('limit-row');
    fields.append(
      label('job-limit', 'Limit'),
      this.limit,
      label('job-enforce', 'Policy'),
      this.enforce,
      this.meaning,
    );
    const actions = div('limit-row');
    actions.append(this.save, this.defaults, this.status);
    this.heading.append(document.createTextNode('Heavy jobs on '), this.machineName);
    this.saved.append(...this.savedParts);
    const heavyHeading = div('agents-heading heavy-heading');
    heavyHeading.textContent = 'Heavy commands';
    this.left.append(
      this.heading,
      this.fault,
      fields,
      this.limitError,
      actions,
      this.over,
      this.saved,
      this.remote,
      heavyHeading,
      this.editor.element,
      hint(),
    );
    this.right.append(this.running.element, this.waiting.element, this.outside.element);
    this.limit.addEventListener('input', () => {
      this.edited.limit = true;
      this.showStatus('', false);
      if (this.jobs) this.checkLimit(this.jobs);
    });
    this.enforce.addEventListener('change', () => {
      this.edited.enforce = true;
      this.showStatus('', false);
      if (this.jobs) this.showMeaning(this.jobs);
    });
    const enter = (event: KeyboardEvent): void => {
      if (event.key !== 'Enter') return;
      event.preventDefault();
      this.saveEdits();
    };
    this.limit.addEventListener('keydown', enter);
    this.enforce.addEventListener('keydown', enter);
    this.save.addEventListener('click', () => void this.saveEdits());
    this.defaults.addEventListener(
      'click',
      () => void this.send({ type: 'jobSettings', limit: null, enforce: null }),
    );
  }

  /** Shows `view`, changing only the parts that differ, and keeps the user's unsaved edits. */
  show(view: MachineView, nowMs: number): void {
    const name = visible(view.name);
    setAttr(this.box, 'aria-label', `Heavy jobs on ${name}`);
    const jobs = view.jobs;
    this.jobs = jobs;
    this.nowMs = nowMs;
    this.settleSave(jobs);
    if (!jobs) {
      if (this.box.firstChild !== this.old) this.box.replaceChildren(this.old);
      return;
    }
    if (this.box.firstChild !== this.left) this.box.replaceChildren(this.left, this.right);
    setText(this.machineName, name);
    setShownText(this.fault, jobs.error === null ? '' : visible(jobs.error));
    if (!this.edited.limit) this.limit.value = jobs.limit === null ? '' : String(jobs.limit);
    if (!this.edited.enforce) this.enforce.value = jobs.enforce;
    this.showMeaning(jobs);
    const running = jobs.running.length;
    const limit = jobs.limit ?? running;
    setShownText(
      this.over,
      running > limit
        ? `${running} running, over the limit of ${limit}: they finish, and queued jobs start once fewer than ${limit} run.`
        : '',
    );
    const fromFile = jobs.source.limit === 'machine' || jobs.source.enforce === 'machine';
    const saved = fromFile
      ? [
          'Saved in ',
          visible(jobs.config),
          `${DOT}defaults ${jobs.default}, warn${DOT}${jobs.min} to ${jobs.max}`,
        ]
      : ['Not saved: the defaults apply', '', ''];
    this.savedParts.forEach((part, at) => void setText(part, saved[at]));
    setShownText(
      this.remote,
      view.remote
        ? `This window's folder is not on this computer: the settings are for this computer, ${name}.`
        : '',
    );
    this.editor.show(jobs, nowMs);
    this.running.show(
      jobs.limit === null ? `Running ${running}` : `Running ${running} of ${jobs.limit}`,
      jobs.running,
      view.worktrees,
      nowMs,
    );
    this.waiting.show(`Queued ${jobs.waiting.length}`, jobs.waiting, view.worktrees, nowMs);
    this.outside.show(`Not queued ${jobs.outside.length}`, jobs.outside, view.worktrees, nowMs);
    if (jobs.enforce === 'off') this.outside.element.remove();
    else if (!this.outside.element.isConnected) this.right.append(this.outside.element);
  }

  /** Recomputes the ages of the jobs, and stops waiting for overdue answers. */
  tick(nowMs: number): void {
    this.nowMs = nowMs;
    for (const list of [this.running, this.waiting, this.outside]) {
      list.tick(nowMs);
    }
    this.settleSave(undefined);
    this.editor.tick(nowMs);
  }

  /** Moves focus to the Limit field, when the strip shows one. */
  focus(): void {
    if (this.limit.isConnected) this.limit.focus();
  }

  /** Shows the extension's answer to the last message the strip sent. */
  answer(answer: JobSettingsAnswer): void {
    if (answer.scope) {
      this.editor.answer(answer, answer.scope);
      return;
    }
    this.endSave(answer.ok, answer.ok ? 'Saved.' : visible(answer.error ?? ''));
  }

  /** Ends the wait for the last save's answer, says `said`, and frees the buttons. */
  private endSave(ok: boolean, said: string): void {
    const sent = this.saving;
    this.saving = undefined;
    this.showBusy(false);
    this.showStatus(said, !ok);
    // Edits made while the save was out stay the user's.
    if (ok && sent)
      this.edited = {
        limit: this.limit.value !== sent.limit,
        enforce: this.enforce.value !== sent.enforce,
      };
    this.host.announce(said);
  }

  private showStatus(said: string, failed: boolean): void {
    setText(this.status, said);
    this.status.classList.toggle('failed', failed);
  }

  /** Disables the buttons while a save waits for its answer, Save reading so. */
  private showBusy(busy: boolean): void {
    this.save.disabled = busy;
    this.defaults.disabled = busy;
    setText(this.save, busy ? 'Saving…' : 'Save');
  }

  /** Gives the field's limit, else marks the field invalid with the reason under it. */
  private checkLimit(jobs: Jobs): number | undefined {
    const value = this.limit.value;
    const limit =
      /^\d+$/.test(value) && Number(value) >= jobs.min && Number(value) <= jobs.max
        ? Number(value)
        : undefined;
    const reason =
      limit === undefined ? `Enter a whole number from ${jobs.min} to ${jobs.max}.` : '';
    setAttr(this.limit, 'aria-invalid', String(limit === undefined));
    if (reason && this.limitError.textContent !== reason) this.host.announce(reason);
    setShownText(this.limitError, reason);
    return limit;
  }

  private policy(jobs: Jobs): Policy {
    return POLICIES.find((policy) => policy === this.enforce.value) ?? jobs.enforce;
  }

  private showMeaning(jobs: Jobs): void {
    setText(this.meaning, MEANINGS[this.policy(jobs)]);
  }

  /** Sends the changed limit and policy, and nothing for an invalid limit. */
  private saveEdits(): void {
    if (!this.jobs) return;
    const limit = this.checkLimit(this.jobs);
    if (limit === undefined) return;
    const policy = this.policy(this.jobs);
    const enforce = policy === this.jobs.enforce ? undefined : policy;
    if (limit !== this.jobs.limit)
      this.send(
        enforce === undefined
          ? { type: 'jobSettings', limit }
          : { type: 'jobSettings', limit, enforce },
      );
    else if (enforce !== undefined) this.send({ type: 'jobSettings', enforce });
  }

  /** Sends `message` unless a save waits for its answer. */
  private send(message: LimitMessage | PolicyMessage): void {
    if (this.saving) return;
    this.showStatus('', false);
    this.showBusy(true);
    this.saving = {
      limit: this.limit.value,
      enforce: this.enforce.value,
      message,
      due: this.post(message),
    };
  }

  /** Sends `message` and gives the time its answer is due by. */
  private post(message: JobSettingsMessage): number {
    this.answersDue = answerDue(message, this.nowMs, this.answersDue);
    this.host.send(message);
    return this.answersDue;
  }

  /**
   * Lets Save send again once the answer to the last save is overdue, or `jobs` has shown it a
   * while: a save the document shows counts as saved, any other says no answer came.
   */
  private settleSave(jobs: Jobs | undefined): void {
    const sent = this.saving;
    if (!sent || !waitEnds(sent, sent.message, jobs, this.nowMs)) return;
    const shown = this.jobs !== undefined && jobsShow(this.jobs, sent.message);
    this.endSave(shown, shown ? 'Saved.' : 'No answer from wtpool');
  }
}
