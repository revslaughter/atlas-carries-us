// Tests for the loan rule. The file sits beside the code it tests.

import { MAX_LOANS, mayBorrow } from "./loan.js";

describe("mayBorrow", () => {
  it.each([0, 1, 2])("allows %i loans", n => expect(mayBorrow(new Array(n))).toBe(true));
  it(`stops at ${MAX_LOANS}`, () => expect(mayBorrow(new Array(MAX_LOANS))).toBe(false));
});
