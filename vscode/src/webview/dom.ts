import type { Piece } from './text';

export function span(className: string): HTMLSpanElement {
  const element = document.createElement('span');
  element.className = className;
  return element;
}

export function div(className: string): HTMLDivElement {
  const element = document.createElement('div');
  element.className = className;
  return element;
}

export function cell(className: string): HTMLTableCellElement {
  const element = document.createElement('td');
  element.className = className;
  return element;
}

/** Makes a `bdi` that isolates a string from the document. */
export function bdi(text: string, className = ''): HTMLElement {
  const element = document.createElement('bdi');
  element.className = className;
  element.textContent = text;
  return element;
}

/** Sets a node's text only when it differs, so an unchanged value makes no mutation. */
export function setText(node: Node, text: string): void {
  if (node.textContent !== text) node.textContent = text;
}

/** Makes a span that holds `text`. */
export function textSpan(className: string, text: string): HTMLSpanElement {
  const element = span(className);
  element.textContent = text;
  return element;
}

/**
 * Makes the nodes of a line of pieces: document strings isolated, the panel's words as text, in a
 * span of their class when they have one.
 */
export function pieceNodes(pieces: Piece[]): Node[] {
  return pieces.map((piece) => {
    if (piece.from) return bdi(piece.text);
    if (piece.since !== undefined) {
      const element = textSpan(piece.className ?? '', piece.text);
      element.dataset.since = piece.since;
      return element;
    }
    return piece.className ? textSpan(piece.className, piece.text) : document.createTextNode(piece.text);
  });
}

export function line(pieces: Piece[], className = 'line'): HTMLDivElement {
  const element = div(className);
  element.append(...pieceNodes(pieces));
  return element;
}
