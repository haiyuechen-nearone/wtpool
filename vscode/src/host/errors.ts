/** Asserts that `cause` is an Error, as everything the extension host calls throws. */
export function assertIsError(cause: unknown): asserts cause is Error {
  if (!(cause instanceof Error)) throw new TypeError('a call threw a value that is not an Error');
}
