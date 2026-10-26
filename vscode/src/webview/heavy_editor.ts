import { visible } from '../shared/invisible';
import {
  isPattern,
  PATTERN_RULE,
  scopeEntries,
  waitEnds,
  type Awaiting,
  type HeavyEdit,
  type HeavyMessage,
  type HeavyPattern,
  type JobSettingsAnswer,
  type Jobs,
  type Scope,
  type Suggestion,
} from '../shared/jobs';
import { Chips } from './chips';
import { bdi, button, div, paragraph, setAttr, setShownText, span, textSpan } from './dom';
import { DOT } from './text';

const DUPLICATE = 'Already in this list.';
const SCOPES: readonly Scope[] = ['machine', 'repository'];
const SCOPE_WORDS: Readonly<Record<Scope, string>> = { machine: 'this machine', repository: 'this repository' };

/** Sends the editor's edits and speaks its answers. */
export interface EditorHost {
  /** Sends `message` and gives the time its answer is due by. */
  send(message: HeavyMessage): number;
  announce(text: string): void;
}

/** An edit sent and not yet answered, with the words its answer announces. */
interface Asked extends Awaiting {
  message: HeavyMessage;
  said: string;
}

/** A chip: its pattern in monospace, a tag, and its button. */
function chip(pattern: string, action: HTMLButtonElement, tag?: string): HTMLLIElement {
  const item = document.createElement('li');
  item.className = 'heavy-chip';
  item.append(bdi(visible(pattern), 'chip-pattern'));
  if (tag) item.append(document.createTextNode(' '), textSpan('heavy-tag', tag));
  item.append(action);
  return item;
}

/** One list of the editor, this machine's or this repository's: its chips, an input and Add. */
class ScopeEditor {
  readonly element = div('heavy-scope');
  readonly input = document.createElement('input');
  private readonly chips: Chips;
  private readonly none = textSpan('heavy-none', 'none');
  private readonly error = paragraph('heavy-error');
  private entries: HeavyPattern[] = [];

  constructor(
    private readonly scope: Scope,
    title: Node[],
    private readonly send: (edit: HeavyEdit, said: string) => void,
  ) {
    this.chips = new Chips('heavy-chips', true, (at) => {
      const { pattern } = this.entries[at];
      this.send({ op: 'rm', scope, pattern }, `Removed ${visible(pattern)}.`);
    });
    this.chips.element.setAttribute('data-scope', scope);
    this.element.setAttribute('data-scope', scope);
    const source = span('heavy-source');
    source.append(...title);
    const heading = div('heavy-list-heading');
    heading.append(source);
    this.input.type = 'text';
    this.input.className = 'heavy-input';
    this.input.setAttribute('data-scope', scope);
    this.input.setAttribute('placeholder', 'add a pattern, such as go test');
    this.input.setAttribute('aria-label', `Add a heavy command to ${SCOPE_WORDS[scope]}`);
    this.input.setAttribute('aria-describedby', `heavy-error-${scope}`);
    this.input.setAttribute('spellcheck', 'false');
    const add = button('job-button secondary small heavy-add', 'Add');
    const row = div('heavy-add-row');
    row.append(this.input, add);
    this.error.id = `heavy-error-${scope}`;
    this.error.hidden = true;
    this.element.append(heading, this.chips.element, this.none, row, this.error);
    add.addEventListener('click', () => void this.submit());
    this.input.addEventListener('input', () => void this.showError(''));
    this.input.addEventListener('keydown', (event) => {
      if (event.key !== 'Enter') return;
      event.preventDefault();
      this.submit();
    });
  }

  show(entries: HeavyPattern[]): void {
    this.entries = entries;
    const words = SCOPE_WORDS[this.scope];
    this.chips.fill(
      entries.map((e) => `${e.source}\n${e.pattern}`),
      `Heavy commands of ${words}, ${entries.length}`,
      () =>
        entries.map((e) => {
          const remove = button('chip-button chip-remove');
          remove.setAttribute('aria-label', `Remove ${visible(e.pattern)} from ${words}`);
          return chip(e.pattern, remove, e.source === 'build_processes' ? 'build_processes' : undefined);
        }),
      () => this.input,
    );
    this.none.hidden = entries.length > 0;
  }

  /** Shows `text` under the input, or nothing for the empty string. */
  showError(text: string): void {
    setShownText(this.error, text);
    setAttr(this.input, 'aria-invalid', String(Boolean(text)));
  }

  /** Clears the input once the pattern it still holds was added. */
  added(pattern: string): void {
    if (this.input.value === pattern) this.input.value = '';
  }

  private submit(): void {
    const pattern = this.input.value;
    const listed = this.entries.some((e) => e.pattern === pattern);
    const problem = !isPattern(pattern) ? PATTERN_RULE : listed ? DUPLICATE : '';
    this.showError(problem);
    if (!problem) this.send({ op: 'add', scope: this.scope, pattern }, `Added ${pattern}.`);
  }
}

/** The label of a group of suggestions: the marker they came from. */
function groupLabel(from: string): Node[] {
  if (from === 'package.json') return [document.createTextNode('from package.json scripts')];
  return [document.createTextNode('from '), bdi(visible(from))];
}

