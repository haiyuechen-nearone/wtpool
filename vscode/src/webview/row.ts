import type { Actor, Worktree } from '../generated/status';
import { visible } from '../shared/invisible';
import { offeredActions, type ActionType } from '../shared/protocol';
import { bdi, cell, line, setText, span } from './dom';
import { GROUPS, RUN_TIME_CLASS, WORDS, age, agentLines, gitText, lastTimes, runTime, shownActors } from './text';

interface ActionButton {
  type: ActionType;
  label: string;
  classSuffix: string;
}

/** The buttons of the row actions, in the order a row shows them. */
const ACTIONS: readonly ActionButton[] = [
  { type: 'open', label: 'Open in new window', classSuffix: 'open' },
  { type: 'terminal', label: 'Open terminal', classSuffix: 'terminal' },
  { type: 'copyPath', label: 'Copy path', classSuffix: 'copy-path' },
  { type: 'free', label: 'Free this worktree…', classSuffix: 'free' },
  { type: 'fg', label: 'Bring its branch to the foreground…', classSuffix: 'fg' },
];

/** The tag after a worktree's name, by its role. */
const ROLE_TAGS: Readonly<Record<Worktree['role'], string>> = {
  foreground: 'fg',
  slot: '',
  unmanaged: 'unmanaged',
};

/** Copies the robot of the page's `<template id="robot">`, if it has one. */
function robot(): Node | null {
  const template = document.getElementById('robot');
  const drawn = template instanceof HTMLTemplateElement ? template.content.firstElementChild : null;
  return drawn ? document.importNode(drawn, true) : null;
}

/**
 * Makes the start of an agent line: the icon of `actor`'s state and that state for screen readers,
 * or an empty icon box for a line of no actor, so every line's text starts at the same x.
 */
function lineStart(actor: Actor | undefined): Node[] {
  const icon = span('line-icon');
  icon.setAttribute('aria-hidden', 'true');
  if (!actor) return [icon];
  const apiError = actor.state === 'idle' && Boolean(actor.error);
  if (actor.state === 'working') {
    const drawn = robot();
    if (drawn) icon.append(drawn);
  } else {
    icon.classList.add(`line-icon-${apiError ? 'error' : actor.state}`);
  }
  const state = span('visually-hidden');
  state.textContent = apiError ? 'idle, API error: ' : `${actor.state}: `;
  return [icon, state];
}

/**
 * Shows one worktree as a row of the table. It is made once and updated in place, so focus and
 * selection survive every document.
 */
export class RowView {
  readonly element: HTMLTableRowElement;
  private current: Worktree;
  private readonly icon = span('icon');
  private readonly word = span('word');
  private readonly count = span('count');
  private readonly agentCount = span('visually-hidden');
  private readonly name = bdi('', 'name');
  private readonly role = span('role');
  private readonly branch = bdi('');
  private readonly git = bdi('');
  private readonly agent = cell('agent');
  private readonly last = cell('last');
  private readonly actions = cell('actions');
  private agentKey = '';
  private runTimes: HTMLElement[] = [];
  private times: Array<string | null> = [];
  private actionKey = '';

  constructor(worktree: Worktree, nowMs: number, flagUnqueued: boolean) {
    this.current = worktree;
    this.element = document.createElement('tr');
    this.element.setAttribute('data-row-id', worktree.path);
    this.element.setAttribute('tabindex', '0');
    this.element.setAttribute('aria-selected', 'false');
    this.icon.setAttribute('aria-hidden', 'true');
    this.count.setAttribute('aria-hidden', 'true');
    const activity = cell('activity');
    activity.append(this.icon, this.word);
    const name = cell('worktree');
    name.append(this.name, document.createTextNode(' '), this.role);
    const branch = cell('branch');
    branch.append(this.branch);
    const git = cell('git');
    git.append(this.git);
    // Each text cell is a left to right isolate, and each string in it a <bdi> of its own.
    for (const text of [name, branch, git, this.agent]) {
      text.setAttribute('dir', 'ltr');
    }
    this.element.append(activity, name, branch, git, this.agent, this.last, this.actions);
    this.update(worktree, nowMs, flagUnqueued);
  }

