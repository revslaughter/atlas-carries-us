// The cards that show books, as JSX components.

import { formatTitle } from "./format.mjs";

/**
 * Shows one book.
 *
 * @param {{ book: import('../domain/book.js').Book }} props
 */
export function BookCard({ book }) {
  return <div className="card">{formatTitle(book)}</div>;
}

/**
 * Shows a list of books.
 *
 * @param {{ books: import('../domain/book.js').Book[] }} props
 */
export function BookList({ books }) {
  return (
    <ul>
      {books.map(b => (
        <BookCard book={b} />
      ))}
    </ul>
  );
}
