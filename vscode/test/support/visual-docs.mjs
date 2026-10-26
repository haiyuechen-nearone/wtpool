// Documents for the visual checks of status-panel.md revision 14: a pool with real-world
// worktree names, long values in every column, many chips, a long machine name, and long
// repository names in the heading (U8, revision 16), on GitHub and local. Each is valid against
// the schema; `visualDocuments` adds every scenario document and every_activity.
import { readdirSync } from 'node:fs';

import { POOL, actor, ago, document, everyActivity, git, mustValidate, scenarioDocuments, worktree } from './docs.mjs';
import { SCENARIOS } from './paths.mjs';

/** A worktree with a real name, its role given; a slot is busy unless `overrides` frees it. */
function named(name, role, overrides = {}) {
  const base = worktree(role === 'foreground' ? 'fg' : role === 'slot' ? 'wt9' : 'x');
  const busy = role === 'slot' ? { free: false } : {};
  return { ...base, ...busy, path: `${POOL}/${name}`, name, role, ...overrides };
}

/** The real-names pool: app in the foreground, app-wt1 to app-wt3, an unmanaged app-review. */
export function realistic() {
  return document([
    named('app', 'foreground', { branch: '1234-check-widget-cache-health-before-each-deploy' }),
    named('app-wt1', 'slot', {
      branch: 'build-widget-cache-generation-for-each-region',
      reasons: ['branch', 'dirty'],
      git: git({ dirty: 2, ahead: 1, behind: 0 }),
      activity: 'working',
      actors: [actor({ label: 'review', action: { tool: 'Bash', target: 'cargo nextest run -E package(app-core)' } })],
      last_event: ago(30),
    }),
    named('app-wt2', 'slot', {
      branch: '5678-signed-release-notes',
      reasons: ['branch', 'dirty'],
      git: git({ dirty: 3, ahead: 2, behind: 0 }),
      activity: 'waiting',
      actors: [
        actor({ label: 'session 13:41', state: 'waiting', reason: 'permission', action: { tool: 'Bash', target: 'git push' }, updated: ago(2) }),
        actor({ label: 'test', state: 'done', action: null, updated: ago(720) }),
      ],
      last_event: ago(2),
    }),
    named('app-wt3', 'slot', { free: true, activity: 'free', last_change: ago(7200) }),
    named('app-review', 'unmanaged', { branch: 'dev/review-signed-verdicts', reasons: ['unmanaged'], activity: 'unused' }),
  ], { repository: { name: 'app', github: 'acme/app' } });
}

/** A long GitHub repository: an owner close to GitHub's 39 character limit and a long name. */
export const LONG_GITHUB = 'acme-widget-platform-engineering-tool/app-widget-cache-health-and-release-candidate-tooling';

/** A long main worktree folder name, for a repository without a GitHub remote. */
export const LONG_FOLDER = 'widget-cache-health-dashboard-for-the-release-candidates-of-every-region';

/** Long values in every text column, a long machine name and a long repository name. */
export function longValues() {
  return document(
    [
      named('app-foreground-checkout', 'foreground', { branch: '1234-check-widget-cache-health-before-each-deploy-and-more' }),
      named('app-wt1', 'slot', {
        branch: 'build-widget-cache-generation-for-each-region-with-a-very-long-suffix',
        reasons: ['branch'],
        activity: 'waiting',
        actors: [
          actor({
            label: 'explore-subagent-widget-cache-health-review',
            state: 'waiting',
            reason: 'permission',
            action: { tool: 'Bash', target: 'cargo nextest run -p app-core --features network-failure-simulation' },
            updated: ago(3),
          }),
        ],
        last_event: ago(3),
      }),
      named('app-wt2', 'slot', {
        branch: '5678-signed-release-notes',
        reasons: ['branch'],
        activity: 'working',
        actors: [
          actor({ label: 'session 13:41 with a long session title', action: { tool: 'mcp__claude_ai_Google_Drive__search_files', target: 'query=widget cache health before each deploy' }, updated: ago(10) }),
          actor({ kind: 'subagent', label: 'general-purpose', state: 'quiet', action: { tool: 'Read', target: 'crates/core/src/widget_cache.rs' }, updated: ago(200) }),
          actor({ label: 'session 14:02', state: 'idle', action: null, error: 'overloaded_error: Overloaded, retry after 30 seconds please', updated: ago(4000) }),
        ],
        last_event: ago(10),
      }),
      named('app-wt3', 'slot', {
        reasons: ['dirty', 'unreferenced', 'operation'],
        git: git({ dirty: 128, unreferenced: 3, operation: 'cherry-pick' }),
        activity: 'not_reusable',
        last_change: ago(200000),
      }),
      named('app-review-signed-verdicts', 'unmanaged', {
        branch: 'dev/review-signed-verdicts-of-the-release-candidate',
        reasons: ['unmanaged'],
        activity: 'unused',
        git: git({ dirty: 12, ahead: 34, behind: 567 }),
      }),
      named('x', 'unmanaged', { branch: 'side', reasons: ['unmanaged'], activity: 'unused' }),
    ],
    { repository: { name: 'app-foreground-checkout', github: LONG_GITHUB } },
    { machine: { name: 'office-build-machine-macbook-pro-m4-max-2026-with-a-very-long-hostname.example.internal' } },
  );
}

