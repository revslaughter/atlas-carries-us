// The data of every page: who reads.

/** Names the reader of the request, for the title bar. */
export async function load({ locals }: { locals: { reader?: string } }) {
  return { reader: locals.reader || "guest" };
}
