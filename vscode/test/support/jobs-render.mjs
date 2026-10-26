// The render layer's reach into the phase 4 parts of the panel: the heading's machine button
// (U10), the machine strip (U11, X20), the pattern editor (U15, X22) and the agent lines' count
// and icons (U12, U13), through the DOM of jobs.md, "Test interfaces it adds", item 5.
import assert from 'node:assert/strict';

import { shownActors } from './grammar.mjs';
import { isDisplayed, isShown, isVisuallyHidden, spoken, text } from './webview.mjs';

export const button = (doc) => doc.querySelector('tr.pool-heading button.machine-button');

/** The shown machine strip, or null. */
export function strip(webview) {
  const found = webview.document.querySelector('tr.machine-detail');
  return found && isShown(webview.window, found) ? found : null;
}

/** The machine button, asserted present: phase 3 has none. */
export function mustButton(webview) {
  const found = button(webview.document);
  assert.ok(found, `the heading holds no button.machine-button (U10): ${text(webview.document.querySelector('tr.pool-heading'))}`);
  return found;
}

/** Opens the strip with a click on the machine button and returns it, asserted shown. */
export function openStrip(webview) {
  const found = mustButton(webview);
  if (!strip(webview)) found.click();
  webview.tick(20);
  const shown = strip(webview);
  assert.ok(shown, 'a click on the machine button shows no tr.machine-detail (U10, U11)');
  return shown;
}

/** An element of the strip, asserted present. */
export function mustFind(root, selector, what) {
  const found = root?.querySelector(selector);
  assert.ok(found, `no ${selector}${what ? ` (${what})` : ''}`);
  return found;
}

/** Types `value` into a field as a user does: the value, then an `input` event. */
export function type(webview, field, value) {
  field.focus();
  field.value = value;
  field.dispatchEvent(new webview.window.Event('input', { bubbles: true }));
  webview.tick(20);
}

/** Chooses `value` in a select: the value, then `input` and `change` events. */
export function choose(webview, select, value) {
  select.focus();
  select.value = value;
  select.dispatchEvent(new webview.window.Event('input', { bubbles: true }));
  select.dispatchEvent(new webview.window.Event('change', { bubbles: true }));
  webview.tick(20);
}

/**
 * Presses `key` on `target` as Chromium does: keydown, then keyup, with Enter activating a button
 * on keydown and Space on keyup, unless a handler prevented the default.
 */
export function press(webview, target, key) {
  const { window } = webview;
  const down = new window.KeyboardEvent('keydown', { key, bubbles: true, cancelable: true });
  const activates = target.tagName === 'BUTTON';
  const notPrevented = target.dispatchEvent(down);
  if (activates && key === 'Enter' && notPrevented) target.click();
  const up = new window.KeyboardEvent('keyup', { key, bubbles: true, cancelable: true });
  const upNotPrevented = target.dispatchEvent(up);
  if (activates && key === ' ' && notPrevented && upNotPrevented) target.click();
  webview.tick(20);
}

/** The accessible name of `element`: `aria-labelledby`, else `aria-label`, else its text, else `title`. */
export function accessibleName(element) {
  const doc = element.ownerDocument;
  const ids = element.getAttribute('aria-labelledby');
  if (ids) {
    return ids
      .split(/\s+/)
      .map((id) => text(doc.getElementById(id)))
      .join(' ')
      .trim();
  }
  const label = element.getAttribute('aria-label');
  if (label !== null) return label.replace(/\s+/g, ' ').trim();
  return spoken(element) || (element.getAttribute('title') ?? '').trim();
}

/** The texts of the polite live regions (X14), joined. */
export function announced(webview) {
  return [...webview.document.querySelectorAll('[aria-live="polite"], [role="status"]')].map(text).join(' ');
}

/** Whether `element` takes part in sequential focus navigation, and its tabindex. */
function tabIndex(window, element) {
  if (element.disabled || !isShown(window, element)) return null;
  const attribute = element.getAttribute('tabindex');
  if (attribute !== null) return Number(attribute);
  const natural = ['BUTTON', 'SELECT', 'TEXTAREA'].includes(element.tagName) || (element.tagName === 'INPUT' && element.type !== 'hidden') || (element.tagName === 'A' && element.hasAttribute('href'));
  return natural ? 0 : null;
}

/** Every element Tab reaches, in the order it reaches them (positive tabindex first). */
export function tabOrder(webview) {
  const all = [...webview.document.querySelectorAll('body *')]
    .map((element) => ({ element, index: tabIndex(webview.window, element) }))
    .filter(({ index }) => index !== null && index >= 0);
  const positive = all.filter(({ index }) => index > 0).sort((a, b) => a.index - b.index);
  return [...positive, ...all.filter(({ index }) => index === 0)].map(({ element }) => element);
}

/** The scope parts of the pattern editor: its chips list, input, Add and error element. */
export function scope(webview, name) {
  const doc = webview.document;
  const inputs = [...doc.querySelectorAll('tr.machine-detail input.heavy-input')];
  const input =
    doc.querySelector(`tr.machine-detail input.heavy-input[data-scope="${name}"]`) ?? inputs[name === 'machine' ? 0 : 1] ?? null;
  const list = doc.querySelector(`tr.machine-detail ul.heavy-chips[data-scope="${name}"]`);
  let box = input;
  while (box && box.parentElement && box.parentElement.querySelectorAll('input.heavy-input').length === 1) box = box.parentElement;
  const add = box?.querySelector('button.heavy-add') ?? null;
  const error = box?.querySelector('.heavy-error') ?? null;
  return { input, list, add, error, box };
}

