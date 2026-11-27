/**
 * Tests of the API client's pure parts: what a refused call says.
 *
 * @packageDocumentation
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { failure, SESSION_ENDED } from "../src/api";

test("a 401 says the session ended and how to keep the edits", () => {
  const d = failure(401, { error: "acu: the request lacks the session token" });
  assert.equal(d.error, SESSION_ENDED);
  assert.match(SESSION_ENDED, /^acu: /);
  assert.match(SESSION_ENDED, /Keep this tab open/);
  assert.equal(failure(401, null).error, SESSION_ENDED);
});

test("another failure keeps the server's own error", () => {
  const d = { error: "acu: the file changed" };
  assert.equal(failure(409, d), d);
});
