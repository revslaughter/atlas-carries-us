package domain

// Status says where a book sits.
type Status int

// The states of a book.
const (
	OnShelf  Status = iota // on the shelf, free to borrow
	Borrowed               // with a member
	Lost                   // missing after an overdue loan
)

// String returns the status as a word.
func (s Status) String() string {
	switch s {
	case OnShelf:
		return "on shelf"
	case Borrowed:
		return "borrowed"
	}
	return "lost"
}
