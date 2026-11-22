// Command acu-adapter-go reads a Go module with the type checker and writes
// an Atlas Carries Us document (schema version 1) as JSON.
//
// The acu command runs it with a request on standard input (protocol 1, see
// docs/adapters.md). For a run by hand, pass flags instead:
//
//	acu-adapter-go -root /path/to/module -tags grafeo -exclude '**/features/**' -plugin go-chi -out atlas.json
package main

import (
	"encoding/json"
	"flag"
	"fmt"
	"io"
	"os"
	"strings"
	"time"

	"github.com/revslaughter/atlas-carries-us/adapters/go/internal/extract"
)

type listFlag []string

func (l *listFlag) String() string     { return strings.Join(*l, ",") }
func (l *listFlag) Set(s string) error { *l = append(*l, s); return nil }

func main() {
	var (
		root     = flag.String("root", "", "folder to read; without it, the request comes as JSON on standard input")
		repoRoot = flag.String("repo-root", "", "repository root that file paths are relative to (default: -root)")
		tags     = flag.String("tags", "", "comma-separated build tags")
		out      = flag.String("out", "", "output file (default: standard output)")
		indent   = flag.Bool("indent", false, "indent the JSON")
		version  = flag.Bool("version", false, "print the version and exit")
		include  listFlag
		exclude  listFlag
		plugin   listFlag
	)
	flag.Var(&include, "include", "glob of package folders to keep (repeatable)")
	flag.Var(&exclude, "exclude", "glob of package folders to leave out (repeatable)")
	flag.Var(&plugin, "plugin", "entry-point plugin to switch on: go-chi, go-nethttp, cobra (repeatable)")
	flag.Parse()
	if *version {
		fmt.Println("acu-adapter-go", extract.AdapterVersion)
		return
	}
	var req extract.Request
	if *root == "" {
		b, err := io.ReadAll(os.Stdin)
		if err != nil {
			fail(err)
		}
		if err := json.Unmarshal(b, &req); err != nil {
			fail(fmt.Errorf("read the request from standard input: %w", err))
		}
		if req.Protocol != 1 {
			fail(fmt.Errorf("request protocol %d: this adapter speaks protocol 1", req.Protocol))
		}
	} else {
		req = extract.Request{Protocol: 1, Root: *root, RepoRoot: *repoRoot, Include: include, Exclude: exclude}
		if *tags != "" {
			req.Options.BuildTags = strings.Split(*tags, ",")
		}
		for _, p := range plugin {
			req.EntryPoints = append(req.EntryPoints, extract.PluginConfig{Plugin: p})
		}
	}
	doc, err := extract.Run(req)
	if err != nil {
		fail(err)
	}
	doc.Meta.Generated = time.Now().UTC().Format(time.RFC3339)
	w := os.Stdout
	if *out != "" {
		f, err := os.Create(*out)
		if err != nil {
			fail(err)
		}
		defer f.Close()
		w = f
	}
	enc := json.NewEncoder(w)
	enc.SetEscapeHTML(false)
	if *indent {
		enc.SetIndent("", " ")
	}
	if err := enc.Encode(doc); err != nil {
		fail(err)
	}
	fmt.Fprintf(os.Stderr, "acu-adapter-go: %d containers, %d symbols, %d edges, %d entry points\n",
		len(doc.Containers), len(doc.Symbols), len(doc.Edges), len(doc.EntryPoints))
}

func fail(err error) {
	fmt.Fprintln(os.Stderr, "acu-adapter-go:", err)
	os.Exit(1)
}
