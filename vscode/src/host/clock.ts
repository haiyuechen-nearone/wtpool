// The clock behind every time the extension keeps: the stale and restart deadlines and the
// restart backoff of the watch stream (X5, X6). Outside WTPOOL_VERIFY it is the system clock; under
// WTPOOL_VERIFY=1 it is a fake that moves only through `advance` (verification.md, "Test
// interfaces", item 3).

/** Cancels a timer; a second call does nothing. */
export type Cancel = () => void;

/** Tells the time and runs timers, on the system clock or a fake one. */
export interface Clock {
  /** Gives the milliseconds since the epoch. */
  now(): number;
  /** Runs `run` once, `ms` from now. */
  after(ms: number, run: () => void): Cancel;
}

/** The clock of the system: `Date.now` and `setTimeout`. */
export const systemClock: Clock = {
  now: () => Date.now(),
  after(ms, run) {
    const timer = setTimeout(run, ms);
    return () => clearTimeout(timer);
  },
};

/** One pending timer of a {@link FakeClock}. */
interface Timer {
  at: number;
  /** The order the timer was set in, the tie break between timers due at once. */
  order: number;
  run: () => void;
}

/** A clock that stands still until `advance` moves it, firing due timers in time order. */
export class FakeClock implements Clock {
  private order = 0;
  private readonly timers = new Set<Timer>();

  constructor(private time: number) {}

  now(): number {
    return this.time;
  }

  after(ms: number, run: () => void): Cancel {
    this.order += 1;
    const timer: Timer = { at: this.time + ms, order: this.order, run };
    this.timers.add(timer);
    return () => this.timers.delete(timer);
  }

  /** Moves the clock `ms` forward; each timer due by then fires at its own time. */
  advance(ms: number): void {
    const target = this.time + ms;
    for (let next = this.due(target); next; next = this.due(target)) {
      this.timers.delete(next);
      this.time = next.at;
      next.run();
    }
    this.time = target;
  }

  /** Finds the timer to fire next by `target`: the earliest, the first set among equals. */
  private due(target: number): Timer | undefined {
    let first: Timer | undefined;
    for (const timer of this.timers) {
      if (timer.at > target) {
        continue;
      }
      if (!first || timer.at < first.at || (timer.at === first.at && timer.order < first.order)) {
        first = timer;
      }
    }
    return first;
  }
}
