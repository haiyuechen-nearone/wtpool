import type { Actor, Worktree } from '../generated/status';
import { visible } from '../shared/invisible';
import type { Tail } from '../shared/protocol';
import { bdi, div, line, pieceNodes, setText, span } from './dom';
import { KeyedCache, placeInOrder } from './order';
import { OutputArea } from './output_area';
import { DOT, actorFacts, gitFacts, shownActors, stateText } from './text';

/** Shows one fact of the grid: a label and a value, isolated when it comes from the document. */
class Fact {
  readonly label = span('fact-label');
  readonly value: HTMLElement;

  constructor(isolated: boolean) {
    this.value = isolated ? bdi('', 'fact-value') : span('fact-value');
  }

  set(name: string, value: string): void {
    setText(this.label, name);
    setText(this.value, value);
  }
}

/** Shows one actor's lines: its label and state, then its action, placement and start. */
class ActorLines {
  readonly element = div('actor');
  private readonly first = div('actor-state');
  private readonly facts = div('actor-facts');
  private key = '';

  constructor() {
    this.element.append(this.first, this.facts);
  }

  /** Shows `actor`, rewriting its lines only when their text changed. */
  update(actor: Actor): void {
    const className = `actor state-${actor.state}`;
    if (this.element.className !== className) this.element.className = className;
    const state = stateText(actor);
    const facts = actorFacts(actor);
    const key = JSON.stringify([actor.label, state, facts]);
    if (key === this.key) return;
    this.key = key;
    this.first.replaceChildren(
      bdi(visible(actor.label), 'actor-label'),
      document.createTextNode(DOT),
      ...pieceNodes(state),
    );
    this.facts.replaceChildren(...pieceNodes(facts));
  }
}

/**
 * Shows the detail strip of the selected row. It is made once and updated in place, each part only
 * when its text changed, so focus, text selection and the scroll of an output block survive.
 */
export class DetailStrip {
  readonly element = div('strip');
  private readonly path = new Fact(true);
  private readonly upstream = new Fact(true);
  private readonly git = new Fact(false);
  private readonly free = new Fact(false);
  private readonly agents = div('agents');
  private readonly noAgent = line([{ kind: 'own', text: 'no agent' }], 'actor-state').element;
  private readonly actors = new KeyedCache<ActorLines>();
  private readonly outputs = new OutputArea();
  /** The row the strip shows. A strip moved to another row forgets its tails. */
  private row: string | null = null;

  constructor() {
    const facts = div('facts');
    for (const fact of [this.path, this.upstream, this.git, this.free]) {
      facts.append(fact.label, fact.value);
    }
    const heading = div('agents-heading');
    heading.textContent = 'Agents';
    this.agents.append(heading, this.noAgent);
    this.element.append(facts, this.agents, this.outputs.element);
  }

  /** Shows the facts of `worktree` at `nowMs`, changing only the parts that differ. */
  update(worktree: Worktree, nowMs: number): void {
    if (worktree.path !== this.row) {
      this.row = worktree.path;
      this.outputs.clearTails();
    }
    this.path.set('Path', visible(worktree.path));
    this.upstream.set('Upstream', worktree.upstream === null ? 'none' : visible(worktree.upstream));
    this.git.set('Git', gitFacts(worktree.git) || 'clean');
    if (worktree.free) {
      this.free.set('Free', 'wtpool bg may reuse it');
    } else {
      this.free.set('Not free', worktree.reasons.join(', ') || 'unknown');
    }
    this.updateActors(shownActors(worktree), nowMs);
  }

  /** Recomputes the run times of the strip's tasks. */
  tick(nowMs: number): void {
    this.outputs.tick(nowMs);
  }

  /** Shows the tails the extension read for row `row`, when that is the strip's row. */
  showTails(row: string, tails: Tail[]): void {
    if (row === this.row) this.outputs.showTails(tails);
  }

  /** Shows the lines of `actors`, kept by key and moved into their order, then their outputs. */
  private updateActors(actors: Actor[], nowMs: number): void {
    this.noAgent.hidden = actors.length > 0;
    this.actors.retain(
      new Set(actors.map((actor) => actor.key)),
      (lines) => void lines.element.remove(),
    );
    const shown = actors.map((actor) => {
      const lines = this.actors.obtain(actor.key, () => new ActorLines());
      lines.update(actor);
      return lines.element;
    });
    placeInOrder(this.noAgent, shown);
    this.outputs.update(actors, nowMs);
  }
}
