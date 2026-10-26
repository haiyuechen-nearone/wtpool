// The heading row of the tab's pool (U8): a repo icon, then `<repository> · <machine>`. The
// repository is its GitHub `owner/repo`, or else the name of its main worktree's folder; the
// machine is the document's machine name. The pool's rows follow it. The row is
// `tr.pool-heading[data-machine][data-pool]`, one cell across every column, kept whole (U1).
import type { Pool } from '../generated/status';
import { visible } from '../shared/invisible';
import { bdi, cell, setText, span } from './dom';
import { DOT } from './text';

/** Names a pool's repository as the heading does: GitHub's `owner/repo`, else the folder name. */
function repositoryName(pool: Pool): string {
  return pool.repository.github ?? pool.repository.name;
}

/** Shows the heading row of the tab's pool (U8). */
export class PoolHeading {
  readonly element = document.createElement('tr');
  private readonly repository = bdi('', 'repository');
  private readonly machine = bdi('');

  constructor(columns: number) {
    this.element.className = 'pool-heading';
    const heading = cell('');
    heading.colSpan = columns;
    const icon = span('repo-icon');
    icon.setAttribute('aria-hidden', 'true');
    // The separator and the machine name read as one muted part after the repository.
    const where = span('heading-machine');
    where.append(document.createTextNode(DOT), this.machine);
    heading.append(icon, this.repository, where);
    this.element.append(heading);
  }

  /** Shows `pool` on `machine`, both as X10 shows document strings. */
  update(pool: Pool, machine: string): void {
    const name = visible(machine);
    if (this.element.getAttribute('data-machine') !== name) this.element.setAttribute('data-machine', name);
    if (this.element.getAttribute('data-pool') !== pool.common_dir) this.element.setAttribute('data-pool', pool.common_dir);
    setText(this.repository, visible(repositoryName(pool)));
    setText(this.machine, name);
  }
}
