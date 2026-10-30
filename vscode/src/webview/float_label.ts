import { div, setText } from './dom';

/** The pause between the pointer entering an element and its label showing. */
const SHOW_DELAY_MS = 300;
/** The gap between an element and its label, and the least gap to the visible area's edge. */
const GAP_PX = 4;

/**
 * A floating label that the page draws over any element carrying a `data-label`: it shows 300 ms
 * after the pointer enters the element and goes when the pointer leaves it. It sits below the
 * element, or above it where there is no room below, inside the visible area, and moves no row.
 */
export class FloatLabel {
  readonly element = div('float-label');
  /** The labelled element under the pointer, whether its label shows yet or not. */
  private target: HTMLElement | null = null;
  private shown = false;
  private timer?: number;
  private x = 0;
  private y = 0;

  constructor(root: Document) {
    this.element.hidden = true;
    this.element.setAttribute('aria-hidden', 'true');
    root.addEventListener('pointerover', (event) => void this.enter(event));
    root.documentElement.addEventListener('pointerleave', () => void this.hide());
    root.addEventListener('scroll', () => void this.hide(), true);
    root.addEventListener('keydown', (event) => {
      if (event.key === 'Escape') this.hide();
    });
  }

  /** Hides the label and forgets the element under the pointer. */
  hide(): void {
    window.clearTimeout(this.timer);
    this.target = null;
    this.shown = false;
    this.element.hidden = true;
  }

  /**
   * Follows the page after its rows changed: moves a shown label with its element, and hands it to
   * the labelled element now under the pointer when its own is gone.
   */
  refresh(): void {
    if (!this.target) return;
    if (!this.target.isConnected) {
      const under = document.elementFromPoint(this.x, this.y)?.closest<HTMLElement>('[data-label]');
      const wasShown = this.shown;
      this.hide();
      if (!under) return;
      this.target = under;
      if (wasShown) this.show(under);
      else this.timer = window.setTimeout(() => void this.show(under), SHOW_DELAY_MS);
    } else if (this.shown) {
      this.place(this.target);
    }
  }

  private enter(event: PointerEvent): void {
    this.x = event.clientX;
    this.y = event.clientY;
    const target =
      event.target instanceof Element ? event.target.closest<HTMLElement>('[data-label]') : null;
    if (target === this.target) return;
    this.hide();
    if (!target) return;
    this.target = target;
    this.timer = window.setTimeout(() => void this.show(target), SHOW_DELAY_MS);
  }

  private show(target: HTMLElement): void {
    if (this.target !== target) return;
    setText(this.element, target.getAttribute('data-label') ?? '');
    this.shown = true;
    this.element.hidden = false;
    this.place(target);
  }

  /** Puts the label below `target`, or above it when the visible area ends first. */
  private place(target: HTMLElement): void {
    const { style } = this.element;
    style.left = '0';
    style.top = '0';
    const box = target.getBoundingClientRect();
    const { clientWidth, clientHeight } = document.documentElement;
    const { offsetWidth, offsetHeight } = this.element;
    const left = Math.max(GAP_PX, Math.min(box.left, clientWidth - offsetWidth - GAP_PX));
    const below = box.bottom + GAP_PX;
    const fitsBelow = below + offsetHeight + GAP_PX <= clientHeight;
    const top = fitsBelow ? below : Math.max(GAP_PX, box.top - GAP_PX - offsetHeight);
    style.left = `${left}px`;
    style.top = `${top}px`;
  }
}
