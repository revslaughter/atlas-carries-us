// The globals that the fixture's tests and pages use: a small test runner and
// the JSX namespace. Both stand in for packages that the fixture does not
// install.

declare function describe(name: string, fn: () => void): void;
declare function it(name: string, fn: () => unknown): void;
declare namespace it {
  function each<T>(cases: T[]): (name: string, fn: (c: T) => unknown) => void;
}
declare function test(name: string, fn: () => unknown): void;
declare function expect(value: unknown): { toBe(want: unknown): void };

declare namespace JSX {
  type Element = string;
  interface IntrinsicElements {
    [name: string]: unknown;
  }
  interface IntrinsicAttributes {
    key?: string;
  }
}
