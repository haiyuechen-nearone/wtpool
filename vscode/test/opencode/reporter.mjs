// A reporter for Node's test runner that writes Mocha's JSON shape, `{passes, failures, pending}`,
// the one the harness reads for the host layer. A test file that fails to load reports as
// `file:<name>`.
export default async function* mochaJson(source) {
  const passes = [];
  const failures = [];
  const pending = [];
  for await (const event of source) {
    if (event.type !== 'test:pass' && event.type !== 'test:fail') continue;
    const { name, details, skip, todo, file } = event.data;
    if (details?.type === 'suite') continue;
    const loadFailure = file && name && file.endsWith(name);
    const title = loadFailure ? `file:${name}` : name;
    if (skip || todo) {
      pending.push({ title });
    } else if (event.type === 'test:pass') {
      passes.push({ title });
    } else {
      const error = details?.error;
      const cause = error?.cause ?? error;
      let message = String(cause?.message ?? cause ?? '');
      if (error?.failureType === 'testTimeoutFailure') message = `Timeout of ${details?.duration_ms ?? 0} ms exceeded: ${message}`;
      failures.push({ title, err: { message, stack: String(cause?.stack ?? message), code: cause?.code ?? null } });
    }
  }
  yield `${JSON.stringify({ passes, failures, pending }, null, 1)}\n`;
}
