/**
 * @packageDocumentation
 * The web side of the invented lending library. A Hono app serves readers, an
 * Express app serves staff, and a typed client talks to the Go lending API.
 */

export { HttpClient } from "./client/api.js";
export * from "./domain/index.js";
export { app } from "./web/app.js";
