// In-page measurements of the panel's layout and colours. `installHelpers` runs once per page
// through page.evaluate and defines window.__visual. Each other export runs through page.evaluate
// after it and uses nothing else from outside.
//
// The tab's horizontal scroller is the viewport or an ancestor of the table with overflow-x auto
// or scroll. Content inside such a scroller counts as reachable when it lies in its scrollable
// area. Every other box that clips (overflow not visible) cuts the part outside its padding box.

/** Defines window.__visual: geometry, clipping and WCAG colour helpers. */
export function installHelpers() {
  if (window.__visual) return;
  const px = (v) => parseFloat(v) || 0;
  const st = (el, pseudo) => getComputedStyle(el, pseudo);
  const n = (v) => Math.round(v * 10) / 10;

  const parse = (c) => {
    const m = /rgba?\(([^)]+)\)/.exec(c ?? '');
    if (!m) return null;
    const p = m[1].split(/[\s,/]+/).filter(Boolean).map(Number);
    return { r: p[0], g: p[1], b: p[2], a: p.length > 3 ? p[3] : 1 };
  };
  const over = (top, under) => ({
    r: top.r * top.a + under.r * (1 - top.a),
    g: top.g * top.a + under.g * (1 - top.a),
    b: top.b * top.a + under.b * (1 - top.a),
    a: 1,
  });
  const lum = (c) => {
    const f = (v) => {
      const s = v / 255;
      return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
    };
    return 0.2126 * f(c.r) + 0.7152 * f(c.g) + 0.0722 * f(c.b);
  };
  const ratio = (a, b) => {
    const [x, y] = [lum(a), lum(b)].sort((p, q) => q - p);
    return (x + 0.05) / (y + 0.05);
  };
  const hex = (c) => `#${[c.r, c.g, c.b].map((v) => Math.round(v).toString(16).padStart(2, '0')).join('')}`;
  const withAlpha = (c, a) => ({ ...c, a: c.a * a });

  const opacity = (el) => {
    let o = 1;
    for (let e = el; e; e = e.parentElement) o *= Number(st(e).opacity);
    return o;
  };
  /** VS Code paints the editor background behind a webview whose page is transparent. */
  const base = () => {
    const probe = document.createElement('span');
    probe.style.color = 'var(--vscode-editor-background)';
    document.body.append(probe);
    const c = parse(st(probe).color);
    probe.remove();
    return c && c.a > 0 ? over(c, { r: 255, g: 255, b: 255, a: 1 }) : { r: 255, g: 255, b: 255, a: 1 };
  };
  /** The composited background behind `el`'s own content: its and its ancestors' backgrounds. */
  const bgOf = (el) => {
    const chain = [];
    for (let e = el; e; e = e.parentElement) chain.push(e);
    let bg = base();
    for (const e of chain.reverse()) {
      const c = parse(st(e).backgroundColor);
      if (c && c.a > 0) bg = over(withAlpha(c, opacity(e)), bg);
    }
    return bg;
  };
  /** `colour` (a computed CSS colour) as painted by `el` over `under`. */
  const paint = (colour, el, under) => {
    const c = parse(colour);
    return c ? over(withAlpha(c, opacity(el)), under) : under;
  };

  const desc = (el) => {
    if (!el || el.nodeType !== 1) return String(el);
    const cls = typeof el.className === 'string' && el.className.trim() ? `.${el.className.trim().split(/\s+/).join('.')}` : '';
    const row = el.closest('tr[data-row-id]');
    const where = row ? `[${row.dataset.rowId.split('/').pop()}]` : el.closest('tr.detail') ? '[detail]' : '';
    return `${el.tagName.toLowerCase()}${cls}${where}`;
  };
  const COLUMNS = ['Activity', 'Worktree', 'Branch', 'Git', 'Agent · doing', 'Last', 'actions'];
  /** Where `el` sits on the panel, for messages: a heading label, a column, the strip, a chip. */
  const place = (el) => {
    const th = el.closest('th');
    if (th) return `heading ${th.textContent.trim() || 'actions'}`;
    if (el.closest('tr[data-machine]')) return 'machine heading';
    if (el.closest('tr.detail')) return 'detail strip';
    const td = el.closest('tr[data-row-id] > td');
    if (td) return `${COLUMNS[[...td.parentElement.children].indexOf(td)] ?? 'a'} column`;
    if (el.closest('.chip')) return 'chip';
    if (el.closest('.header')) return 'header';
    return 'page';
  };
  const shown = (el) => el.checkVisibility({ opacityProperty: true, visibilityProperty: true });
  /** Screen reader only text: inside a box of at most 1 px or clipped away by `clip`/`clip-path`. */
  const srOnly = (el) => {
    for (let e = el; e && e !== document.body; e = e.parentElement) {
      const r = e.getBoundingClientRect();
      const s = st(e);
      if (r.width <= 1.5 && r.height <= 1.5 && s.overflowX !== 'visible') return true;
      if (s.clip && s.clip !== 'auto') return true;
      if (s.clipPath && /inset\(50%/.test(s.clipPath)) return true;
    }
    return false;
  };
  const paddingBox = (el) => {
    const r = el.getBoundingClientRect();
    const s = st(el);
    return {
      left: r.left + px(s.borderLeftWidth),
      right: r.right - px(s.borderRightWidth),
      top: r.top + px(s.borderTopWidth),
      bottom: r.bottom - px(s.borderBottomWidth),
    };
  };
  const contentBox = (el) => {
    const p = paddingBox(el);
    const s = st(el);
    return { left: p.left + px(s.paddingLeft), right: p.right - px(s.paddingRight), top: p.top + px(s.paddingTop), bottom: p.bottom - px(s.paddingBottom) };
  };
  const cut = (a, b) => ({ left: Math.max(a.left, b.left), right: Math.min(a.right, b.right), top: Math.max(a.top, b.top), bottom: Math.min(a.bottom, b.bottom) });
  const has = (r) => r.right - r.left > 0.5 && r.bottom - r.top > 0.5;
  const scrolls = (v) => v === 'auto' || v === 'scroll';

  const tabScrollers = () => {
    const table = document.querySelector('table');
    const found = [];
    for (let e = table?.parentElement; e && e !== document.body && e !== document.documentElement; e = e.parentElement) {
      if (scrolls(st(e).overflowX)) found.push(e);
    }
    return found;
  };
  /** True when <body>'s overflow goes to the viewport (CSS overflow propagation). */
  const bodyPropagates = () => st(document.documentElement).overflowX === 'visible' && st(document.documentElement).overflowY === 'visible';
  const viewportOverflow = () => (bodyPropagates() ? st(document.body) : st(document.documentElement));
  /** The area `el` lets its content show in, in client coordinates, per axis. */
  const region = (el, allowed) => {
    const s = st(el);
    const p = paddingBox(el);
    const out = { left: -Infinity, right: Infinity, top: -Infinity, bottom: Infinity };
    const scrollArea = allowed.includes(el);
    if (s.overflowX !== 'visible') {
      if (scrollArea) {
        out.left = p.left - el.scrollLeft;
        out.right = out.left + el.scrollWidth;
      } else {
        out.left = p.left;
        out.right = p.right;
      }
    }
    if (s.overflowY !== 'visible') {
      if (scrollArea || scrolls(s.overflowY)) {
        out.top = p.top - el.scrollTop;
        out.bottom = out.top + el.scrollHeight;
      } else {
        out.top = p.top;
        out.bottom = p.bottom;
      }
    }
    return out;
  };
  const viewportRegion = () => {
    const root = document.scrollingElement ?? document.documentElement;
    const s = viewportOverflow();
    const x = scrolls(s.overflowX) || s.overflowX === 'visible';
    const y = scrolls(s.overflowY) || s.overflowY === 'visible';
    return {
      left: x ? -window.scrollX : 0,
      right: x ? root.scrollWidth - window.scrollX : root.clientWidth,
      top: y ? -window.scrollY : 0,
      bottom: y ? root.scrollHeight - window.scrollY : root.clientHeight,
    };
  };
  /**
   * The part of `rect` that can show: cut by every box from `from` up that clips, a tab scroller
   * by its scrollable area, and by the viewport's. Inside a tab scroller nothing above it counts:
   * scrolling it brings its content into view. `cutters` lists the boxes that cut > 0.5 px.
   */
  const visible = (rect, from) => {
    const allowed = tabScrollers();
    let v = { left: rect.left, right: rect.right, top: rect.top, bottom: rect.bottom };
    const cutters = [];
    const apply = (area, who) => {
      const next = cut(v, area);
      const lost = Math.max(next.left - v.left, v.right - next.right, next.top - v.top, v.bottom - next.bottom);
      if (lost > 0.5) cutters.push({ by: who, px: n(lost) });
      v = next;
    };
    for (let e = from; e && e !== document.documentElement; e = e.parentElement) {
      if (e === document.body && bodyPropagates()) break;
      const s = st(e);
      if (s.overflowX !== 'visible' || s.overflowY !== 'visible') apply(region(e, allowed), desc(e));
      if (allowed.includes(e)) return { v, cutters };
    }
    apply(viewportRegion(), 'viewport');
    return { v, cutters };
  };
  /** Every shown, non-blank text node below `root`, with its line boxes. */
  const texts = (root) => {
    const found = [];
    const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
    for (let node = walker.nextNode(); node; node = walker.nextNode()) {
      if (!node.textContent.trim()) continue;
      const parent = node.parentElement;
      if (!parent || !shown(parent) || srOnly(parent) || parent.closest('script, style, [hidden]')) continue;
      const range = document.createRange();
      range.selectNodeContents(node);
      const rects = [...range.getClientRects()].filter(has);
      if (rects.length) found.push({ node, parent, rects, text: node.textContent.trim().slice(0, 60) });
    }
    return found;
  };

  /** The part of the viewport where `el` can show: inside every tab scroller that holds it. */
  const viewOf = (el) => {
    const root = document.documentElement;
    let view = { left: 0, right: root.clientWidth, top: 0, bottom: root.clientHeight };
    for (const e of tabScrollers().filter((s) => s.contains(el))) {
      const r = e.getBoundingClientRect();
      const left = r.left + e.clientLeft;
      const top = r.top + e.clientTop;
      view = cut(view, { left, right: left + e.clientWidth, top, bottom: top + e.clientHeight });
    }
    return view;
  };

  window.__visual = { px, st, n, parse, over, lum, ratio, hex, opacity, bgOf, paint, desc, place, shown, srOnly, paddingBox, contentBox, cut, has, tabScrollers, region, visible, texts, viewportRegion, viewOf };
}

