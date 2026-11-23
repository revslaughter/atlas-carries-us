// Feeds of the blog's posts, in more than one format.

/** Makes a request handler that answers with the post list in one format: "json" or "text". */
export function feedHandler(format: "json" | "text") {
  return async () => {
    const posts = ["first-light", "second-wind"];
    return format === "json" ? Response.json(posts) : new Response(posts.join("\n"));
  };
}