/** This repository's suggestions, grouped by the marker they came from, with `Add all`. */
class Suggestions {
  readonly element = div('heavy-suggestions');
  private readonly chips: Chips;
  private suggested: Suggestion[] = [];

  constructor(
    private readonly send: (edit: HeavyEdit, said: string) => void,
    private readonly fallback: () => HTMLElement,
  ) {
    this.chips = new Chips('heavy-suggested', false, (at) => {
      const { pattern } = this.suggested[at];
      this.send({ op: 'add', scope: 'repository', pattern }, `Added ${visible(pattern)}.`);
    });
    const heading = div('heavy-list-heading');
    heading.append(textSpan('heavy-source', 'Suggested for this repository'));
    const all = button('job-button secondary small suggest-all', 'Add all');
    all.addEventListener('click', () => {
      const count = this.suggested.filter((s) => isPattern(s.pattern)).length;
      this.send({ op: 'add-suggested', scope: 'repository', pattern: null }, `Added ${count} suggestions.`);
    });
    const row = div('heavy-add-row');
    row.append(all, textSpan('heavy-note', 'Nothing is heavy until you add it.'));
    this.element.append(heading, this.chips.element, row);
  }

  show(suggested: Suggestion[]): void {
    const groups = new Map<string, Suggestion[]>();
    for (const entry of suggested) {
      groups.set(entry.from, [...(groups.get(entry.from) ?? []), entry]);
    }
    this.suggested = [...groups.values()].flat();
    this.chips.fill(
      this.suggested.map((s) => `${s.from}\n${s.pattern}`),
      `Suggested for this repository, ${suggested.length}`,
      () => [...groups].map(([from, entries]) => this.group(from, entries)),
      this.fallback,
    );
  }

  private group(from: string, entries: Suggestion[]): HTMLLIElement {
    const item = document.createElement('li');
    item.className = 'suggest-group';
    const label = span('suggest-group-label');
    label.append(...groupLabel(from));
    const list = document.createElement('ul');
    list.className = 'suggest-chips';
    list.setAttribute('role', 'list');
    list.setAttribute('aria-label', label.textContent ?? '');
    list.append(
      ...entries.map((entry) => {
        const add = button('chip-button suggest-add');
        add.setAttribute('aria-label', `Add ${visible(entry.pattern)} to this repository`);
        const found = chip(entry.pattern, add);
        found.classList.add('suggested');
        found.setAttribute('data-from', entry.from);
        return found;
      }),
    );
    item.append(label, list);
    return item;
  }
}

/**
 * The editor of the heavy patterns: this machine's list, this repository's, and the repository's
 * suggestions while it has any. Each edit is sent at once, and the next document shows it.
 */
export class HeavyEditor {
  readonly element = div('heavy-editor');
  private readonly scopes: Readonly<Record<Scope, ScopeEditor>>;
  private readonly suggestions: Suggestions;
  /** The unanswered edits of each list, oldest first, as the extension answers in order. */
  private readonly asked: Record<Scope, Asked[]> = { machine: [], repository: [] };

  constructor(private readonly host: EditorHost) {
    const send = (edit: HeavyEdit, said: string): void => {
      const message: HeavyMessage = { type: 'jobSettings', heavy: edit };
      this.asked[edit.scope].push({ message, said, due: this.host.send(message) });
    };
    this.scopes = {
      machine: new ScopeEditor('machine', [document.createTextNode('This machine')], send),
      repository: new ScopeEditor('repository', [document.createTextNode(`This repository${DOT}`), bdi('.git/wtpool.toml')], send),
    };
    this.suggestions = new Suggestions(send, () => this.scopes.repository.input);
    this.element.append(this.scopes.machine.element, this.scopes.repository.element);
  }

  show(jobs: Jobs, nowMs: number): void {
    this.scopes.machine.show(scopeEntries(jobs, 'machine'));
    this.scopes.repository.show(scopeEntries(jobs, 'repository'));
    this.suggestions.show(jobs.suggested);
    if (jobs.suggested.length) {
      if (!this.suggestions.element.isConnected) this.element.append(this.suggestions.element);
    } else {
      if (this.suggestions.element.contains(document.activeElement)) this.scopes.repository.input.focus();
      this.suggestions.element.remove();
    }
    this.settle(jobs, nowMs);
  }

  /** Stops waiting for the answers that are overdue. */
  tick(nowMs: number): void {
    this.settle(undefined, nowMs);
  }

  /** Stops waiting for the answers of edits that are overdue, or that `jobs` has shown a while. */
  private settle(jobs: Jobs | undefined, nowMs: number): void {
    for (const scope of SCOPES) {
      this.asked[scope] = this.asked[scope].filter((asked) => !waitEnds(asked, asked.message, jobs, nowMs));
    }
  }

  /** Shows the answer to an edit of the list `scope`: announced, and an error under its input. */
  answer(answer: JobSettingsAnswer, scope: Scope): void {
    const asked = this.asked[scope].shift();
    const editor = this.scopes[scope];
    const said = answer.ok ? (asked?.said ?? '') : visible(answer.error ?? '');
    editor.showError(answer.ok ? '' : said);
    if (answer.ok && asked?.message.heavy.pattern) editor.added(asked.message.heavy.pattern);
    if (said) this.host.announce(said);
  }
}
