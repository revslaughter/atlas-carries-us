// Tests of the shelf: they feed the test index, never the map.

const { describe, it } = require("node:test");
const assert = require("node:assert");
const { Shelf } = require("../lib/domain/book");

describe("Shelf", () => {
  it("keeps books in order", () => {
    const s = Shelf.from("a", [{ id: "1", title: "One" }]);
    assert.equal(s.first().id, "1");
  });
});
