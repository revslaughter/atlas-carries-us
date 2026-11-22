package service_test

import (
	"context"
	"testing"
	"time"

	"example.com/lending/internal/domain"
	"example.com/lending/internal/service"
	"example.com/lending/internal/store/memory"
)

func TestBorrow(t *testing.T) {
	s := memory.New()
	_ = s.PutBook(context.Background(), domain.Book{ID: "b1", Title: "Emma"})
	l := &service.Lending{Store: s, Now: time.Now}
	if _, err := l.Borrow(context.Background(), "m1", "b1"); err != nil {
		t.Fatal(err)
	}
}

func ExampleLending_Borrow() {
	l := &service.Lending{Store: memory.New(), Now: time.Now}
	_, _ = l.Borrow(context.Background(), "m1", "b1")
}
