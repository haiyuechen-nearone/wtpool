import type { Actor, Worktree } from '../generated/status';
import { visible } from '../shared/invisible';
import type { Tail } from '../shared/protocol';
import { bdi, div, line, pieceNodes, setText, span } from './dom';
import { OutputArea } from './tasks';
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
    this.first.replaceChildren(bdi(visible(actor.label), 'actor-label'), document.createTextNode(DOT), ...pieceNodes(state));
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
  private readonly noAgent = line([{ text: 'no agent' }], 'actor-state');
  private readonly actors = new Map<string, ActorLines>();
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

  /** Shows the facts of `worktree`, changing only the parts that differ from the strip's. */
  update(worktree: Worktree): void {
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
    this.updateActors(shownActors(worktree));
  }

  /** Shows the tails the extension read for row `row`, when that is the strip's row. */
  showTails(row: string, tails: Tail[]): void {
    if (row === this.row) this.outputs.showTails(tails);
  }

  /** Shows the lines of `actors`, kept by key and moved into their order, then their outputs. */
  private updateActors(actors: Actor[]): void {
    this.noAgent.hidden = actors.length > 0;
    const keys = new Set(actors.map((actor) => actor.key));
    for (const [key, lines] of this.actors) {
      if (keys.has(key)) continue;
      lines.element.remove();
      this.actors.delete(key);
    }
    let previous: Element = this.noAgent;
    for (const actor of actors) {
      let lines = this.actors.get(actor.key);
      if (!lines) {
        lines = new ActorLines();
        this.actors.set(actor.key, lines);
      }
      lines.update(actor);
      if (previous.nextElementSibling !== lines.element) previous.after(lines.element);
      previous = lines.element;
    }
    this.outputs.update(actors);
  }
}
