// The book card: one book as the readers' pages show it.

import { type Book, Shelf } from "../domain/book.js";

/** The props of the book card. */
export interface BookCardProps {
  book: Book;
  compact?: boolean;
}

/** Draws one book: its title, and a badge when someone has borrowed it. */
export function BookCard({ book, compact = false }: BookCardProps) {
  return (
    <article>
      <h2>{book.title}</h2>
      {!compact && book.shelf !== Shelf.OnShelf ? <Badge text="Borrowed" /> : null}
    </article>
  );
}

/** A small label beside a title. */
export const Badge = (props: { text: string }) => <span>{props.text}</span>;

/** Draws a list of books. */
export function BookList(props: { books: Book[] }) {
  return (
    <ul>
      {props.books.map(b => (
        <BookCard key={b.id} book={b} compact />
      ))}
    </ul>
  );
}
