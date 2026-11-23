// The server hook: it names the reader of each request.

import type { Handle } from "@sveltejs/kit";

/** Reads the reader's name from a cookie, for every request. */
export const handle: Handle = async ({ event, resolve }) => {
  event.locals.reader = event.cookies.get("reader") ?? "";
  return resolve(event);
};
