// The Output section of the detail strip (U9): per actor with outputs (S13), each task's summary as
// a heading with `running` or `ended HH:MM`, a Follow button (X19), then the tail the extension
// read (R12) in a monospace block. Each task's view is made once and kept while the document lists
// it, so its scroll position and a focused Follow button survive every document (X15). The block
// follows the end as lines arrive unless the user scrolled it up. Text reaches the page only
// through textContent, each line as X10 shows control characters.
import type { Actor, Output } from '../generated/status';
import { visible } from '../shared/invisible';
import type { Tail } from '../shared/protocol';
import { bdi, div, setText, span } from './dom';
import { DOT, clock } from './text';

/** The pixels from the end that a block may be scrolled up and still count as at the end. */
const AT_END_PX = 4;

/** Gives the words after a task's summary: `running`, or `ended HH:MM` in local time. */
function taskState(output: Output): string {
  return output.running || output.ended === null ? 'running' : `ended ${clock(output.ended)}`;
}

/** Gives the text of a tail block: the lines, the reason there are none, or that none came yet. */
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
  private readonly button = document.createElement('button');
  private readonly block = document.createElement('pre');
  private shown?: string;
  /** True while the block follows the end: it was at the end when the user last scrolled it. */
  private atEnd = true;

  constructor(readonly path: string) {
    this.element.setAttribute('data-output-path', path);
    const heading = div('output-heading');
    heading.append(this.summary, document.createTextNode(DOT), this.state);
    this.button.type = 'button';
    this.button.className = 'follow';
    this.button.setAttribute('data-follow', path);
    this.button.textContent = 'Follow';
    this.block.className = 'output-tail';
    // A block that scrolls is reached by the keyboard too, so its lines can be scrolled (X14).
    this.block.tabIndex = 0;
    this.block.addEventListener('scroll', () => {
      this.atEnd = this.block.scrollTop + this.block.clientHeight >= this.block.scrollHeight - AT_END_PX;
    });
    const top = div('output-top');
    top.append(heading, this.button);
    this.element.append(top, this.block);
    this.show();
  }

  /** Shows the summary and state of `output`, listed by the actor `label`. */
  update(output: Output, label: string): void {
    setText(this.summary, visible(output.summary || 'background task'));
    setText(this.state, taskState(output));
    const name = `${visible(output.summary || 'background task')} of ${visible(label)}`;
    if (this.button.getAttribute('aria-label') !== `Follow ${name} in a terminal`) {
      this.button.setAttribute('aria-label', `Follow ${name} in a terminal`);
      this.block.setAttribute('aria-label', `Output of ${name}`);
    }
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

/** Shows the Output sections of one strip: per actor, its tasks, kept by path across documents. */
export class OutputArea {
  readonly element = div('output-area');
  private readonly sections = new Map<string, HTMLElement>();
  private readonly tasks = new Map<string, TaskView>();
  private readonly tails = new Map<string, Tail>();

  /** Shows the outputs of `actors`, in their order; an actor without outputs has no section. */
  update(actors: Actor[]): void {
    const listed = new Set<string>();
    const keep = new Set<string>();
    let previous: Element | null = null;
    for (const actor of actors) {
      const outputs = actor.outputs ?? [];
      if (!outputs.length) continue;
      keep.add(actor.key);
      const section = this.section(actor);
      const after: Element | null = previous;
      const misplaced = after ? after.nextElementSibling !== section : this.element.firstElementChild !== section;
      // A section goes after the previous one, or first when it is the strip's first.
      if (misplaced && after) after.after(section);
      if (misplaced && !after) this.element.prepend(section);
      previous = section;
      let last: Element | null = section.firstElementChild;
      for (const output of outputs) {
        if (listed.has(output.path)) continue;
        listed.add(output.path);
        const task = this.task(output.path);
        task.update(output, actor.label);
        task.show(this.tails.get(output.path));
        if (last?.nextElementSibling !== task.element) last?.after(task.element);
        last = task.element;
      }
    }
    for (const [path, task] of this.tasks) {
      if (listed.has(path)) continue;
      task.element.remove();
      this.tasks.delete(path);
      this.tails.delete(path);
    }
    for (const [key, section] of this.sections) {
      if (keep.has(key)) continue;
      section.remove();
      this.sections.delete(key);
    }
    this.element.hidden = listed.size === 0;
  }

  /** Shows the tails the extension read for the listed tasks; others are ignored. */
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
    this.tasks.forEach((task) => task.show());
  }

  /** Gives the Output section of `actor`, made on first use, with its label current. */
  private section(actor: Actor): HTMLElement {
    let section = this.sections.get(actor.key);
    if (!section) {
      section = document.createElement('section');
      section.className = 'outputs';
      const heading = div('outputs-heading');
      heading.append(document.createTextNode(`Output${DOT}`), bdi('', 'outputs-label'));
      section.append(heading);
      this.sections.set(actor.key, section);
    }
    const label = section.querySelector('.outputs-label');
    if (label) setText(label, visible(actor.label));
    return section;
  }

  /** Gives the view of the task at `path`, made on first use. */
  private task(path: string): TaskView {
    let task = this.tasks.get(path);
    if (!task) {
      task = new TaskView(path);
      this.tasks.set(path, task);
    }
    return task;
  }
}
