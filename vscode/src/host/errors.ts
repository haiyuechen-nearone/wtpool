// Errors as the extension host meets them: Node and the VS Code API throw and reject only with
// instances of Error, so a catch reads `message` after one check.

/** Asserts that `cause` is an Error, as everything the extension host calls throws. */
export function assertIsError(cause: unknown): asserts cause is Error {
  if (!(cause instanceof Error)) throw new TypeError('a call threw a value that is not an Error');
}
