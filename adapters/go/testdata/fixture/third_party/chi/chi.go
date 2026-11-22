// Package chi stands in for github.com/go-chi/chi in the adapter's test
// fixture. It holds only the method set that the fixture calls.
package chi

import "net/http"

// Router registers routes.
type Router interface {
	http.Handler
	Get(pattern string, h http.HandlerFunc)
	Post(pattern string, h http.HandlerFunc)
	Delete(pattern string, h http.HandlerFunc)
	Method(method, pattern string, h http.Handler)
	Route(pattern string, fn func(r Router)) Router
	Group(fn func(r Router)) Router
}

// Mux implements Router.
type Mux struct{}

// NewRouter returns a new Mux.
func NewRouter() *Mux { return &Mux{} }

func (m *Mux) ServeHTTP(w http.ResponseWriter, r *http.Request) {}
func (m *Mux) Get(pattern string, h http.HandlerFunc)           {}
func (m *Mux) Post(pattern string, h http.HandlerFunc)          {}
func (m *Mux) Delete(pattern string, h http.HandlerFunc)        {}
func (m *Mux) Method(method, pattern string, h http.Handler)    {}
func (m *Mux) Route(pattern string, fn func(r Router)) Router   { fn(m); return m }
func (m *Mux) Group(fn func(r Router)) Router                   { fn(m); return m }
