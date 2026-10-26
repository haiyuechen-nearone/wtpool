import type { Worktree } from '../generated/status';
import { visible } from '../shared/invisible';
import type { Job } from '../shared/jobs';
import { bdi, div, setText, span, textSpan } from './dom';
import { DOT, age } from './text';

/** Gives the worktree a job runs in, with its actor's label, or `outside this pool`. */
function place(job: Job, worktrees: Worktree[]): string {
  const worktree = worktrees.find((w) => w.path === job.worktree);
  if (!worktree) return 'outside this pool';
  const actor = worktree.actors.find((a) => a.key === job.actor);
  return actor ? `${visible(worktree.name)}${DOT}${visible(actor.label)}` : visible(worktree.name);
}

/**
 * Shows one list of jobs under its heading: per job its position when `numbered`, its label or
 * summary, its place and its age; `none` while it is empty.
 */
export class JobList {
  readonly element = div('jobs-group');
  private readonly heading = div('agents-heading');
  private readonly list = document.createElement('ol');
  private readonly none = textSpan('jobs-none', 'none');
  /** The age cells and the times they count from. */
  private ages: Array<[HTMLElement, string]> = [];
  private key = '';

  constructor(
    className: string,
    private readonly numbered: boolean,
  ) {
    this.list.className = `jobs-list ${className}`;
    this.element.append(this.heading, this.list, this.none);
  }

  show(title: string, jobs: Job[], worktrees: Worktree[], nowMs: number): void {
    setText(this.heading, title);
    const lines = jobs.map((job) => ({ id: job.id, name: visible(job.label ?? job.summary), place: place(job, worktrees), since: job.since }));
    const key = JSON.stringify(lines);
    if (key !== this.key) {
      this.key = key;
      this.ages = [];
      this.list.replaceChildren(
        ...lines.map((line, index) => {
          const item = document.createElement('li');
          item.setAttribute('data-job-id', line.id);
          const cells: HTMLElement[] = [bdi(line.name, 'job-summary'), bdi(line.place, 'job-where'), span('job-age')];
          if (this.numbered) cells.unshift(textSpan('job-position', String(index + 1)));
          // Spaces between the cells keep the line's words apart when it is read as text.
          item.append(...cells.flatMap((cell, at) => (at ? [document.createTextNode(' '), cell] : [cell])));
          this.ages.push([cells[cells.length - 1], line.since]);
          return item;
        }),
      );
    }
    this.list.hidden = !jobs.length;
    this.none.hidden = jobs.length > 0;
    this.tick(nowMs);
  }

  /** Recomputes the ages, nothing else. */
  tick(nowMs: number): void {
    for (const [cell, since] of this.ages) {
      setText(cell, age(since, nowMs));
    }
  }
}
