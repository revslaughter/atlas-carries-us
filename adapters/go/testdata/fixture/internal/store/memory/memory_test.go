package memory

import (
	"context"
	"errors"
	"testing"

	"example.com/lending/internal/domain"
)

func TestMissingBook(t *testing.T) {
	s := New()
	if _, err := s.Book(context.Background(), "x"); !errors.Is(err, domain.ErrNotFound) {
		t.Fatal(err)
	}
}