/**
 * Finds text clipped or spilling, `…` in effect or in the text, wrapped table text, and a table
 * with a side the tab cannot scroll to. `sourceHasEllipsis` is true when the document itself holds
 * a `…`.
 */
export function clipping(sourceHasEllipsis) {
  const V = window.__visual;
  const issues = [];
  const push = (kind, detail) => issues.push({ kind, ...detail });
  const allowed = V.tabScrollers();
  for (const t of V.texts(document.body)) {
    for (const rect of t.rects) {
      const { cutters } = V.visible(rect, t.parent);
      for (const c of cutters) push('text-clipped', { place: V.place(t.parent), el: V.desc(t.parent), text: t.text, by: c.by, px: c.px });
    }
    if (!sourceHasEllipsis && t.node.textContent.includes('…')) push('cut-with-ellipsis-in-text', { place: V.place(t.parent), el: V.desc(t.parent), text: t.text });
  }
  for (const el of document.body.querySelectorAll('*')) {
    if (allowed.includes(el) || !V.shown(el) || V.srOnly(el) || el.closest('[hidden], script, style')) continue;
    if (!el.textContent.trim() || el.clientWidth === 0) continue;
    const s = V.st(el);
    const over = el.scrollWidth - el.clientWidth;
    if (over > 1) {
      const kind = s.textOverflow === 'ellipsis' && s.overflowX !== 'visible' ? 'cut-with-ellipsis' : 'content-wider-than-its-box';
      push(kind, { place: V.place(el), el: V.desc(el), text: el.textContent.trim().slice(0, 60), px: over });
    }
  }
  // One line per table cell, per `.line`, per heading label and per chip: no squeeze into lines.
  const groups = [];
  for (const th of document.querySelectorAll('thead th')) groups.push(th);
  for (const td of document.querySelectorAll('tr[data-row-id] > td, tr[data-machine] > td')) {
    const lines = td.querySelectorAll('.line');
    if (lines.length) groups.push(...lines);
    else groups.push(td);
  }
  groups.push(...document.querySelectorAll('.chip'));
  for (const g of groups) {
    const rects = V.texts(g).flatMap((t) => t.rects);
    if (rects.length < 2) continue;
    const top = Math.max(...rects.map((r) => r.top));
    const bottom = Math.min(...rects.map((r) => r.bottom));
    if (top > bottom - 1) push('text-wraps', { place: V.place(g), el: V.desc(g), text: g.textContent.trim().slice(0, 60), lines: new Set(rects.map((r) => Math.round(r.top))).size });
  }
  // The table's left and right ends can be scrolled into view.
  const table = document.querySelector('table');
  const facts = { table: Math.round(table.getBoundingClientRect().width), tab: document.documentElement.clientWidth };
  const saved = allowed.map((e) => e.scrollLeft);
  const savedX = window.scrollX;
  const end = (toEnd) => {
    allowed.forEach((e) => {
      e.scrollLeft = toEnd ? e.scrollWidth : 0;
    });
    window.scrollTo(toEnd ? document.documentElement.scrollWidth : 0, window.scrollY);
    const r = table.getBoundingClientRect();
    let room = { left: 0, right: document.documentElement.clientWidth };
    for (const e of allowed) room = { left: Math.max(room.left, V.paddingBox(e).left), right: Math.min(room.right, V.paddingBox(e).right) };
    return toEnd ? r.right - room.right : room.left - r.left;
  };
  const right = end(true);
  const left = end(false);
  allowed.forEach((e, i) => {
    e.scrollLeft = saved[i];
  });
  window.scrollTo(savedX, window.scrollY);
  if (right > 0.5) push('table-end-out-of-reach', { side: 'right', px: V.n(right) });
  if (left > 0.5) push('table-end-out-of-reach', { side: 'left', px: V.n(left) });
  return { issues, facts };
}

