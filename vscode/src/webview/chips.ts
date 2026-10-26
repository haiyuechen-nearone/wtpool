import { setAttr } from './dom';

/** The index {@link Chips.fill} gives a focused chip once no chip is left. */
const NONE_LEFT = -1;

/** Gives the index a key moves focus to among `count` buttons from `at`, or undefined for another key. */
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
export class Chips {
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
      const at = this.indexOf(
        event.target instanceof Element ? event.target.closest('button') : null,
      );
      if (at >= 0) this.act(at);
    });
    this.element.addEventListener('keydown', (event) => void this.keydown(event));
  }

  /**
   * Shows the chips of `keys`, made by `chips`, when they changed. Focus stays on its chip, else
   * moves to the next one, else the previous, else to `fallback`.
   */
  fill(keys: string[], label: string, chips: () => Node[], fallback: () => HTMLElement): void {
    setAttr(this.element, 'aria-label', label);
    if (JSON.stringify(keys) === JSON.stringify(this.keys)) return;
    const before = this.buttons();
    const focused = before.findIndex((b) => b === document.activeElement);
    const roving = Math.max(
      0,
      before.findIndex((b) => b.tabIndex === 0),
    );
    const stay = (at: number): number => {
      if (keys.includes(this.keys[at])) return keys.indexOf(this.keys[at]);
      return keys.length ? Math.min(at, keys.length - 1) : NONE_LEFT;
    };
    const rovingAt = stay(roving);
    const focusAt = focused >= 0 ? stay(focused) : undefined;
    this.keys = keys;
    this.element.replaceChildren(...chips());
    this.element.hidden = !keys.length;
    this.rove(rovingAt);
    if (focusAt === NONE_LEFT) fallback().focus();
    else if (focusAt !== undefined) this.buttons()[focusAt].focus();
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
