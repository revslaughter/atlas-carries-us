// Package domain holds the shared vocabulary of the lending library: books,
// members, loans, and the errors that every layer returns.
//
// Domain types hold data and small pure rules. They import nothing else in
// the module, so every other package can use them.
package domain

import (
	"errors"
	"fmt"
	"time"
)

// Errors that every layer wraps. Test for them with [errors.Is].
var (
	// ErrNotFound reports a missing book, member or loan.
	ErrNotFound = errors.New("not found")
	// ErrOnLoan reports a book that another member holds.
	ErrOnLoan = errors.New("book on loan")
)

// MaxLoans caps the books one member may hold at once.
const MaxLoans = 5

// BookID identifies a book.
type BookID string

// Book is one copy of a title on the shelves.
type Book struct {
	// ID identifies the copy.
	ID     BookID `json:"id"`
	Title  string `json:"title"`
	Author string `json:"author"`
	Status Status `json:"status"`
}

// Member is a person who may borrow books.
type Member struct {
	ID   string
	Name string
}

// Loan records that a member holds a book until a due date.
type Loan struct {
	Book   BookID
	Member string
	Due    time.Time
}

// Overdue reports whether the loan has passed its due date at now.
func (l Loan) Overdue(now time.Time) bool { return now.After(l.Due) }

// Page holds one page of results from a list call.
type Page[T any] struct {
	Items []T
	Next  string
}

// Timestamp is an alias kept for older callers.
//
// Deprecated: use [time.Time] directly.
type Timestamp = time.Time

// Describe returns a one-line description of b.
func Describe(b Book) string { return fmt.Sprintf("%s by %s", b.Title, b.Author) }
