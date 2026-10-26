import type { Actor, Output } from '../generated/status';
import { visible } from '../shared/invisible';
import type { Tail } from '../shared/protocol';
import { bdi, button, div, setText, span } from './dom';
import { KeyedCache, placeInOrder } from './order';
import { DOT, clock, runTime } from './text';

/** The pixels from the end that a block may be scrolled up and still count as at the end. */
const AT_END_PX = 4;

function taskState(output: Output, nowMs: number): string {
  if (output.running || output.ended === null) {
    return `running${DOT}${runTime(output.started, nowMs)}`;
  }
  return `ended ${clock(output.ended)}`;
}

function tailText(tail?: Tail): string {
  if (!tail) return 'reading the output';
  if (tail.error !== undefined) return visible(tail.error);
  if (!tail.lines.length) return 'no output yet';
  return tail.lines.map((line) => visible(line)).join('\n');
}

/** Shows one task's heading, Follow button and tail block. */
class TaskView {
  readonly element = div('output');
  private readonly summary = bdi('', 'output-summary');
  private readonly state = span('output-state');
  private readonly button = button('follow', 'Follow');
  private readonly block = document.createElement('pre');
  private shown?: string;
  private output?: Output;
  /** True while the block follows the end: it was at the end when the user last scrolled it. */
  private atEnd = true;

  constructor(readonly path: string) {
    this.element.setAttribute('data-output-path', path);
    const heading = div('output-heading');
    heading.append(this.summary, document.createTextNode(DOT), this.state);
    this.button.setAttribute('data-follow', path);
    this.block.className = 'output-tail';
    // A block that scrolls takes keyboard focus, so the keyboard can scroll it too.
    this.block.tabIndex = 0;
    this.block.addEventListener('scroll', () => {
      this.atEnd =
        this.block.scrollTop + this.block.clientHeight >= this.block.scrollHeight - AT_END_PX;
    });
    const top = div('output-top');
    top.append(heading, this.button);
    this.element.append(top, this.block);
    this.show();
  }

  /** Shows the summary and state of `output`, listed by the actor `label`, at `nowMs`. */
  update(output: Output, label: string, nowMs: number): void {
    this.output = output;
    setText(this.summary, visible(output.summary || 'background task'));
    this.tick(nowMs);
    const name = `${visible(output.summary || 'background task')} of ${visible(label)}`;
    if (this.button.getAttribute('aria-label') !== `Follow ${name} in a terminal`) {
      this.button.setAttribute('aria-label', `Follow ${name} in a terminal`);
      this.block.setAttribute('aria-label', `Output of ${name}`);
    }
  }

  /** Recomputes the run time of a running task, nothing else. */
  tick(nowMs: number): void {
    if (this.output) setText(this.state, taskState(this.output, nowMs));
  }

  /** Shows `tail`, or that none came yet, and keeps to the end unless the user scrolled up. */
  show(tail?: Tail): void {
    const text = tailText(tail);
    const empty = !tail || tail.error !== undefined || !tail.lines.length;
    this.block.classList.toggle('output-note', empty);
    if (text === this.shown) return;
    this.shown = text;
    this.block.textContent = text;
    if (this.atEnd) this.block.scrollTop = this.block.scrollHeight;
  }
}

/**
 * Shows the Output sections of one strip: per actor, its tasks. Each task's view is kept by path
 * across documents, so its scroll and a focused Follow button survive.
 */
export class OutputArea {
  readonly element = div('output-area');
  private readonly sections = new KeyedCache<HTMLElement>();
  private readonly tasks = new KeyedCache<TaskView>();
  private readonly tails = new Map<string, Tail>();

  /**
   * Shows the outputs of `actors` at `nowMs`, in their order. An actor without outputs has no
   * section.
   */
  update(actors: Actor[], nowMs: number): void {
    const listed = new Set<string>();
    const withOutputs = actors.filter((actor) => (actor.outputs ?? []).length > 0);
    const sections = withOutputs.map((actor) => {
      const section = this.section(actor);
      const tasks: HTMLElement[] = [];
      for (const output of actor.outputs ?? []) {
        if (listed.has(output.path)) continue;
        listed.add(output.path);
        const task = this.tasks.obtain(output.path, () => new TaskView(output.path));
        task.update(output, actor.label, nowMs);
        task.show(this.tails.get(output.path));
        tasks.push(task.element);
      }
      if (section.firstElementChild) placeInOrder(section.firstElementChild, tasks);
      return section;
    });
    placeInOrder({ firstIn: this.element }, sections);
    this.tasks.retain(listed, (task) => {
      task.element.remove();
      this.tails.delete(task.path);
    });
    this.sections.retain(
      new Set(withOutputs.map((actor) => actor.key)),
      (section) => void section.remove(),
    );
    this.element.hidden = listed.size === 0;
  }

  /** Recomputes the run times of the running tasks. */
  tick(nowMs: number): void {
    this.tasks.forEach((task) => void task.tick(nowMs));
  }

  /** Shows the tails the extension read for the listed tasks, and ignores any other. */
  showTails(tails: Tail[]): void {
    for (const tail of tails) {
      const task = this.tasks.get(tail.path);
      if (!task) continue;
      this.tails.set(tail.path, tail);
      task.show(tail);
    }
  }

  /** Forgets every tail: another row is selected, whose tails come anew. */
  clearTails(): void {
    this.tails.clear();
    this.tasks.forEach((task) => void task.show());
  }

  /** Gives the Output section of `actor`, made on first use, with its label current. */
  private section(actor: Actor): HTMLElement {
    const section = this.sections.obtain(actor.key, () => {
      const made = document.createElement('section');
      made.className = 'outputs';
      const heading = div('outputs-heading');
      heading.append(document.createTextNode(`Output${DOT}`), bdi('', 'outputs-label'));
      made.append(heading);
      return made;
    });
    const label = section.querySelector('.outputs-label');
    if (label) setText(label, visible(actor.label));
    return section;
  }
}
