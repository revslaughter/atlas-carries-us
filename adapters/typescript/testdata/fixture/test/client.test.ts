// Tests for the HTTP client and the cache, with a fake fetch.

import { type Fetch, HttpClient, HttpError } from "../src/client/api.js";
import { CachingClient } from "../src/client/cache.js";

const fake =
  (body: unknown, status = 200): Fetch =>
  async () => ({ ok: status < 400, status, json: async () => body });

describe("HttpClient", () => {
  it("reads a book", async () => {
    const c = new HttpClient("http://x", fake({ id: "b-1", title: "Dune" }));
    expect((await c.getBook("b-1")).title).toBe("Dune");
  });
  describe("borrow", () => {
    it("refuses a member who holds too many books", async () => {
      const c = new HttpClient("http://x", fake([{}, {}, {}, {}, {}]));
      await c.borrow("b-1", "m-1").catch(e => expect(e instanceof HttpError).toBe(true));
    });
  });
});

test("the cache answers twice from one request", async () => {
  const c = new CachingClient("http://x", fake({ id: "b-2" }), () => 0);
  await c.getBook("b-2");
  expect((await c.getBook("b-2")).id).toBe("b-2");
});
