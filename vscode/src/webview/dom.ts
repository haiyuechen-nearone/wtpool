// DOM helpers. Strings reach the page only through textContent and setAttribute, every string
// from the document sits in its own <bdi> (X10), and colours come from classes (X13).
import type { Piece } from './text';

/** Makes a `span` of `className`. */
export function span(className: string): HTMLSpanElement {
  const element = document.createElement('span');
  element.className = className;
  return element;
}

/** Makes a `div` of `className`. */
export function div(className: string): HTMLDivElement {
  const element = document.createElement('div');
  element.className = className;
  return element;
}

/** Makes a table cell of `className`. */
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

/** Makes the nodes of a line of pieces: document strings isolated, the panel's words as text. */
export function pieceNodes(pieces: Piece[]): Node[] {
  return pieces.map((piece) => (piece.from ? bdi(piece.text) : document.createTextNode(piece.text)));
}

/** Makes a `div` holding pieces, of class `line` unless `className` names another. */
export function line(pieces: Piece[], className = 'line'): HTMLDivElement {
  const element = div(className);
  element.append(...pieceNodes(pieces));
  return element;
}