/**
 * Checks that each row's tag (`expected[rowId]`, `fg` or `unmanaged`) shows whole, inside its
 * Worktree cell's content box, after the name and on its line.
 */
export function tags(expected) {
  const V = window.__visual;
  const issues = [];
  for (const tr of document.querySelectorAll('tr[data-row-id]')) {
    const id = tr.dataset.rowId;
    const want = expected[id];
    if (!want) continue;
    const row = id.split('/').pop();
    const push = (kind, detail = {}) => issues.push({ kind, row, tag: want.tag, ...detail });
    const cell = tr.querySelectorAll(':scope > td')[1];
    const nodes = [];
    const walker = document.createTreeWalker(cell, NodeFilter.SHOW_TEXT);
    for (let node = walker.nextNode(); node; node = walker.nextNode()) nodes.push(node);
    const full = nodes.map((node) => node.textContent).join('');
    const end = full.trimEnd().length;
    const start = full.length - full.trimStart().length;
    if (!full.trimEnd().endsWith(want.tag) || full.slice(start, start + want.name.length) !== want.name) {
      push('cell-text', { text: full.replace(/\s+/g, ' ').trim() });
      continue;
    }
    // A start on a node boundary goes to the next node, an end to the earlier one.
    const at = (offset, isEnd) => {
      let left = offset;
      for (const node of nodes) {
        const length = node.textContent.length;
        if (isEnd ? left <= length : left < length) return [node, left];
        left -= length;
      }
      return [nodes.at(-1), nodes.at(-1).textContent.length];
    };
    const rangeOf = (from, to) => {
      const range = document.createRange();
      range.setStart(...at(from, false));
      range.setEnd(...at(to, true));
      return range;
    };
    const tagRange = rangeOf(end - want.tag.length, end);
    const nameRange = rangeOf(start, start + want.name.length);
    const tagRects = [...tagRange.getClientRects()].filter(V.has);
    const nameRects = [...nameRange.getClientRects()].filter(V.has);
    const tagParent = tagRange.startContainer.parentElement;
    if (!tagRects.length || !V.shown(tagParent)) {
      push('tag-not-shown');
      continue;
    }
    const content = V.contentBox(cell);
    for (const r of tagRects) {
      const { cutters } = V.visible(r, tagParent);
      for (const c of cutters) push('tag-clipped', { by: c.by, px: c.px });
      if (r.left < content.left - 0.5 || r.right > content.right + 0.5) {
        push('tag-outside-its-cell', { tag_left: V.n(r.left), tag_right: V.n(r.right), cell_left: V.n(content.left), cell_right: V.n(content.right) });
      }
    }
    for (const r of nameRects) {
      for (const c of V.visible(r, nameRange.startContainer.parentElement).cutters) push('name-clipped', { by: c.by, px: c.px });
    }
    if (nameRects.length) {
      const last = nameRects.at(-1);
      const first = tagRects[0];
      if (first.left < last.right - 0.5) push('tag-not-after-the-name', { name_right: V.n(last.right), tag_left: V.n(first.left) });
      if (first.top >= last.bottom - 1 || first.bottom <= last.top + 1) push('tag-not-on-the-name-line', { name_top: V.n(last.top), tag_top: V.n(first.top) });
    }
    // A tag drawn as a box (border or background) keeps that box inside the cell too.
    const box = tagParent !== cell && tagParent.textContent.trim() === want.tag ? tagParent : null;
    if (box) {
      const s = V.st(box);
      const painted = (V.px(s.borderLeftWidth) > 0 && s.borderLeftStyle !== 'none') || (V.parse(s.backgroundColor)?.a ?? 0) > 0;
      const r = box.getBoundingClientRect();
      if (painted && (r.left < content.left - 0.5 || r.right > content.right + 0.5)) push('tag-box-outside-its-cell', { box_right: V.n(r.right), cell_right: V.n(content.right) });
      if (painted) for (const c of V.visible(r, box.parentElement).cutters) push('tag-box-clipped', { by: c.by, px: c.px });
    }
  }
  return issues;
}

