import assert from 'node:assert/strict';
import { afterEach, test } from 'vitest';

import { document, mustValidate, worktree } from '../support/docs.mjs';
import { headingText, normalizeEscapes } from '../support/grammar.mjs';
import { isShown, mount, rows, text } from '../support/webview.mjs';

let webview;
afterEach(() => webview?.close());

function headings() {
  return [...webview.document.querySelectorAll('tr[data-machine]')].filter((row) => isShown(webview.window, row));
}

function poolOf(heading) {
  return heading.matches('tr.pool-heading[data-machine][data-pool]') ? heading.getAttribute('data-pool') : null;
}

const onePool = (repository) => mustValidate(document([worktree('fg'), worktree('wt1')], { repository }));

const cases = [
  ['github', { name: 'fg', github: 'acme/app' }, 'acme/app · verify-mac'],
  ['no_github_remote', { name: 'app', github: null }, 'app · verify-mac'],
  ['dots_dashes_and_underscores', { name: 'fg', github: 'acme-co/my_app.v2' }, 'acme-co/my_app.v2 · verify-mac'],
  ['hostile_folder_name', { name: 'app‮bad', github: null }, 'app\\u{202E}bad · verify-mac'],
];

for (const [name, repository, expected] of cases) {
  test(`heading__should_name_the_repository_then_the_machine ${name}`, async () => {
    // Given
    const status = onePool(repository);
    webview = await mount();

    // When
    webview.show(status);

    // Then
    const shown = headings();
    assert.equal(shown.length, 1, `one pool shows ${shown.length} headings tr[data-machine]`);
    assert.equal(expected, headingText(status.pools[0], status.machine), 'setup: the oracle model differs from the case');
    assert.equal(normalizeEscapes(text(shown[0])), normalizeEscapes(expected), 'the heading text');
    assert.equal(poolOf(shown[0]), status.pools[0].common_dir, 'the heading is no tr.pool-heading[data-pool] of the pool');
    const first = rows(webview.document)[0];
    assert.ok(
      shown[0].compareDocumentPosition(first) & webview.window.Node.DOCUMENT_POSITION_FOLLOWING,
      'the heading row comes before the worktree rows',
    );
  });
}

test('heading__should_name_the_repository_then_the_machine extra_pool_ignored', async () => {
  // Given: a document a later wtpool might print, with a second pool after the first, as changes
  // within version 1 only add. The tab shows the first pool's repository alone.
  const other = '/Users/verify/tools';
  const second = document(
    [
      { ...worktree('fg'), path: other, name: 'tools' },
      { ...worktree('wt1'), path: `${other}-wt1`, name: 'tools-wt1' },
    ],
    { common_dir: `${other}/.git`, repository: { name: 'tools', github: null } },
  ).pools[0];
  const first = onePool({ name: 'app', github: 'acme/app' });
  const status = mustValidate({ ...first, pools: [first.pools[0], second] });
  webview = await mount();

  // When
  webview.show(status);

  // Then
  assert.deepEqual(headings().map((row) => [text(row), poolOf(row)]), [['acme/app · verify-mac', first.pools[0].common_dir]], 'the headings of a document with a second pool');
  assert.deepEqual(
    rows(webview.document).map((row) => row.getAttribute('data-row-id')).sort(),
    first.pools[0].worktrees.map((w) => w.path).sort(),
    `the rows: the first pool's, none of ${other}`,
  );
  assert.deepEqual(webview.errors.map(String), [], 'the webview raised errors');
});
