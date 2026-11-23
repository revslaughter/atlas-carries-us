// The items API: list the items, add one, and refuse every other method.

import { plural } from "#lib/format.js";

/** The items, in memory. */
const items: string[] = [];

/** Lists the items, with a count. */
export async function GET() {
  return Response.json({ items, count: plural(items.length, "item") });
}

/** Adds the item in the request body. */
export const POST = async ({ request }: { request: Request }) => {
  items.push(String(await request.text()));
  return new Response(null, { status: 201 });
};

/** Refuses every other method. */
export function fallback() {
  return new Response(null, { status: 405 });
}
