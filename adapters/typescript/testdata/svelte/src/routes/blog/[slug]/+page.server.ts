// The data and the form actions of a post page on the server.

import type { Actions, PageServerLoad } from "./$types";

import { slugTitle } from "#lib/format.js";

/** Reads the post that the slug names, for the reader in the root layout. */
export const load: PageServerLoad = async ({ params, parent }) => {
  const { reader } = await parent();
  return { title: slugTitle(params.slug), reader };
};

/** Records that the reader likes the post. */
async function like({ params }: { params: { slug: string } }) {
  return { liked: params.slug };
}

/** The forms of a post page: like it, or comment on it. */
export const actions = {
  like,
  comment: async ({ request }) => {
    const form = await request.formData();
    return { comment: slugTitle(String(form.get("text"))) };
  },
} satisfies Actions;