/**
 * Measures the focused element's ring. It must exist, lie inside every box that clips it (a tab
 * scroller by its scrollable area) and have 3:1 against the background under it.
 */
export function ring() {
  const V = window.__visual;
  const el = document.activeElement;
  if (!el || el === document.body) return null;
  const s = V.st(el);
  const row = el.closest('tr[data-row-id]');
  const key = `${el.matches('tr[data-row-id]') ? 'row' : el.dataset.action ?? V.desc(el)} ${row ? row.dataset.rowId.split('/').pop() : ''}`.trim();
  const state = row ? `${row.getAttribute('aria-selected') === 'true' ? 'selected' : 'plain'}${row.matches(':hover') ? ' hovered' : ''}` : 'plain';
  const r = el.getBoundingClientRect();
  let width = 0;
  let offset = 0;
  let colour = null;
  if (s.outlineStyle !== 'none' && V.px(s.outlineWidth) > 0) {
    width = V.px(s.outlineWidth);
    offset = V.px(s.outlineOffset);
    colour = s.outlineColor;
  } else if (s.boxShadow !== 'none') {
    const m = /(rgba?\([^)]*\))\s+(-?[\d.]+)px\s+(-?[\d.]+)px\s+([\d.]+)px\s+(-?[\d.]+)px(\s+inset)?/.exec(s.boxShadow);
    if (m && (Number(m[5]) > 0 || Number(m[4]) > 0)) {
      colour = m[1];
      width = Number(m[5]) + Number(m[4]);
      offset = m[6] ? -width : 0;
    }
  }
  if (!colour || (V.parse(colour)?.a ?? 0) === 0) return { key, state, missing: true };
  // The tab scrolls, so the viewport cuts only the part outside the scrollable page.
  const grow = offset + width;
  const outer = { left: r.left - grow, right: r.right + grow, top: r.top - grow, bottom: r.bottom + grow };
  const { v: area, cutters } = V.visible(outer, el.parentElement);
  const sides = {
    left: V.n(area.left - outer.left),
    right: V.n(outer.right - area.right),
    top: V.n(area.top - outer.top),
    bottom: V.n(outer.bottom - area.bottom),
  };
  // The background under the ring: the topmost element at the middle of each side of the band,
  // the middle of the part in view for an element wider or taller than the tab.
  const mid = offset + width / 2;
  const cx = (Math.max(r.left, 0) + Math.min(r.right, window.innerWidth)) / 2;
  const cy = (Math.max(r.top, 0) + Math.min(r.bottom, window.innerHeight)) / 2;
  const points = [
    [cx, r.top - mid],
    [cx, r.bottom + mid],
    [r.left - mid, cy],
    [r.right + mid, cy],
  ];
  let worst = null;
  for (const [x, y] of points) {
    if (x < 0 || y < 0 || x >= window.innerWidth || y >= window.innerHeight) continue;
    const hit = document.elementFromPoint(x, y);
    if (!hit) continue;
    const under = V.bgOf(hit);
    const value = V.ratio(V.paint(colour, el, under), under);
    if (!worst || value < worst.ratio) worst = { ratio: Math.round(value * 100) / 100, ring: V.hex(V.paint(colour, el, under)), under: V.hex(under) };
  }
  return {
    key,
    state,
    missing: false,
    outline: `${width}px ${colour} offset ${offset}px`,
    lost: Object.fromEntries(Object.entries(sides).filter(([, v]) => v > 0.4)),
    clippedBy: cutters.map((c) => c.by),
    contrast: worst,
  };
}

