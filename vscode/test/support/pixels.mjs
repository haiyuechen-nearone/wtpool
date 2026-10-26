// Pixel reading for the visual checks: a blank Chromium page decodes screenshots and measures
// regions of them. Nothing here touches the panel's page.

/** A blank page in its own context of `browser`, for decoding PNGs. */
export async function pixelHelper(browser) {
  const context = await browser.newContext();
  const page = await context.newPage();
  return { page, close: () => context.close() };
}

/**
 * The ink of each region of a PNG taken at `scale`: the bounds, in CSS px, of the pixels that
 * differ by more than `threshold` in some channel from the background, the colour `ref`
 * ([r, g, b]) or else the pixel at `bg` (CSS px).
 */
export async function inkBounds(helper, png, scale, regions) {
  return helper.page.evaluate(
    async ({ b64, s, list }) => {
      const bitmap = await createImageBitmap(await (await fetch(`data:image/png;base64,${b64}`)).blob());
      const canvas = new OffscreenCanvas(bitmap.width, bitmap.height);
      const ctx = canvas.getContext('2d');
      ctx.drawImage(bitmap, 0, 0);
      const data = ctx.getImageData(0, 0, bitmap.width, bitmap.height).data;
      const at = (x, y) => {
        const i = (y * bitmap.width + x) * 4;
        return [data[i], data[i + 1], data[i + 2]];
      };
      const out = {};
      for (const { key, rect, bg, ref: given, threshold } of list) {
        const ref = given ?? at(Math.round(bg.x * s), Math.round(bg.y * s));
        let [x0, y0, x1, y1, count] = [Infinity, Infinity, -Infinity, -Infinity, 0];
        const X0 = Math.max(0, Math.round(rect.x * s));
        const Y0 = Math.max(0, Math.round(rect.y * s));
        const X1 = Math.min(bitmap.width, Math.round((rect.x + rect.w) * s));
        const Y1 = Math.min(bitmap.height, Math.round((rect.y + rect.h) * s));
        for (let y = Y0; y < Y1; y += 1) {
          for (let x = X0; x < X1; x += 1) {
            const p = at(x, y);
            if (Math.max(Math.abs(p[0] - ref[0]), Math.abs(p[1] - ref[1]), Math.abs(p[2] - ref[2])) > threshold) {
              count += 1;
              x0 = Math.min(x0, x);
              y0 = Math.min(y0, y);
              x1 = Math.max(x1, x);
              y1 = Math.max(y1, y);
            }
          }
        }
        out[key] = count ? { count, cx: (x0 + x1 + 1) / 2 / s, cy: (y0 + y1 + 1) / 2 / s, left: x0 / s, right: (x1 + 1) / s, top: y0 / s, bottom: (y1 + 1) / s } : { count: 0 };
      }
      return out;
    },
    { b64: png.toString('base64'), s: scale, list: regions },
  );
}

/** How many pixels of `rect` (CSS px) differ by more than 8 in some channel between two PNGs. */
export async function changedPixels(helper, a, b, rect, scale) {
  return helper.page.evaluate(
    async ({ A, B, r, s }) => {
      const load = async (b64) => {
        const bitmap = await createImageBitmap(await (await fetch(`data:image/png;base64,${b64}`)).blob());
        const canvas = new OffscreenCanvas(bitmap.width, bitmap.height);
        const ctx = canvas.getContext('2d');
        ctx.drawImage(bitmap, 0, 0);
        return ctx.getImageData(Math.round(r.x * s), Math.round(r.y * s), Math.max(1, Math.round(r.w * s)), Math.max(1, Math.round(r.h * s))).data;
      };
      const [p, q] = [await load(A), await load(B)];
      let changed = 0;
      for (let i = 0; i < p.length; i += 4) {
        if (Math.max(Math.abs(p[i] - q[i]), Math.abs(p[i + 1] - q[i + 1]), Math.abs(p[i + 2] - q[i + 2])) > 8) changed += 1;
      }
      return { changed, of: p.length / 4 };
    },
    { A: a.toString('base64'), B: b.toString('base64'), r: rect, s: scale },
  );
}
