module example.com/lending

go 1.22

require (
	github.com/go-chi/chi/v5 v5.0.0
	github.com/spf13/cobra v1.0.0
)

replace github.com/go-chi/chi/v5 => ./third_party/chi

replace github.com/spf13/cobra => ./third_party/cobra
