// The heavy jobs of a status document (status-panel.md S14, from phase 4) for the render, host and
// shot layers, and the oracle's model of what the heading's machine button (U10), the machine
// strip (U11) and the pattern editor (U15) say about them, written from status-panel.md and
// docs/design/jobs/jobs.md. Only the rows of the mockups bind.
import { DOT, age, visible } from './grammar.mjs';
import { NOW, POOL, ago, document, withActor, worktree } from './docs.mjs';

/** The `config` of every test document: the machine file of J4. */
export const CONFIG = '/Users/verify/.config/wtpool/machine.toml';

/** S14's `machine.jobs`: limit 4 of 1 to 64 under `warn`, saved, with nothing listed or running. */
export function jobs(overrides = {}) {
  return {
    limit: 4,
    default: 4,
    min: 1,
    max: 64,
    enforce: 'warn',
    source: { limit: 'machine', enforce: 'machine' },
    config: CONFIG,
    error: null,
    heavy: [],
    suggested: [],
    running: [],
    waiting: [],
    outside: [],
    ...overrides,
  };
}

let jobCount = 0;

/** A job or heavy process of S14: `id`, `summary`, `label`, `worktree`, `actor` and `since`. */
export function job(overrides = {}) {
  jobCount += 1;
  return {
    id: `${4100 + jobCount}${(0xa1b2c300 + jobCount).toString(16)}`,
    summary: 'cargo test',
    label: null,
    worktree: null,
    actor: null,
    since: ago(60),
    ...overrides,
  };
}

/** `status` with `machine.jobs` set to `value`, a fresh copy. */
export function withJobs(status, value) {
  const copy = structuredClone(status);
  copy.machine.jobs = value;
  return copy;
}

/**
 * The Strip mockup's state (jobs.md, "Status, hook and panel"): 4 of 4 run, 2 wait, 1 heavy
 * process runs outside a job, the machine lists `just`, this repository three patterns and a
 * `build_processes` name, and two suggestions from `Cargo.toml` are left.
 */
export function busyDocument() {
  const wt1 = withActor(
    'wt1',
    { key: 's1', label: 'Design jobs', state: 'working', action: { tool: 'queued:', target: 'cargo test' } },
    { branch: 'design-jobs' },
  );
  wt1.actors.push({ ...wt1.actors[0], key: 's2', label: 'WT panel build', action: { tool: 'Bash', target: 'cargo nextest' }, updated: ago(40) });
  const wt3 = withActor('wt3', { key: 's3', label: 'session 08:40', action: { tool: 'Bash', target: 'npm run' } }, { branch: 'review-ts' });
  const status = document([worktree('fg'), wt1, worktree('wt2'), wt3]);
  return withJobs(
    status,
    jobs({
      heavy: [
        { pattern: 'just', source: 'machine' },
        { pattern: 'cargo build', source: 'repository' },
        { pattern: 'cargo test', source: 'repository' },
        { pattern: 'cargo xtask verify', source: 'repository' },
        { pattern: 'protoc', source: 'build_processes' },
      ],
      suggested: [
        { pattern: 'cargo nextest', from: 'Cargo.toml' },
        { pattern: 'cargo clippy', from: 'Cargo.toml' },
      ],
      running: [
        job({ summary: 'cargo nextest', worktree: `${POOL}/wt1`, actor: 's2', since: ago(240) }),
        job({ summary: 'npm run', worktree: `${POOL}/wt3`, actor: 's3', since: ago(120) }),
        job({ summary: 'cargo xtask verify', label: 'verify', since: ago(720) }),
        job({ summary: 'cargo build', since: ago(60) }),
      ],
      waiting: [
        job({ summary: 'cargo test', worktree: `${POOL}/wt1`, actor: 's1', since: ago(60) }),
        job({ summary: 'cargo clippy', worktree: `${POOL}/fg`, since: ago(20) }),
      ],
      outside: [job({ summary: 'make', since: ago(180) })],
    }),
  );
}

/**
 * The FirstOpen mockup's state (J23, J24): no list yet, `make` from `Makefile` and six pnpm
 * scripts from `package.json` suggested, nothing running.
 */
export function firstOpenDocument() {
  const status = document([worktree('fg'), worktree('wt1')], { repository: { name: 'shop', github: 'acme/shop' } });
  const pnpm = ['pnpm run build', 'pnpm build', 'pnpm run test', 'pnpm test', 'pnpm run test:e2e', 'pnpm test:e2e'];
  return withJobs(
    status,
    jobs({ suggested: [{ pattern: 'make', from: 'Makefile' }, ...pnpm.map((pattern) => ({ pattern, from: 'package.json' }))] }),
  );
}

/** The patterns of one scope of U15: `machine`, or `repository` with its `build_processes` names. */
export function scopePatterns(value, scope) {
  const sources = scope === 'machine' ? ['machine'] : ['repository', 'build_processes'];
  return value.heavy.filter((entry) => sources.includes(entry.source));
}

/** U10's text of the machine button, its codicons aside. */
export function buttonText(value, machine) {
  const name = visible(machine);
  if (value === undefined) return name;
  if (value.error !== null) return `${name}${DOT}jobs`;
  const parts = [`${name}${DOT}jobs ${value.running.length}/${value.limit}`];
  if (value.waiting.length) parts.push(`${value.waiting.length} queued`);
  if (value.enforce !== 'off' && value.outside.length) parts.push(`${value.outside.length} not queued`);
  if (value.suggested.length && !scopePatterns(value, 'repository').length) parts.push(`${value.suggested.length} suggested`);
  return parts.join(DOT);
}

