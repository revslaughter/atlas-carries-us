// Package store defines the storage contracts that the services use. Two
// stores implement them: [memory.Store] and [filestore.Store].
//
// Every store follows these rules:
//
//  1. ctx comes first.
//  2. Values go in and out, never shared pointers.
//  3. Errors wrap the sentinels in package domain, such as [domain.ErrNotFound].
//
// A typical read:
//
//	b, err := s.Book(ctx, id)
//	if errors.Is(err, domain.ErrNotFound) { … }
package store

import (
	"context"

	"example.com/lending/internal/domain"
)

// Books reads and writes books.
type Books interface {
	// Book returns one book, or an error that wraps domain.ErrNotFound.
	Book(ctx context.Context, id domain.BookID) (domain.Book, error)
	// PutBook stores b as given.
	PutBook(ctx context.Context, b domain.Book) error
}

// Loans reads and writes loans.
type Loans interface {
	// Open records a new loan.
	Open(ctx context.Context, l domain.Loan) error
	// ForMember lists the open loans of one member.
	ForMember(ctx context.Context, member string) (domain.Page[domain.Loan], error)
}

// Store holds every contract. See [Books] and [Loans].
type Store interface {
	Books
	Loans
}
