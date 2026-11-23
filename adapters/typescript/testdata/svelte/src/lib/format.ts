// Text helpers for the reading room's pages.

/** Writes a count with its noun, in the plural when the count is not one. */
export function plural(count: number, noun: string): string {
  return `${count} ${count === 1 ? noun : noun + "s"}`;
}

/** Writes a text in capitals, with an exclamation mark. */
export function shout(text: string): string {
  return text.toUpperCase() + "!";
}

/** Turns a post's slug, such as "first-light", into its title, "First light". */
export function slugTitle(slug: string): string {
  const words = slug.split("-");
  return [words[0][0].toUpperCase() + words[0].slice(1), ...words.slice(1)].join(" ");
}