/** Many worktrees: chips with two digit counts. */
export function bigCounts() {
  const list = [named('app', 'foreground')];
  for (let i = 1; i <= 12; i += 1) list.push(named(`app-wt${i}`, 'slot', { free: true, activity: 'free' }));
  for (let i = 13; i <= 24; i += 1) {
    const waiting = i % 2 === 0;
    list.push(
      named(`app-wt${i}`, 'slot', {
        branch: `b${i}`,
        reasons: ['branch'],
        activity: waiting ? 'waiting' : 'working',
        actors: [actor(waiting ? { state: 'waiting', reason: 'question', action: null } : {})],
        last_event: ago(20),
      }),
    );
  }
  return document(list, { repository: { name: 'app', github: null } });
}

/** The real-names pool without a GitHub remote, its long main worktree folder name its heading (U8). */
export function longFolder() {
  const doc = realistic();
  const pool = doc.pools[0];
  pool.repository = { name: LONG_FOLDER, github: null };
  pool.worktrees = pool.worktrees.map((w) => (w.role === 'foreground' ? { ...w, name: LONG_FOLDER, path: `${POOL}/${LONG_FOLDER}` } : w));
  return doc;
}

/** The documents whose every row the checks hover, focus and select. */
export function stateDocuments() {
  return [
    { name: 'every_activity', doc: mustValidate(everyActivity()) },
    { name: 'realistic', doc: mustValidate(realistic()) },
    { name: 'long_values', doc: mustValidate(longValues()) },
  ];
}

/**
 * Every scenario document, then the state documents and bigCounts, each with `states` true when
 * a check should also hover each row, and the header in its stale and paused forms.
 */
export function visualDocuments() {
  const list = [];
  const names = readdirSync(SCENARIOS)
    .filter((f) => f.endsWith('.json'))
    .map((f) => f.slice(0, -5))
    .sort();
  for (const name of names) {
    scenarioDocuments(name).documents.forEach((doc, i) => list.push({ name: `${name}#${i}`, doc: mustValidate(doc) }));
  }
  for (const item of stateDocuments()) list.push({ ...item, states: true });
  list.push({ name: 'big_counts', doc: mustValidate(bigCounts()) });
  list.push({ name: 'long_folder', doc: mustValidate(longFolder()) });
  for (const [name, doc] of [['every_activity', everyActivity()], ['big_counts', bigCounts()]]) {
    list.push({ name: `${name} stale`, doc: mustValidate(doc), options: { connection: 'stale', lastLine: '2026-10-20T11:58:00Z' } });
    list.push({ name: `${name} paused`, doc: mustValidate(doc), options: { connection: 'paused', retryAt: '2026-10-20T12:00:30Z' } });
  }
  return list;
}

/** The tag each row of `doc` carries (U1): `fg` for the foreground, `unmanaged` for those. */
export function expectedTags(doc) {
  const out = {};
  for (const w of doc.pools[0].worktrees) {
    if (w.role === 'foreground') out[w.path] = { name: w.name, tag: 'fg' };
    if (w.role === 'unmanaged') out[w.path] = { name: w.name, tag: 'unmanaged' };
  }
  return out;
}
