import assert from 'node:assert/strict';
import { JSDOM } from 'jsdom';
import { test } from 'vitest';

import { fakeWebview, mustLoadBuildHtml } from '../support/webview.mjs';

const cases = [
  ['first_nonce', 'https://a.vscode-cdn.test', 'r4nd0mN0nce4F00d1234567890abcdef'],
  ['second_nonce', 'https://file+.vscode-resource.vscode-cdn.net', 'Zm9vYmFyYmF6cXV4MTIzNDU2Nzg5MA'],
];

test.each(cases)('csp__should_match_the_template %s', async (_, cspSource, nonce) => {
  // Given
  const buildHtml = await mustLoadBuildHtml();

  // When
  const html = buildHtml(fakeWebview(cspSource), nonce);

  // Then
  const { document } = new JSDOM(html).window;
  const metas = [...document.querySelectorAll('meta[http-equiv="Content-Security-Policy" i]')];
  assert.equal(metas.length, 1, 'exactly one CSP meta element');
  assert.equal(
    metas[0].getAttribute('content'),
    `default-src 'none'; script-src 'nonce-${nonce}'; style-src ${cspSource}; font-src ${cspSource}; require-trusted-types-for 'script'`,
  );
  const head = document.head.firstElementChild;
  assert.ok(
    metas[0] === head || (head.tagName === 'META' && head.hasAttribute('charset') && head.nextElementSibling === metas[0]),
    'the CSP meta element comes first in head, after a charset meta at most',
  );
  const scripts = [...document.querySelectorAll('script')];
  assert.ok(scripts.length >= 1, 'no script element');
  for (const script of scripts) {
    // Any script with the nonce may run, a file under cspSource or an inline one.
    assert.equal(script.getAttribute('nonce'), nonce, `a script without the nonce, or with another: ${script.outerHTML}`);
    if (script.hasAttribute('src')) {
      assert.ok(script.getAttribute('src').startsWith(`${cspSource}/`), `a script not loaded from cspSource: ${script.outerHTML}`);
    } else {
      assert.ok(script.textContent.trim(), `an empty inline script: ${script.outerHTML}`);
    }
    assert.notEqual(script.getAttribute('type'), 'module', 'the webview script is a classic script');
  }
  assert.equal(document.querySelectorAll('style').length, 0, 'an inline style element');
  assert.equal(document.querySelectorAll('[style]').length, 0, 'an inline style attribute');
  for (const link of document.querySelectorAll('link[href]')) {
    assert.ok(link.getAttribute('href').startsWith(`${cspSource}/`), `a link outside cspSource: ${link.outerHTML}`);
  }
  assert.ok(!html.includes('unsafe-'), 'the HTML names an unsafe- source');
});
