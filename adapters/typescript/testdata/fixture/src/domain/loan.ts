// Loans and members, and the rule for how many books a member may hold.

import type { Book, BookID } from "./book.js";

/** A member of the library. */
export interface Member {
  id: string;
  name: string;
}

/**
 * A loan: one member holds one book until the due date.
 *
 * @typeParam T - the kind of note that the loan carries
 */
export interface Loan<T = string> {
  book: Book;
  member: Member;
  due: Date;
  note?: T;
}

/** The most books that one member may hold at once. It matches MaxLoans in the Go service. */
export const MAX_LOANS = 5,
  LOAN_DAYS = 21;

/**
 * Reports whether a member may borrow one more book.
 *
 * Read the Go service's rule in
 * [lending.go](https://github.com/revslaughter/atlas-carries-us/blob/main/adapters/go/testdata/fixture/internal/service/lending.go).
 *
 * @param loans - the member's open loans
 * @returns `true` when the member holds fewer than {@link MAX_LOANS} books
 * @deprecated Ask the service instead: {@link Member} rules may change.
 * @since 0.2.0
 */
export function mayBorrow(loans: readonly Loan[]): boolean {
  return loans.length < MAX_LOANS;
}

/** Turns a book ID into the path that the Go API expects. */
export const bookPath = (id: BookID): string => `/api/books/${id}`;
