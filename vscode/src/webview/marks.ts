import type { Actor } from '../generated/status';
import { visible } from '../shared/invisible';
import { markLabelText, resolveMark } from '../shared/marks';
import { span } from './dom';

/** Copies the mark of row `id` from the page's templates, or gives null where the page has none. */
function markSvg(id: string): Node | null {
  const template = document.getElementById(`mark-${id}`);
  const drawn = template instanceof HTMLTemplateElement ? template.content.firstElementChild : null;
  return drawn ? document.importNode(drawn, true) : null;
}

/**
 * Makes the box after a line's state icon: the mark of `actor`'s model and harness, with the
 * harness's badge at its lower right corner when the harness is not the maker's own, and the text
 * of its floating label in `data-label`. The box stays empty for a line of no actor and for an
 * actor with no mark, so every line's text starts at the same x.
 */
export function markBox(actor: Actor | undefined): HTMLElement {
  const box = span('line-mark');
  box.setAttribute('aria-hidden', 'true');
  const marks = actor ? resolveMark(actor.harness, actor.model) : null;
  const mark = marks ? markSvg(marks.mark) : null;
  if (!actor || !marks || !mark) return box;
  box.append(mark);
  box.setAttribute('data-mark', marks.mark);
  box.setAttribute('data-label', visible(markLabelText(actor.harness, actor.model)));
  const badge = marks.badge ? markSvg(marks.badge.mark) : null;
  if (marks.badge && badge) {
    const ring = span('line-badge');
    ring.append(badge);
    box.append(ring);
    box.setAttribute('data-badge', marks.badge.harness);
  }
  return box;
}
