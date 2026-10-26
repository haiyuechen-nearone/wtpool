import { visible } from '../shared/invisible';
import { isPattern, type HeavyEdit, type HeavyPattern, type JobSettingsAnswer, type Jobs, type Scope, type Suggestion } from '../shared/jobs';
import { bdi, div, setText, span, textSpan } from './dom';
import { DOT } from './text';

const PATTERN_ERROR =
  'Not a pattern: 1 to 3 words of letters, digits and . _ + -, and : after the first, none starting with - or +; only the first may end in *.';
const DUPLICATE = 'Already in this list.';
const SCOPE_WORDS: Readonly<Record<Scope, string>> = { machine: 'this machine', repository: 'this repository' };

/** Sends the editor's edits and speaks its answers. */
export interface EditorHost {
  send(edit: HeavyEdit): void;
  announce(text: string): void;
}

function button(className: string, text = ''): HTMLButtonElement {
  const element = document.createElement('button');
  element.type = 'button';
  element.className = className;
  element.textContent = text;
  return element;
}

/** The index a key moves focus to among `count` buttons from `at`, or undefined for another key. */
function moveTo(key: string, at: number, count: number): number | undefined {
  switch (key) {
    case 'ArrowLeft':
    case 'ArrowUp':
      return Math.max(0, at - 1);
    case 'ArrowRight':
    case 'ArrowDown':
      return Math.min(count - 1, at + 1);
    case 'Home':
      return 0;
    case 'End':
      return count - 1;
    default:
      return undefined;
  }
}

/**
 * A list of chips that is one tab stop: focus enters on the button focused last, and the arrows,
 * Home and End move it among the buttons. A press, or Delete and Backspace where `removes`, acts on
 * the chip.
 */
class Chips {
  readonly element = document.createElement('ul');
  /** One key per button, in their order. */
  private keys: string[] = [];

  constructor(
    className: string,
    private readonly removes: boolean,
    private readonly act: (index: number) => void,
  ) {
    this.element.className = className;
    this.element.setAttribute('role', 'list');
    this.element.hidden = true;
    this.element.addEventListener('focusin', (event) => void this.rove(this.indexOf(event.target)));
    this.element.addEventListener('click', (event) => {
      const at = this.indexOf(event.target instanceof Element ? event.target.closest('button') : null);
      if (at >= 0) this.act(at);
    });
    this.element.addEventListener('keydown', (event) => void this.keydown(event));
  }

  /**
   * Shows the chips of `keys`, made by `chips`, when they changed. Focus stays on its chip, else
   * moves to the next one, else the previous, else to `fallback`.
   */
  fill(keys: string[], label: string, chips: () => Node[], fallback: () => HTMLElement): void {
    if (this.element.getAttribute('aria-label') !== label) this.element.setAttribute('aria-label', label);
    if (JSON.stringify(keys) === JSON.stringify(this.keys)) return;
    const before = this.buttons();
    const focused = before.findIndex((b) => b === document.activeElement);
    const roving = Math.max(0, before.findIndex((b) => b.tabIndex === 0));
    const stay = (at: number): number => (keys.includes(this.keys[at]) ? keys.indexOf(this.keys[at]) : Math.min(at, keys.length - 1));
    const rovingAt = stay(roving);
    const focusAt = focused >= 0 ? stay(focused) : -2;
    this.keys = keys;
    this.element.replaceChildren(...chips());
    this.element.hidden = !keys.length;
    this.rove(rovingAt);
    if (focusAt >= 0) this.buttons()[focusAt].focus();
    if (focusAt === -1) fallback().focus();
  }

  private buttons(): HTMLButtonElement[] {
    return [...this.element.querySelectorAll('button')];
  }

  private indexOf(target: EventTarget | null): number {
    return this.buttons().findIndex((b) => b === target);
  }

  /** Makes the button at `index` the list's one tab stop. */
  private rove(index: number): void {
    this.buttons().forEach((b, at) => void b.setAttribute('tabindex', at === index ? '0' : '-1'));
  }

  private keydown(event: KeyboardEvent): void {
    const buttons = this.buttons();
    const at = this.indexOf(event.target);
    if (at < 0) return;
    const to = moveTo(event.key, at, buttons.length);
    if (to !== undefined) {
      event.preventDefault();
      buttons[to].focus();
    } else if (this.removes && (event.key === 'Delete' || event.key === 'Backspace')) {
      event.preventDefault();
      this.act(at);
    }
  }
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
  private readonly error = document.createElement('p');
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
    this.error.className = 'heavy-error';
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
    setText(this.error, text);
    this.error.hidden = !text;
    this.input.setAttribute('aria-invalid', String(Boolean(text)));
  }

  /** Clears the input once the pattern it still holds was added. */
  added(pattern: string): void {
    if (this.input.value === pattern) this.input.value = '';
  }

  private submit(): void {
    const pattern = this.input.value;
    const listed = this.entries.some((e) => e.pattern === pattern);
    const problem = !isPattern(pattern) ? PATTERN_ERROR : listed ? DUPLICATE : '';
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
  /** The words to announce once the answer to the last edit of each list comes. */
  private readonly asked = new Map<Scope, { said: string; pattern: string | null }>();

  constructor(private readonly host: EditorHost) {
    const send = (edit: HeavyEdit, said: string): void => {
      this.asked.set(edit.scope, { said, pattern: edit.pattern });
      this.host.send(edit);
    };
    this.scopes = {
      machine: new ScopeEditor('machine', [document.createTextNode('This machine')], send),
      repository: new ScopeEditor('repository', [document.createTextNode(`This repository${DOT}`), bdi('.git/wtpool.toml')], send),
    };
    this.suggestions = new Suggestions(send, () => this.scopes.repository.input);
    this.element.append(this.scopes.machine.element, this.scopes.repository.element);
  }

  show(jobs: Jobs): void {
    this.scopes.machine.show(jobs.heavy.filter((e) => e.source === 'machine'));
    this.scopes.repository.show(jobs.heavy.filter((e) => e.source !== 'machine'));
    this.suggestions.show(jobs.suggested);
    if (jobs.suggested.length) {
      if (!this.suggestions.element.isConnected) this.element.append(this.suggestions.element);
    } else {
      if (this.suggestions.element.contains(document.activeElement)) this.scopes.repository.input.focus();
      this.suggestions.element.remove();
    }
  }

  /** Shows the answer to an edit of the list `scope`: announced, and an error under its input. */
  answer(answer: JobSettingsAnswer, scope: Scope): void {
    const asked = this.asked.get(scope);
    this.asked.delete(scope);
    const editor = this.scopes[scope];
    const said = answer.ok ? (asked?.said ?? '') : visible(answer.error ?? '');
    editor.showError(answer.ok ? '' : said);
    if (answer.ok && asked?.pattern) editor.added(asked.pattern);
    if (said) this.host.announce(said);
  }
}
