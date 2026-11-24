// Formatting for the screens, as an ES module.

/**
 * Formats a book's title for a card.
 *
 * @param {import('../domain/book.js').Book} book
 * @returns {string}
 */
export function formatTitle(book) {
  return book.title.trim();
}
