import type { Pool } from '../generated/status';
import { visible } from '../shared/invisible';
import { scopePatterns, type Jobs } from '../shared/jobs';
import { bdi, cell, setAttr, setText, span, textSpan } from './dom';
import { DOT } from './text';

function repositoryName(pool: Pool): string {
  return pool.repository.github ?? pool.repository.name;
}

/** One count of the machine button: its words, and the class of the colour it takes. */
interface Count {
  text: string;
  className?: string;
}

/** The suggestions the button names: only until this repository lists its first pattern. */
function shownSuggestions(jobs: Jobs): number {
  return scopePatterns(jobs, 'repository').length ? 0 : jobs.suggested.length;
}

/** The counts of the machine button after the machine name, as shown. */
function counts(jobs: Jobs | undefined): Count[] {
  if (!jobs) return [];
  if (jobs.error !== null) return [{ text: 'jobs', className: 'jobs-problem' }];
  const shown: Count[] = [{ text: `jobs ${jobs.running.length}/${jobs.limit}` }];
  const outside = jobs.enforce === 'off' ? 0 : jobs.outside.length;
  const suggested = shownSuggestions(jobs);
  if (jobs.waiting.length) shown.push({ text: `${jobs.waiting.length} queued`, className: 'jobs-attention' });
  if (outside) shown.push({ text: `${outside} not queued`, className: 'jobs-attention' });
  if (suggested) shown.push({ text: `${suggested} suggested`, className: 'jobs-muted' });
  return shown;
}

/**
 * The machine button's accessible name: its visible text first, so speech input finds it by what it
 * shows, then the counts in words.
 */
function spokenName(machine: string, jobs: Jobs | undefined): string {
  const shown = [machine, ...counts(jobs).map((count) => count.text)].join(DOT);
  const name = `${shown}, heavy jobs on ${machine}`;
  if (!jobs) return name;
  if (jobs.error !== null) return `${name}: the settings cannot be read`;
  const parts = [`${jobs.running.length} of ${jobs.limit} running`];
  for (const count of counts(jobs).slice(1)) {
    parts.push(count.text);
  }
  return `${name}: ${parts.join(', ')}`;
}

/** Shows the heading row of the tab's pool: a repo icon, the repository and the machine button. */
export class PoolHeading {
  readonly element = document.createElement('tr');
  /** Opens and closes the machine strip, and names this machine's heavy jobs. */
  readonly button = document.createElement('button');
  private readonly repository = bdi('', 'repository');
  private key = '';

  /** Builds the heading. `toggle` runs when the machine button is pressed. */
  constructor(columns: number, toggle: () => void) {
    this.element.className = 'pool-heading';
    const heading = cell('');
    heading.colSpan = columns;
    const icon = span('repo-icon');
    icon.setAttribute('aria-hidden', 'true');
    this.button.type = 'button';
    this.button.className = 'machine-button';
    this.button.setAttribute('aria-expanded', 'false');
    this.button.addEventListener('click', toggle);
    // The separator and the machine button read as one muted part after the repository.
    const where = span('heading-machine');
    where.append(document.createTextNode(DOT), this.button);
    heading.append(icon, this.repository, where);
    this.element.append(heading);
  }

  update(pool: Pool, machine: string, jobs: Jobs | undefined): void {
    const name = visible(machine);
    setAttr(this.element, 'data-machine', name);
    setAttr(this.element, 'data-pool', pool.common_dir);
    setText(this.repository, visible(repositoryName(pool)));
    const shown = counts(jobs);
    const key = JSON.stringify([name, shown]);
    if (key !== this.key) {
      this.key = key;
      const parts = shown.flatMap(({ text, className }) => [
        document.createTextNode(DOT),
        className ? textSpan(className, text) : document.createTextNode(text),
      ]);
      this.button.replaceChildren(bdi(name), ...parts);
    }
    setAttr(this.button, 'aria-label', spokenName(name, jobs));
  }

  /** Marks the machine strip open or closed. */
  expand(open: boolean): void {
    setAttr(this.button, 'aria-expanded', String(open));
  }
}
