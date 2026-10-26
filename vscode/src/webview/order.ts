/** The place before the first child of `firstIn`, for a list that has no element to follow. */
export interface Start {
  firstIn: Element;
}

/**
 * Puts `items` in order right after `anchor`, or at the start of a parent. An item already in
 * place is not touched, as a move would drop its focus.
 */
export function placeInOrder(anchor: Element | Start, items: readonly Element[]): void {
  let previous: Element | null = 'firstIn' in anchor ? null : anchor;
  const parent = 'firstIn' in anchor ? anchor.firstIn : null;
  for (const item of items) {
    const current = previous ? previous.nextElementSibling : (parent?.firstElementChild ?? null);
    if (current !== item) {
      if (previous) previous.after(item);
      else parent?.prepend(item);
    }
    previous = item;
  }
}

/** Keeps the views made for a list by key, so a later document updates them instead of remaking. */
export class KeyedCache<V> extends Map<string, V> {
  /** Gives the view of `key`, made by `make` on first use. */
  obtain(key: string, make: () => V): V {
    let value = this.get(key);
    if (value === undefined) {
      value = make();
      this.set(key, value);
    }
    return value;
  }

  /** Drops the views whose key is not in `keys`, each through `dispose`. */
  retain(keys: ReadonlySet<string>, dispose: (value: V, key: string) => void): void {
    for (const [key, value] of this) {
      if (keys.has(key)) continue;
      dispose(value, key);
      this.delete(key);
    }
  }
}
