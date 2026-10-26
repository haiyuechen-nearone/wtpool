import type { Worktree } from '../generated/status';
import { visible } from '../shared/invisible';
import { offeredActions, type ActionType } from '../shared/protocol';
import { bdi, cell, line, setText, span } from './dom';
import { GROUPS, WORDS, age, agentLines, gitText, lastTimes, shownActors } from './text';

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
 * Shows one worktree as a row of the table. It is made once and updated in place, so focus and
 * selection survive every document.
 */
export class RowView {
  readonly element: HTMLTableRowElement;
  private current: Worktree;
  private readonly icon = span('icon');
  private readonly word = span('word');
  private readonly name = bdi('', 'name');
  private readonly role = span('role');
  private readonly branch = bdi('');
  private readonly git = bdi('');
  private readonly agent = cell('agent');
  private readonly last = cell('last');
  private readonly actions = cell('actions');
  private agentKey = '';
  private times: Array<string | null> = [];
  private actionKey = '';

  constructor(worktree: Worktree, nowMs: number) {
    this.current = worktree;
    this.element = document.createElement('tr');
    this.element.setAttribute('data-row-id', worktree.path);
    this.element.setAttribute('tabindex', '0');
    this.element.setAttribute('aria-selected', 'false');
    this.icon.setAttribute('aria-hidden', 'true');
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
    this.update(worktree, nowMs);
  }

  /** Gives the row's id: its worktree's path. */
  get id(): string {
    return this.current.path;
  }

  get worktree(): Worktree {
    return this.current;
  }

  /** Shows `worktree`, changing only the parts that differ from the row's. */
  update(worktree: Worktree, nowMs: number): void {
    this.current = worktree;
    const className = `row activity-${worktree.activity.replaceAll('_', '-')} group-${GROUPS[worktree.activity]}`;
    if (this.element.className !== className) this.element.className = className;
    this.updateIcon(worktree.activity === 'working');
    setText(this.word, WORDS[worktree.activity]);
    setText(this.name, visible(worktree.name));
    const role = ROLE_TAGS[worktree.role];
    setText(this.role, role);
    this.role.hidden = !role;
    setText(this.branch, worktree.branch === null ? '(detached)' : visible(worktree.branch));
    this.branch.classList.toggle('detached', worktree.branch === null);
    setText(this.git, gitText(worktree.git));
    this.updateAgent(worktree);
    this.updateLast(worktree, nowMs);
    this.updateActions(offeredActions(worktree));
  }

  /** Recomputes the ages of the Last cell, nothing else. */
  tick(nowMs: number): void {
    this.times.forEach((time, index) => void setText(this.last.children[index], age(time, nowMs)));
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
  private updateAgent(worktree: Worktree): void {
    const lines = agentLines(worktree);
    const states = shownActors(worktree).map((actor) => actor.state);
    const key = JSON.stringify([lines, states]);
    if (key === this.agentKey) return;
    this.agentKey = key;
    this.agent.replaceChildren(
      ...lines.map((pieces, index) => line(pieces, `line state-${states[index] ?? 'none'}`)),
    );
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
