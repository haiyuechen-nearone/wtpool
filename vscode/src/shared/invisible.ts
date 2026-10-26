/**
 * The characters a string from the document may hide text behind. The tag block is listed whole,
 * as Cf leaves out its code points that are not assigned yet.
 */
const INVISIBLE = /[\p{Cc}\p{Cf}\p{Zl}\p{Zp}\u{E0000}-\u{E007F}]/gu;

/** Turns a string from the document into plain text for a terminal name or a dialog. */
export function plain(text: string): string {
  return text.replace(INVISIBLE, '');
}

function escaped(char: string): string {
  // A match is one code point, so the fallback never applies.
  const code = char.codePointAt(0) ?? 0;
  return `\\u{${code.toString(16).toUpperCase().padStart(4, '0')}}`;
}

/** Gives `text` with each invisible character written as `\u{XXXX}`. */
export function visible(text: string): string {
  return text.replace(INVISIBLE, escaped);
}
