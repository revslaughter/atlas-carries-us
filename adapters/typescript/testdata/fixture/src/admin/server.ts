// The staff server, on Express. Staff add books and read the loan report.

import express, { Router } from "express";

import type { HttpClient } from "../client/api.js";
import * as handlers from "./handlers.js";

/** The router for the staff pages. server mounts it under /admin. */
export const admin = Router();
admin.get("/books/:id", handlers.showBook);
admin.route("/books").get(handlers.listBooks).post(express.json(), handlers.addBook);

/**
 * Builds the staff server.
 *
 * @param client - the client for the Go service
 */
export function server(client: HttpClient) {
  const app = express();
  app.use("/admin", admin);
  app.get("/status", (_req, res) => res.json({ client: client.describe() }));
  routes(app);
  return app;
}

/** Adds the report routes. The fixture's config puts them under /reports. */
function routes(app: express.Express) {
  app.get("/loans", handlers.requireStaff, handlers.loanReport);
  app.delete("/cache", handlers.clearCache);
}
