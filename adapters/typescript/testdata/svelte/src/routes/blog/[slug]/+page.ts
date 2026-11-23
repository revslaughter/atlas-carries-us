// The data of a post page in the browser.

import type { PageLoad } from "./$types";

/** Joins the section of the blog layout to the post's slug. */
export const load: PageLoad = async ({ params, parent }) => {
  const { section } = await parent();
  return { section, slug: params.slug };
};
