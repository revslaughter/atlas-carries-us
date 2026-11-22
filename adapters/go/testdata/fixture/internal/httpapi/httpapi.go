// Package httpapi turns HTTP requests into service calls. Handlers decode,
// call one service method, and encode.
package httpapi

import (
	"context"
	"encoding/json"
	"net/http"

	"github.com/go-chi/chi/v5"

	"example.com/lending/internal/domain"
)

// Lender holds what the handlers need from the service layer.
type Lender interface {
	Borrow(ctx context.Context, member string, id domain.BookID) (domain.Loan, error)
	Book(ctx context.Context, id domain.BookID) (domain.Book, error)
}

// Handler serves the lending API.
type Handler struct {
	Lending Lender
}

// Routes registers every route of the API. The router mounts them under /api.
func (h *Handler) Routes(r chi.Router) {
	r.Get("/books/{id}", h.getBook)
	r.Post("/loans", h.borrow)
	r.Route("/members", func(r chi.Router) {
		r.Get("/{id}/loans", h.memberLoans)
		r.Delete("/{id}", func(w http.ResponseWriter, r *http.Request) { w.WriteHeader(http.StatusNotImplemented) })
	})
	r.Method(http.MethodGet, "/status", http.HandlerFunc(h.status))
}

// NewMux builds the top-level mux: /health at the root, the API below /api.
func NewMux(h *Handler) *http.ServeMux {
	mux := http.NewServeMux()
	mux.HandleFunc("GET /health", health)
	r := chi.NewRouter()
	h.Routes(r)
	mux.Handle("/api/", http.StripPrefix("/api", r))
	return mux
}

func (h *Handler) getBook(w http.ResponseWriter, r *http.Request) {
	b, err := h.Lending.Book(r.Context(), domain.BookID(r.PathValue("id")))
	if err != nil {
		http.Error(w, err.Error(), http.StatusNotFound)
		return
	}
	writeJSON(w, b)
}

func (h *Handler) borrow(w http.ResponseWriter, r *http.Request) {
	var in struct {
		Member string
		Book   domain.BookID
	}
	_ = json.NewDecoder(r.Body).Decode(&in)
	loan, err := h.Lending.Borrow(r.Context(), in.Member, in.Book)
	if err != nil {
		http.Error(w, err.Error(), http.StatusConflict)
		return
	}
	writeJSON(w, loan)
}

func (h *Handler) memberLoans(w http.ResponseWriter, r *http.Request) { writeJSON(w, []domain.Loan{}) }

func (h *Handler) status(w http.ResponseWriter, r *http.Request) { writeJSON(w, "ok") }

func health(w http.ResponseWriter, r *http.Request) { w.WriteHeader(http.StatusOK) }

func writeJSON(w http.ResponseWriter, v any) {
	w.Header().Set("Content-Type", "application/json")
	_ = json.NewEncoder(w).Encode(v)
}
