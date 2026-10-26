import type { Pool } from '../generated/status';
import { visible } from '../shared/invisible';
import { bdi, cell, setText, span } from './dom';
import { DOT } from './text';

function repositoryName(pool: Pool): string {
  return pool.repository.github ?? pool.repository.name;
}

/** Shows the heading row of the tab's pool: a repo icon, the repository and the machine. */
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

  update(pool: Pool, machine: string): void {
    const name = visible(machine);
    if (this.element.getAttribute('data-machine') !== name) this.element.setAttribute('data-machine', name);
    if (this.element.getAttribute('data-pool') !== pool.common_dir) this.element.setAttribute('data-pool', pool.common_dir);
    setText(this.repository, visible(repositoryName(pool)));
    setText(this.machine, name);
  }
}