  /** Gives the row's id: its worktree's path. */
  get id(): string {
    return this.current.path;
  }

  get worktree(): Worktree {
    return this.current;
  }

  /**
   * Shows `worktree`, changing only the parts that differ from the row's. With `flagUnqueued`, a
   * build that no job holds is marked.
   */
  update(worktree: Worktree, nowMs: number, flagUnqueued: boolean): void {
    this.current = worktree;
    const className = `row activity-${worktree.activity.replaceAll('_', '-')} group-${GROUPS[worktree.activity]}`;
    if (this.element.className !== className) this.element.className = className;
    this.updateIcon(worktree.activity === 'working');
    setText(this.word, WORDS[worktree.activity]);
    this.updateCount(worktree.actors.length);
    setText(this.name, visible(worktree.name));
    const role = ROLE_TAGS[worktree.role];
    setText(this.role, role);
    this.role.hidden = !role;
    setText(this.branch, worktree.branch === null ? '(detached)' : visible(worktree.branch));
    this.branch.classList.toggle('detached', worktree.branch === null);
    setText(this.git, gitText(worktree.git));
    this.updateAgent(worktree, flagUnqueued);
    this.updateLast(worktree, nowMs);
    this.updateActions(offeredActions(worktree));
  }

  /** Recomputes the ages of the Last cell and the run times of the agent lines, nothing else. */
  tick(nowMs: number): void {
    this.times.forEach((time, index) => void setText(this.last.children[index], age(time, nowMs)));
    this.runTimes.forEach((element) => void setText(element, runTime(element.dataset.since ?? '', nowMs)));
  }

  /** Shows the robot while working, else the codicon glyph of the icon's ::before. */
  private updateIcon(working: boolean): void {
    const shown = this.icon.firstChild;
    if (!working) {
      shown?.remove();
    } else if (!shown) {
      const drawn = robot();
      if (drawn) this.icon.append(drawn);
    }
  }

  /** Rewrites the agent cell when its lines or their states changed. */
  private updateAgent(worktree: Worktree, flagUnqueued: boolean): void {
    const lines = agentLines(worktree, flagUnqueued);
    const actors = shownActors(worktree);
    const states = actors.map((actor) => actor.state);
    const key = JSON.stringify([lines, states]);
    if (key === this.agentKey) return;
    this.agentKey = key;
    this.agent.replaceChildren(
      ...lines.map((pieces, index) => {
        const element = line(pieces, `line state-${states[index] ?? 'none'}`);
        element.prepend(...lineStart(actors.at(index)));
        return element;
      }),
    );
    this.runTimes = [...this.agent.querySelectorAll<HTMLElement>(`.${RUN_TIME_CLASS}`)];
  }

  /** Shows the number of agents after the Activity word when there are two or more. */
  private updateCount(agents: number): void {
    if (agents < 2) {
      this.count.remove();
      this.agentCount.remove();
      return;
    }
    setText(this.count, `×${agents}`);
    setText(this.agentCount, `, ${agents} agents`);
    if (this.word.nextSibling !== this.count) this.word.after(this.count, this.agentCount);
  }

  /** Remakes the Last cell's lines when its times changed, then shows their ages. */
  private updateLast(worktree: Worktree, nowMs: number): void {
    const times = lastTimes(worktree);
    if (JSON.stringify(times) !== JSON.stringify(this.times)) {
      this.times = times;
      this.last.replaceChildren(...times.map(() => line([])));
    }
    this.tick(nowMs);
  }

  /** Remakes the action buttons when the offered actions changed. */
  private updateActions(actions: ActionType[]): void {
    const key = actions.join();
    if (key === this.actionKey) return;
    this.actionKey = key;
    this.actions.replaceChildren(
      ...ACTIONS.filter((action) => actions.includes(action.type)).map(({ type, label, classSuffix }) => {
        const button = document.createElement('button');
        button.type = 'button';
        button.className = `action action-${classSuffix}`;
        button.setAttribute('data-action', type);
        button.setAttribute('aria-label', label);
        button.setAttribute('title', label);
        return button;
      }),
    );
  }
}
