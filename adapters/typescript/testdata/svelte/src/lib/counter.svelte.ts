// A counter whose count lives in a rune, so that any component can share it.

/** A count that a component reads and raises. */
export interface Counter {
  /** The count now. */
  readonly count: number;
  /** Raises the count by one. */
  increment(): void;
}

/** Makes a counter that starts at `start`. */
export function createCounter(start = 0): Counter {
  let count = $state(start);
  return {
    get count() {
      return count;
    },
    increment() {
      count += 1;
    },
  };
}