/**
 * U10's accessible name: the button's visible text, then the sentence. The template lists every
 * count; the oracle reads it as naming the counts the button shows, in the template's words, so a
 * zero count is left out as it is on screen.
 */
export function buttonName(value, machine) {
  const parts = [`${value.running.length} of ${value.limit} running`];
  if (value.waiting.length) parts.push(`${value.waiting.length} queued`);
  if (value.enforce !== 'off' && value.outside.length) parts.push(`${value.outside.length} not queued`);
  if (value.suggested.length && !scopePatterns(value, 'repository').length) parts.push(`${value.suggested.length} suggested`);
  return `${buttonText(value, machine)}, heavy jobs on ${visible(machine)}: ${parts.join(', ')}`;
}

/** The parts of U10's text in the attention colour, and the one in the description colour. */
export function buttonColours(value) {
  const attention = [];
  if (value.waiting.length) attention.push(`${value.waiting.length} queued`);
  if (value.enforce !== 'off' && value.outside.length) attention.push(`${value.outside.length} not queued`);
  const muted = value.suggested.length && !scopePatterns(value, 'repository').length ? [`${value.suggested.length} suggested`] : [];
  return { attention, muted };
}

/** The worktree name and actor label of a job line (U11, S15), or `outside this pool`. */
export function jobPlace(entry, status) {
  const worktrees = status.pools[0].worktrees;
  const where = worktrees.find((w) => w.path === entry.worktree);
  if (!where) return 'outside this pool';
  const actor = where.actors.find((a) => a.key === entry.actor);
  return actor ? `${visible(where.name)}${DOT}${visible(actor.label)}` : visible(where.name);
}

/** The words of one job line of U11, in order: position (Queued only), name, place and age. */
export function jobLineParts(entry, status, position, nowMs = NOW) {
  const parts = [];
  if (position !== undefined) parts.push(String(position));
  parts.push(visible(entry.label ?? entry.summary), jobPlace(entry, status), age(entry.since, nowMs));
  return parts;
}

/** U11's fixed sentences. */
export const TEXT = {
  oldWtpool: 'This wtpool has no heavy job limit. Update wtpool to a version with wtpool job to set one.',
  remote: (machine) => `This window's folder is not on this computer: the settings are for this computer, ${machine}.`,
  over: (running, limit) => `${running} running, over the limit of ${limit}: they finish, and queued jobs start once fewer than ${limit} run.`,
  saved: (value) => `Saved in ${value.config}${DOT}defaults ${value.default}, warn${DOT}${value.min} to ${value.max}`,
  notSaved: 'Not saved: the defaults apply',
  hint: 'Agents and scripts take a place when they run a command as wtpool job -- <command>.',
  limitError: (min, max) => `Enter a whole number from ${min} to ${max}.`,
  patternError:
    'Not a pattern: 1 to 3 words of letters, digits and . _ + -, and : after the first, none starting with - or +; only the first may end in *.',
  duplicate: 'Already in this list.',
  nothingHeavy: 'Nothing is heavy until you add it.',
  placeholder: 'add a pattern, such as go test',
};

/**
 * J18 as the oracle reads it: one to three words split by single spaces, each 1 to 32 characters
 * of `A-Z a-z 0-9 . _ + -`, `:` allowed in a word but the first, no word starting with `-` or
 * `+`, and only the first word may end in a `*`, which is then not its only character.
 */
export function isPattern(text) {
  const words = text.split(' ');
  if (words.length < 1 || words.length > 3) return false;
  return words.every((word, index) => {
    const star = index === 0 && word.endsWith('*');
    const body = star ? word.slice(0, -1) : word;
    if (!body.length || word.length > 32) return false;
    const allowed = index === 0 ? /^[A-Za-z0-9._+-]+$/ : /^[A-Za-z0-9._+:-]+$/;
    return allowed.test(body) && !/^[-+]/.test(body);
  });
}

/** J18's cases: the examples, both limits of a word's length, and each rule broken once. */
export const PATTERN_CASES = [
  ['make', true],
  ['cargo build', true],
  ['cargo xtask verify', true],
  ['cargo-*', true],
  ['npm run build', true],
  ['pnpm run test:e2e', true],
  ['*', false],
  ['make *', false],
  ['-rf', false],
  ['cargo  build', false],
  ['a', true],
  ['x'.repeat(32), true],
  ['x'.repeat(33), false],
  ['cargo build y', true],
  ['cargo build test extra', false],
  ['+x', false],
  ['cargo -p', false],
  ['cargo +nightly', false],
  ['cargo b*', false],
  ['mvn:x', false],
  ['go test:unit', true],
  ['cargo\tbuild', false],
  [' make', false],
  ['make ', false],
  ['ma/ke', false],
  ['cargo bé', false],
  ['cargö', false],
  ['make​', false],
];

/** U15's label of a suggestions group: `from <marker>`, `package.json` read as its scripts. */
export function groupLabel(from) {
  return from === 'package.json' ? 'from package.json scripts' : `from ${visible(from)}`;
}

/** The suggestions grouped by `from`, groups in their first appearance's order. */
export function suggestionGroups(value) {
  const groups = new Map();
  for (const entry of value.suggested) {
    if (!groups.has(entry.from)) groups.set(entry.from, []);
    groups.get(entry.from).push(entry.pattern);
  }
  return [...groups].map(([from, patterns]) => ({ from, label: groupLabel(from), patterns }));
}
