// In-page measurements of the tab's sideways scroll, for page.evaluate or frame.evaluate after
// installHelpers of support/measure.mjs. A tab scroller is an ancestor of the table with
// overflow-x auto or scroll. The view is the part of the viewport every tab scroller shows (inside
// its borders and scrollbars).

/** The page's own sideways overflow, the table's width and the tab scrollers. */
export function scrollState() {
  const V = window.__visual;
  const root = document.scrollingElement ?? document.documentElement;
  const table = document.querySelector('table');
  const scrollers = V.tabScrollers().map((e) => ({ el: V.desc(e), scrollWidth: e.scrollWidth, clientWidth: e.clientWidth }));
  return {
    page: root.scrollWidth - root.clientWidth,
    tab: root.clientWidth,
    table: Math.round(table.getBoundingClientRect().width),
    scrolls: scrollers.some((s) => s.scrollWidth > s.clientWidth + 1),
    scrollers,
  };
}

/** Scrolls every tab scroller to its start, or with `toEnd` to its end. */
export function scrollTable(toEnd) {
  for (const e of window.__visual.tabScrollers()) e.scrollLeft = toEnd ? e.scrollWidth : 0;
}

/** Scrolls every tab scroller, and the page, down to its bottom. */
export function scrollDown() {
  for (const e of window.__visual.tabScrollers()) e.scrollTop = e.scrollHeight;
  const root = document.scrollingElement ?? document.documentElement;
  window.scrollTo(window.scrollX, root.scrollHeight);
}

/**
 * The tab's height against the box that scrolls the table sideways: the viewport's height,
 * the page's own vertical overflow, the innermost tab scroller that scrolls sideways with its
 * bottom edge (where its horizontal scrollbar is drawn) and the rows it hides vertically, and
 * where the header parts are.
 */
export function tallLayout() {
  const V = window.__visual;
  const root = document.scrollingElement ?? document.documentElement;
  const table = document.querySelector('table');
  const box = V.tabScrollers().find((e) => e.scrollWidth > e.clientWidth + 1);
  const scroller = box && {
    el: V.desc(box),
    bottom: V.n(box.getBoundingClientRect().bottom),
    sideways: box.scrollWidth - box.clientWidth,
    hidden: box.scrollHeight - box.clientHeight,
  };
  const header = [...document.querySelectorAll('h1, h2, .chip, .connection')]
    .filter((el) => !table.contains(el) && V.shown(el))
    .map((el) => {
      const r = el.getBoundingClientRect();
      return { part: el.textContent.replace(/\s+/g, ' ').trim(), top: V.n(r.top), bottom: V.n(r.bottom) };
    });
  return { height: root.clientHeight, page: root.scrollHeight - root.clientHeight, table: Math.round(table.getBoundingClientRect().height), scroller, header };
}

/**
 * Where the header parts, each row's Last and actions cells and the detail strip are, and the
 * view. The header parts are the chips, the connection text and a heading outside the table.
 */
export function scrollLayout() {
  const V = window.__visual;
  const view = window.__visual.viewOf(document.querySelector('table'));
  const box = (r) => ({ left: V.n(r.left), right: V.n(r.right) });
  const table = document.querySelector('table');
  const header = [...document.querySelectorAll('h1, h2, .chip, .connection')]
    .filter((el) => !table.contains(el) && V.shown(el))
    .map((el) => ({ part: el.textContent.replace(/\s+/g, ' ').trim(), ...box(el.getBoundingClientRect()) }));
  const cells = [];
  for (const row of document.querySelectorAll('tr[data-row-id]')) {
    const tds = row.querySelectorAll(':scope > td');
    const name = row.dataset.rowId.split('/').pop();
    if (tds[5]) cells.push({ cell: `${name} Last`, ...box(tds[5].getBoundingClientRect()) });
    const actions = row.querySelector('td.actions');
    if (actions) cells.push({ cell: `${name} actions`, ...box(actions.getBoundingClientRect()) });
  }
  let strip = null;
  const detail = document.querySelector('tr.detail');
  if (detail) {
    const rects = V.texts(detail).flatMap((t) => t.rects);
    if (rects.length) {
      const all = { left: Math.min(...rects.map((r) => r.left)), right: Math.max(...rects.map((r) => r.right)) };
      // The innermost element that holds all of the strip's text: the strip as drawn.
      const holders = [detail, ...detail.querySelectorAll('*')].filter((el) => {
        const r = el.getBoundingClientRect();
        return r.left <= all.left + 1 && r.right >= all.right - 1;
      });
      strip = { text: box(all), box: box(holders.at(-1).getBoundingClientRect()), el: V.desc(holders.at(-1)) };
    }
  }
  return { view: box(view), header, cells, strip };
}

/**
 * The focused element against the view: a focused row must show its start, a focused action or
 * other control its whole box and focus ring.
 */
export function focusInView() {
  const V = window.__visual;
  const el = document.activeElement;
  if (!el || el === document.body) return null;
  const view = V.viewOf(el);
  const row = el.closest('tr[data-row-id]');
  const name = row ? row.dataset.rowId.split('/').pop() : '';
  const isRow = el.matches('tr[data-row-id]');
  const key = isRow ? `row ${name}` : el.matches('button[data-action]') ? `${el.dataset.action} ${name}` : V.desc(el);
  const s = V.st(el);
  let grow = 0;
  if (s.outlineStyle !== 'none') grow = V.px(s.outlineWidth) + V.px(s.outlineOffset);
  else for (const m of s.boxShadow.matchAll(/-?[\d.]+px\s+-?[\d.]+px\s+([\d.]+)px\s+(-?[\d.]+)px(?!\s+inset)/g)) grow = Math.max(grow, Number(m[1]) + Number(m[2]));
  grow = Math.max(0, grow);
  const r = el.getBoundingClientRect();
  const shown = isRow
    ? r.left >= view.left - 0.5 && r.left < view.right && r.top >= view.top - 0.5 && r.bottom <= view.bottom + 0.5
    : r.left - grow >= view.left - 0.5 && r.right + grow <= view.right + 0.5 && r.top - grow >= view.top - 0.5 && r.bottom + grow <= view.bottom + 0.5;
  return { key, isRow, shown, left: V.n(r.left), right: V.n(r.right), ring: V.n(grow), view: { left: V.n(view.left), right: V.n(view.right) } };
}
