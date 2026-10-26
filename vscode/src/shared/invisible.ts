// The characters a string from the document may hide behind (X10, R8): control and format
// characters, bidi controls and zero width ones among them, the line and paragraph separators,
// and the tag block. The webview shows each as `\u{XXXX}`; names and dialogs outside the webview
// leave them out.

/**
 * Control (Cc) and format (Cf) characters, the line (Zl) and paragraph (Zp) separators, and the
 * whole tag block, the code points it has not assigned yet included.
 */
const INVISIBLE = /[\p{Cc}\p{Cf}\p{Zl}\p{Zp}\u{E0000}-\u{E007F}]/gu;

/** Turns a string from the document into plain text for a terminal name or a dialog. */
export function plain(text: string): string {
  return text.replace(INVISIBLE, '');
}

/** Writes one invisible character as `\u{XXXX}`: upper case hex, at least four digits. */
function escaped(char: string): string {
  // A match is one code point, so the fallback never applies.
  const code = char.codePointAt(0) ?? 0;
  return `\\u{${code.toString(16).toUpperCase().padStart(4, '0')}}`;
}

/** Gives a string as X10 shows it, each invisible character as `\u{XXXX}`. */
export function visible(text: string): string {
  return text.replace(INVISIBLE, escaped);
}
