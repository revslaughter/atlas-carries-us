// The blog's feed. A factory builds the handler, so the export holds what it
// returns rather than a function of its own.

import { feedHandler } from "#lib/feed.js";

/** Answers with the post list as JSON. */
export const GET = feedHandler("json");
