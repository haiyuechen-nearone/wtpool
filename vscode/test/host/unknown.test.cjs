const assert = require('node:assert/strict');
const { isDeepStrictEqual } = require('node:util');

const { expectedDocuments, openStatus, posted, resetFake, sleep, starts, waitFor } = require('./support.cjs');

const UNKNOWN = 'wtpool_verify_unknown';

/**
 * `doc` with an unknown property on every object: the document, machine, pool, repository,
 * worktree, git, actor, action and build, of values of each JSON type.
 */
function withUnknown(doc) {
  const out = structuredClone(doc);
  out[UNKNOWN] = { added: 'later', in: ['version', 1] };
  out.machine[UNKNOWN] = 'machine';
  for (const pool of out.pools) {
    pool[UNKNOWN] = ['pool'];
    if (pool.repository) pool.repository[UNKNOWN] = 1;
    for (const worktree of pool.worktrees) {
      worktree[UNKNOWN] = null;
      worktree.git[UNKNOWN] = true;
      for (const actor of worktree.actors) {
        actor[UNKNOWN] = {};
        if (actor.action) actor.action[UNKNOWN] = 'action';
      }
      for (const build of worktree.builds) build[UNKNOWN] = 0;
    }
  }
  return out;
}

/** `value` without the unknown properties {@link withUnknown} added. */
function known(value) {
  if (Array.isArray(value)) return value.map(known);
  if (value === null || typeof value !== 'object') return value;
  return Object.fromEntries(Object.entries(value).filter(([key]) => key !== UNKNOWN).map(([key, v]) => [key, known(v)]));
}

describe('unknown properties', () => {
  it('stream__should_accept_unknown_properties', async () => {
    // Given: a document with actors and actions, then one with a build, each given unknown
    // properties, distinct by `generated`.
    const plain = [
      expectedDocuments('several_agents_one_worktree').at(-1),
      expectedDocuments('building_without_agent')[0],
    ].map((doc, index) => ({ ...doc, generated: `2026-10-20T13:00:0${index}Z` }));
    const documents = plain.map(withUnknown);
    const count = (key) => JSON.stringify(documents).split(`"${key}"`).length - 1;
    assert.ok(count(UNKNOWN) >= 12, `setup: only ${count(UNKNOWN)} unknown properties`);
    await resetFake({ documents, aliveMs: 200, gapMs: 300 });
    const from = (await posted()).length;

    // When
    await openStatus();
    const views = () => posted().then((all) => all.slice(from).filter((m) => m && m.type === 'view'));
    const reached = await waitFor(async () => {
      const seen = await views();
      return plain.every((doc) => seen.some((v) => v.connection === 'live' && isDeepStrictEqual(known(v.status), doc))) && seen;
    }, 15_000);
    await sleep(1500);
    const seen = await views();

    // Then
    const missing = plain.filter((doc) => !(reached || []).some((v) => isDeepStrictEqual(known(v.status), doc))).map((doc) => doc.generated);
    assert.deepEqual(missing, [], 'documents with unknown properties that never reached the webview live');
    assert.equal(starts().length, 1, 'a document with unknown properties restarted the stream');
    assert.deepEqual(
      seen.filter((v) => !['starting', 'live'].includes(v.connection)).map((v) => v.connection),
      [],
      'a document with unknown properties paused the stream',
    );
  });
});