/** Finds every shown text below 4.5:1 (3:1 when large) against its composited background. */
export function textContrast() {
  const V = window.__visual;
  const failures = new Map();
  for (const t of V.texts(document.body)) {
    const el = t.parent;
    const s = V.st(el);
    const bg = V.bgOf(el);
    const fg = V.paint(s.color, el, bg);
    const size = V.px(s.fontSize);
    const large = size >= 24 || (size >= 18.66 && Number(s.fontWeight) >= 700);
    const need = large ? 3 : 4.5;
    const value = V.ratio(fg, bg);
    if (value >= need - 0.005) continue;
    const row = el.closest('tr[data-row-id]');
    const state = row ? ` ${row.getAttribute('aria-selected') === 'true' ? 'selected' : 'plain'}${row.matches(':hover') ? ' hovered' : ''}` : '';
    // One entry per place, element class, row state and colour pair, with the first text as sample.
    const key = `${V.place(el)}: ${V.desc(el).replace(/\[.*\]$/, '')}${state} ${V.hex(fg)} on ${V.hex(bg)}`;
    if (!failures.has(key)) failures.set(key, { key, ratio: Math.round(value * 100) / 100, need, text: t.text, row: row ? row.dataset.rowId.split('/').pop() : null });
  }
  return [...failures.values()];
}

