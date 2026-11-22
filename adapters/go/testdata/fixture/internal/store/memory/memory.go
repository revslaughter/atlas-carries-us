// Package memory keeps every table in process memory. Tests and local runs
// use it.
package memory

import (
	"context"
	"fmt"
	"sync"

	"example.com/lending/internal/domain"
)

// Store implements store.Store in memory.
type Store struct {
	mu    sync.Mutex
	books map[domain.BookID]domain.Book
	loans []domain.Loan
}

// New returns an empty Store.
func New() *Store { return &Store{books: map[domain.BookID]domain.Book{}} }

// Book returns one book.
func (s *Store) Book(ctx context.Context, id domain.BookID) (domain.Book, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	b, ok := s.books[id]
	if !ok {
		return domain.Book{}, fmt.Errorf("book %s: %w", id, domain.ErrNotFound)
	}
	return b, nil
}

// PutBook stores b.
func (s *Store) PutBook(ctx context.Context, b domain.Book) error {
	s.mu.Lock()
	defer s.mu.Unlock()
	s.books[b.ID] = b
	return nil
}

// Open records a loan.
func (s *Store) Open(ctx context.Context, l domain.Loan) error {
	s.mu.Lock()
	defer s.mu.Unlock()
	s.loans = append(s.loans, l)
	return nil
}

// ForMember lists the loans of one member.
func (s *Store) ForMember(ctx context.Context, member string) (domain.Page[domain.Loan], error) {
	var p domain.Page[domain.Loan]
	for _, l := range s.loans {
		if l.Member == member {
			p.Items = append(p.Items, l)
		}
	}
	return p, nil
}