/** A key for each element Tab reaches from the machine button to the first row, for the X20 and X22 order. */
export function stopsAfterButton(webview) {
  const order = tabOrder(webview);
  const start = order.indexOf(button(webview.document));
  const keys = [];
  for (const element of order.slice(start + 1)) {
    if (element.matches('tr[data-row-id]')) {
      keys.push('row');
      break;
    }
    keys.push(stopKey(webview, element));
  }
  return keys;
}

function stopKey(webview, element) {
  const plain = [
    ['input.job-limit', 'limit'],
    ['select.job-enforce', 'enforce'],
    ['button.job-save', 'save'],
    ['button.job-default', 'default'],
    ['button.suggest-all', 'suggest-all'],
  ];
  for (const [selector, key] of plain) if (element.matches(selector)) return key;
  const chips = element.closest('ul.heavy-chips[data-scope]');
  if (chips) return `chips:${chips.dataset.scope}`;
  if (element.closest('ul.heavy-suggested')) return 'suggested';
  for (const name of ['machine', 'repository']) {
    const parts = scope(webview, name);
    if (element === parts.input) return `input:${name}`;
    if (element === parts.add) return `add:${name}`;
  }
  return `${element.tagName.toLowerCase()}.${[...element.classList].join('.')}`;
}

/** The chips of one scope: each pattern with its tag text and its remove button. */
export function chips(webview, name) {
  const list = webview.document.querySelector(`tr.machine-detail ul.heavy-chips[data-scope="${name}"]`);
  return [...(list?.querySelectorAll('li.heavy-chip') ?? [])].map((chip) => ({
    chip,
    text: text(chip),
    remove: chip.querySelector('button.chip-remove'),
  }));
}

/** The suggestion chips in document order, each with its `data-from` and add button. */
export function suggestions(webview) {
  return [...webview.document.querySelectorAll('tr.machine-detail ul.heavy-suggested li.heavy-chip')].map((chip) => ({
    chip,
    from: chip.getAttribute('data-from'),
    text: text(chip),
    add: chip.querySelector('button.suggest-add'),
  }));
}

/** Posts the extension's answer to a jobSettings message (X21). */
export function answer(webview, fields) {
  webview.post({ type: 'jobSettings', version: 1, ...fields });
}

/** The messages the webview posted since `from`, jobSettings only. */
export function settingsSince(webview, from) {
  return webview.posted.slice(from).filter((message) => message?.type === 'jobSettings');
}

/** U13's visually hidden prefix of an actor's line. */
export function statePrefix(actor) {
  return actor.state === 'idle' && actor.error ? 'idle, API error: ' : `${actor.state}: `;
}

/**
 * What U12 and U13 put in a row: the `.count` text, whether it is hidden from screen readers,
 * the Activity cell as a screen reader reads it, and each agent line's spoken start, its first
 * element and whether that is a hidden `.line-icon`.
 */
export function agentFacts(window, row) {
  const cells = [...row.children].filter((child) => child.tagName === 'TD' && !child.classList.contains('actions'));
  const count = cells[0]?.querySelector('.count') ?? null;
  const hiddenUp = (node, stop) => {
    for (let at = node; at && at !== stop.parentElement; at = at.parentElement) if (at.getAttribute('aria-hidden') === 'true') return true;
    return false;
  };
  const lines = [...(cells[4]?.querySelectorAll('.line') ?? [])];
  return {
    count: count && isDisplayed(window, count) ? text(count) : null,
    countHidden: count ? hiddenUp(count, cells[0]) : null,
    activitySpoken: spoken(cells[0]),
    lines: lines.map((line) => {
      const icon = line.firstElementChild;
      const hiddenText = [...line.querySelectorAll('*')].filter((el) => isVisuallyHidden(window, el)).map(text).join('');
      return {
        spoken: spoken(line),
        iconFirst: Boolean(icon?.classList.contains('line-icon')),
        iconHidden: icon ? hiddenUp(icon, line) : false,
        hiddenText,
      };
    }),
  };
}

/** The problems of a row against U12 and U13 for `worktree`, the word its Activity shows. */
export function agentProblems(window, row, worktree, word) {
  const facts = agentFacts(window, row);
  const problems = [];
  const n = worktree.actors.length;
  if (n >= 2) {
    if (facts.count !== `×${n}`) problems.push(`no .count ×${n} after the Activity word (U12): ${facts.count}`);
    if (!facts.countHidden) problems.push('the count is not aria-hidden (U12)');
    if (facts.activitySpoken !== `${word}, ${n} agents`) problems.push(`the Activity cell reads "${facts.activitySpoken}", not "${word}, ${n} agents" (U12)`);
  } else if (facts.count !== null) {
    problems.push(`a row with ${n} agent lines shows the count ${facts.count} (U12)`);
  }
  shownActors(worktree).forEach((actor, index) => {
    const line = facts.lines[index];
    const prefix = statePrefix(actor);
    if (!line) {
      problems.push(`no line ${index + 1} for ${actor.key}`);
      return;
    }
    if (!line.iconFirst) problems.push(`${actor.key}: the line does not start with a .line-icon (U13)`);
    if (line.iconFirst && !line.iconHidden) problems.push(`${actor.key}: the line icon is not aria-hidden (U13)`);
    if (!line.spoken.startsWith(prefix.trim()) || !line.hiddenText.replace(/\s+/g, ' ').trim().startsWith(prefix.trim())) {
      problems.push(`${actor.key}: the line has no visually hidden "${prefix}" at its start (U13): "${line.spoken}"`);
    }
  });
  return problems;
}
