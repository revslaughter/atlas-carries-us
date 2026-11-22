// Package filestore keeps the tables in JSON files in one folder.
package filestore

import (
	"context"
	"encoding/json"
	"os"
	"path/filepath"

	"example.com/lending/internal/domain"
)

// Store implements store.Store on files.
type Store struct {
	// Dir holds one file per table.
	Dir string
}

// Book reads one book from books.json.
func (s Store) Book(ctx context.Context, id domain.BookID) (domain.Book, error) {
	all, err := s.read()
	if err != nil {
		return domain.Book{}, err
	}
	b, ok := all[id]
	if !ok {
		return domain.Book{}, domain.ErrNotFound
	}
	return b, nil
}

// PutBook writes one book to books.json.
func (s Store) PutBook(ctx context.Context, b domain.Book) error {
	all, err := s.read()
	if err != nil {
		return err
	}
	all[b.ID] = b
	return s.write(all)
}

// Open is not supported by the file store yet.
func (s Store) Open(ctx context.Context, l domain.Loan) error { return nil }

// ForMember returns no loans.
func (s Store) ForMember(ctx context.Context, member string) (domain.Page[domain.Loan], error) {
	return domain.Page[domain.Loan]{}, nil
}

func (s Store) read() (map[domain.BookID]domain.Book, error) {
	b, err := os.ReadFile(filepath.Join(s.Dir, "books.json"))
	if err != nil {
		return map[domain.BookID]domain.Book{}, nil
	}
	m := map[domain.BookID]domain.Book{}
	return m, json.Unmarshal(b, &m)
}

func (s Store) write(m map[domain.BookID]domain.Book) error {
	b, _ := json.Marshal(m)
	return os.WriteFile(filepath.Join(s.Dir, "books.json"), b, 0o644)
}
