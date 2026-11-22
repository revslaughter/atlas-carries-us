// Command lendd runs the lending library server.
package main

import (
	"log"
	"net/http"
	"os"

	"github.com/spf13/cobra"

	"example.com/lending/internal/httpapi"
	"example.com/lending/internal/service"
	"example.com/lending/internal/store"
	"example.com/lending/internal/store/filestore"
	"example.com/lending/internal/store/memory"
)

var rootCmd = &cobra.Command{Use: "lendd", Short: "Run the lending library"}

var serveCmd = &cobra.Command{
	Use:   "serve [flags]",
	Short: "Serve the HTTP API",
	RunE:  serve,
}

func init() {
	rootCmd.AddCommand(serveCmd)
	rootCmd.AddCommand(&cobra.Command{Use: "version", Run: func(cmd *cobra.Command, args []string) { log.Print("lendd 1.0") }})
}

func main() {
	if err := rootCmd.Execute(); err != nil {
		os.Exit(1)
	}
}

// openStore picks the store from the LEND_STORE variable.
func openStore() store.Store {
	if dir := os.Getenv("LEND_STORE"); dir != "" {
		return filestore.Store{Dir: dir}
	}
	return memory.New()
}

func serve(cmd *cobra.Command, args []string) error {
	l := &service.Lending{Store: openStore()}
	h := &httpapi.Handler{Lending: l}
	return http.ListenAndServe(":8080", httpapi.NewMux(h))
}
