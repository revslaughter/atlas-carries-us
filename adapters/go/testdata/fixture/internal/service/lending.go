// Package service holds the lending rules. It checks each request and calls
// storage in the right order.
package service

import (
	"context"
	"fmt"
	"time"

	"example.com/lending/internal/domain"
	"example.com/lending/internal/store"
)

// LoanPeriod sets how long a member keeps a book.
const LoanPeriod = 14 * 24 * time.Hour

// Lending applies the lending rules.
type Lending struct {
	Store store.Store
	// Now returns the current time. Tests replace it.
	Now func() time.Time
}

// Borrow lends a book to a member. It fails when the book sits with another
// member, or when the member already holds [domain.MaxLoans] books.
func (l *Lending) Borrow(ctx context.Context, member string, id domain.BookID) (domain.Loan, error) {
	b, err := l.Store.Book(ctx, id)
	if err != nil {
		return domain.Loan{}, err
	}
	if b.Status != domain.OnShelf {
		return domain.Loan{}, fmt.Errorf("%s: %w", domain.Describe(b), domain.ErrOnLoan)
	}
	page, err := l.Store.ForMember(ctx, member)
	if err != nil {
		return domain.Loan{}, err
	}
	if len(page.Items) >= domain.MaxLoans {
		return domain.Loan{}, fmt.Errorf("member %s holds %d books", member, len(page.Items))
	}
	loan := domain.Loan{Book: id, Member: member, Due: l.Now().Add(LoanPeriod)}
	b.Status = domain.Borrowed
	if err := l.Store.PutBook(ctx, b); err != nil {
		return domain.Loan{}, err
	}
	return loan, l.Store.Open(ctx, loan)
}

// Book returns one book.
func (l *Lending) Book(ctx context.Context, id domain.BookID) (domain.Book, error) {
	return l.Store.Book(ctx, id)
}