/**
 * Measures the boundary of each element of `selector` against its surroundings: its four borders,
 * its outline or a box-shadow ring against its parent's background, or its fill against `page`
 * (the background of `pageOf`, default its parent). Gives the best of them per element.
 */
export function boundaries(selector, pageOf = null) {
  const V = window.__visual;
  const out = [];
  for (const el of document.querySelectorAll(selector)) {
    if (!V.shown(el)) continue;
    const s = V.st(el);
    const outside = V.bgOf(el.parentElement);
    const page = pageOf ? V.bgOf(document.querySelector(pageOf)) : outside;
    const against = (colour) => V.ratio(V.paint(colour, el, outside), outside);
    const fill = V.ratio(V.bgOf(el), page);
    const sides = ['Top', 'Right', 'Bottom', 'Left'].map((side) => {
      const w = V.px(s[`border${side}Width`]);
      return w >= 1 && !['none', 'hidden'].includes(s[`border${side}Style`]) ? against(s[`border${side}Color`]) : 0;
    });
    const border = Math.min(...sides);
    const outline = s.outlineStyle !== 'none' && V.px(s.outlineWidth) >= 1 ? against(s.outlineColor) : 0;
    let shadow = 0;
    for (const m of s.boxShadow.matchAll(/(rgba?\([^)]*\))\s+-?[\d.]+px\s+-?[\d.]+px\s+[\d.]+px\s+([\d.]+)px/g)) {
      if (Number(m[2]) >= 1) shadow = Math.max(shadow, against(m[1]));
    }
    const best = Math.max(fill, border, outline, shadow);
    const round = (v) => Math.round(v * 100) / 100;
    out.push({ el: V.desc(el), text: el.textContent.replace(/\s+/g, ' ').trim().slice(0, 40), best: round(best), fill: round(fill), border: round(border), outline: round(outline), shadow: round(shadow) });
  }
  return out;
}

