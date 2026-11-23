// The book: its ID, where it sits, and its genre.

import type { Loan } from "./loan.js";

/** A book's ID, as the Go service writes it: "b-" and a number. */
export type BookID = `b-${number}`;

/** Where a book sits. The values match the Go service's status strings. */
export enum Shelf {
  /** On the shelf, ready to lend. */
  OnShelf = "on-shelf",
  Borrowed = "borrowed",
  Lost = "lost",
}

/** Genres, as small numbers for the wire. */
export const enum Genre {
  Fiction,
  Science,
  History = 10,
}

/**
 * Book is one title in the catalogue.
 *
 * @see {@link Loan} for a book that a member has borrowed.
 */
export interface Book {
  /** The book's ID. */
  id: BookID;
  title: string;
  readonly shelf: Shelf;
  genre?: Genre;
}

/** The fields that a new book needs. The service picks the ID and the shelf. */
export type NewBook = Omit<Book, "id" | "shelf">;
