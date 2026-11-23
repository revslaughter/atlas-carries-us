// The book routes. app.ts mounts them under /books.

import { type Context, Hono } from "hono";

import type { BookID } from "../domain/book.js";
import { client } from "./app.js";

/** The book routes, as one Hono app. */
export const books = new Hono();

books.get("/:id", async c => c.json(await client.getBook(c.req.param("id") as BookID)));
books.post("/:id/loans", borrowBook);

/** Lends a book to the member that the request body names. */
async function borrowBook(c: Context) {
  const { member } = await c.req.json<{ member: string }>();
  return c.json(await client.borrow(c.req.param("id") as BookID, member), 201);
}
