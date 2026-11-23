// The readers' web app, on Hono. It mounts the book routes under /books, the
// loan routes under /api, and serves the static files.

import { type Context, Hono, serveStatic } from "hono";

import { HttpClient } from "../client/api.js";
import { CachingClient } from "../client/cache.js";
import { books } from "./books.js";

/** The client that every handler shares. */
export const client = new CachingClient("http://localhost:8080", fetch);

/** The app: every reader's request starts here. */
export const app = new Hono();

app.get("/health", c => c.text("ok"));
app.on(["GET", "POST"], "/version", version);
app.route("/books", books);
app.get("/static/*", serveStatic({ root: "./public" }));

const api = new Hono().basePath("/api");
api.get("/loans/:member", async c => c.json(await client.loans(c.req.param("member"))));
app.route("/", api);

/** Answers with the app's version and the number of clients. */
export function version(c: Context) {
  return c.json({ version: "0.1.0", clients: HttpClient.count });
}