/**
 * Measures the detail strip's boundary. Candidates are `tr.detail` and each element in it with a
 * box that holds all of the strip's text. The best candidate's boundary counts.
 */
export function stripBoundary() {
  const V = window.__visual;
  const detail = document.querySelector('tr.detail');
  if (!detail) return null;
  const rects = V.texts(detail).flatMap((t) => t.rects);
  if (!rects.length) return { best: 0, candidates: [] };
  const all = {
    left: Math.min(...rects.map((r) => r.left)),
    right: Math.max(...rects.map((r) => r.right)),
    top: Math.min(...rects.map((r) => r.top)),
    bottom: Math.max(...rects.map((r) => r.bottom)),
  };
  const candidates = [detail, ...detail.querySelectorAll('*')].filter((el) => {
    const r = el.getBoundingClientRect();
    return r.left <= all.left + 1 && r.right >= all.right - 1 && r.top <= all.top + 1 && r.bottom >= all.bottom - 1;
  });
  const page = V.bgOf(detail.parentElement);
  const found = [];
  for (const el of candidates) {
    const s = V.st(el);
    const outside = V.bgOf(el.parentElement);
    const against = (colour) => V.ratio(V.paint(colour, el, outside), outside);
    const sides = ['Top', 'Right', 'Bottom', 'Left'].map((side) =>
      V.px(s[`border${side}Width`]) >= 1 && !['none', 'hidden'].includes(s[`border${side}Style`]) ? against(s[`border${side}Color`]) : 0,
    );
    const outline = s.outlineStyle !== 'none' && V.px(s.outlineWidth) >= 1 ? against(s.outlineColor) : 0;
    const fill = V.ratio(V.bgOf(el), page);
    found.push({ el: V.desc(el), fill: Math.round(fill * 100) / 100, border: Math.round(Math.min(...sides) * 100) / 100, outline: Math.round(outline * 100) / 100 });
  }
  return { best: Math.max(0, ...found.map((f) => Math.max(f.fill, f.border, f.outline))), candidates: found };
}

export function actionBoxes(rowId) {
  const row = [...document.querySelectorAll('tr[data-row-id]')].find((r) => r.dataset.rowId === rowId);
  if (!row) return [];
  return [...row.querySelectorAll('button[data-action]')]
    .filter((b) => b.checkVisibility({ opacityProperty: true, visibilityProperty: true }))
    .map((b) => {
      const r = b.getBoundingClientRect();
      return { action: b.dataset.action, x: r.left, y: r.top, w: r.width, h: r.height };
    });
}

/**
 * Gives each row's activity icon (the `::before` of its Activity cell or an element in it, or an
 * inline `<svg>` in the cell) and the background behind it.
 */
export function iconBoxes() {
  const V = window.__visual;
  return [...document.querySelectorAll('tr[data-row-id]')].map((row) => {
    const cell = row.querySelector(':scope > td');
    const glyph = [cell, ...cell.querySelectorAll('*')].find((el) => {
      const content = getComputedStyle(el, '::before').content;
      return content && content !== 'none' && content !== 'normal';
    });
    const icon = glyph ?? cell.querySelector('svg');
    const r = (icon ?? cell).getBoundingClientRect();
    const bg = V.bgOf(icon ?? cell);
    return { row: row.dataset.rowId.split('/').pop(), word: cell.textContent.trim(), found: Boolean(icon), x: r.left, y: r.top, w: r.width, h: r.height, bg: [bg.r, bg.g, bg.b].map(Math.round) };
  });
}
