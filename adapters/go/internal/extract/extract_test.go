package extract

import (
	"bytes"
	"encoding/json"
	"flag"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/santhosh-tekuri/jsonschema/v6"

	"github.com/revslaughter/atlas-carries-us/adapters/go/internal/schema"
)

var update = flag.Bool("update", false, "rewrite the golden files")

const (
	fixture    = "../../testdata/fixture"
	golden     = "../../testdata/golden/fixture.json"
	schemaFile = "../../../../schema/atlas.schema.json"
)

func fixtureRequest() Request {
	return Request{
		Protocol: 1,
		Root:     fixture,
		Exclude:  []string{"**/features/**"},
		EntryPoints: []PluginConfig{
			{Plugin: "go-chi", Options: json.RawMessage(`{"prefixes":[{"inFunc":"Routes","prefix":"/api"}]}`)},
			{Plugin: "go-nethttp"},
			{Plugin: "cobra"},
		},
	}
}

func runFixture(t *testing.T) (*schema.Document, []byte) {
	t.Helper()
	doc, err := Run(fixtureRequest())
	if err != nil {
		t.Fatal(err)
	}
	doc.Meta.Generated = "2000-01-01T00:00:00Z"
	var buf bytes.Buffer
	enc := json.NewEncoder(&buf)
	enc.SetEscapeHTML(false)
	enc.SetIndent("", " ")
	if err := enc.Encode(doc); err != nil {
		t.Fatal(err)
	}
	return doc, buf.Bytes()
}

// TestGolden compares the fixture's output with the golden file. Run
// "go test ./... -update" after a deliberate change, and read the diff.
func TestGolden(t *testing.T) {
	_, got := runFixture(t)
	if *update {
		if err := os.WriteFile(golden, got, 0o644); err != nil {
			t.Fatal(err)
		}
		return
	}
	want, err := os.ReadFile(golden)
	if err != nil {
		t.Fatalf("%v (run go test ./... -update to create it)", err)
	}
	if !bytes.Equal(got, want) {
		gl, wl := strings.Split(string(got), "\n"), strings.Split(string(want), "\n")
		for i := 0; i < len(gl) && i < len(wl); i++ {
			if gl[i] != wl[i] {
				t.Fatalf("output differs from %s at line %d:\n got: %s\nwant: %s\n(run go test ./... -update after a deliberate change)", golden, i+1, gl[i], wl[i])
			}
		}
		t.Fatalf("output differs from %s in length: %d lines, want %d", golden, len(gl), len(wl))
	}
}

// TestSchema validates the fixture's output against schema/atlas.schema.json.
func TestSchema(t *testing.T) {
	_, got := runFixture(t)
	c := jsonschema.NewCompiler()
	sch, err := c.Compile(filepath.ToSlash(schemaFile))
	if err != nil {
		t.Fatal(err)
	}
	inst, err := jsonschema.UnmarshalJSON(bytes.NewReader(got))
	if err != nil {
		t.Fatal(err)
	}
	if err := sch.Validate(inst); err != nil {
		t.Fatalf("output breaks the schema:\n%v", err)
	}
}

// TestFacts checks the facts that the viewer depends on, in words a reader
// can follow when one breaks.
func TestFacts(t *testing.T) {
	doc, _ := runFixture(t)
	const m = "go:example.com/lending/"
	syms := map[string]*schema.Symbol{}
	for _, s := range doc.Symbols {
		syms[s.ID] = s
	}
	has := func(from, to, kind string) bool {
		for _, e := range doc.Edges {
			if e.From == from && e.To == to && e.Kind == kind {
				return true
			}
		}
		return false
	}
	for _, c := range []struct{ from, to, kind string }{
		{m + "internal/store/memory#Store", m + "internal/store#Store", "implements"},
		{m + "internal/store/filestore#Store", m + "internal/store#Store", "implements"},
		{m + "internal/store#Store", m + "internal/store#Books", "extends"},
		{m + "internal/service#Lending.Borrow", m + "internal/store#Books.Book", "calls"},
		{m + "internal/service#Lending.Store", m + "internal/store#Store", "field_type"},
		{m + "internal/service#Lending.Borrow", m + "internal/domain#BookID", "param_type"},
		{m + "internal/service#Lending.Borrow", m + "internal/domain#Loan", "return_type"},
		{"go:pkg:example.com/lending/internal/service", "go:pkg:example.com/lending/internal/store", "imports"},
	} {
		if !has(c.from, c.to, c.kind) {
			t.Errorf("missing edge %s -%s-> %s", c.from, c.kind, c.to)
		}
	}
	if has("go:pkg:example.com/lending/internal/features", "", "") || syms[m+"internal/features#Step"] != nil {
		t.Error("the excluded features package appears in the output")
	}
	eps := map[string]*schema.EntryPoint{}
	for _, e := range doc.EntryPoints {
		eps[e.ID] = e
	}
	for id, handler := range map[string]string{
		"http:GET /api/books/{id}":         m + "internal/httpapi#Handler.getBook",
		"http:GET /api/members/{id}/loans": m + "internal/httpapi#Handler.memberLoans",
		"http:GET /api/status":             m + "internal/httpapi#Handler.status",
		"http:GET /health":                 m + "internal/httpapi#health",
		"cli:serve":                        m + "cmd/lendd#serve",
	} {
		if eps[id] == nil {
			t.Errorf("missing entry point %s", id)
		} else if eps[id].Handler != handler {
			t.Errorf("entry point %s: handler %s, want %s", id, eps[id].Handler, handler)
		}
	}
	if e := eps["http:DELETE /api/members/{id}"]; e == nil || !e.Inline {
		t.Error("the inline DELETE handler should appear, marked inline")
	}
	if b := syms[m+"internal/service#Lending.Borrow"]; b == nil || b.Doc == nil || !strings.Contains(b.Doc.Description[0].Text, "(sym:"+m+"internal/domain#MaxLoans)") {
		t.Error("the doc link [domain.MaxLoans] in Lending.Borrow should resolve to its symbol")
	}
	if d := syms[m+"internal/domain#Timestamp"]; d == nil || d.Doc == nil || d.Doc.Deprecated == nil {
		t.Error("Timestamp should carry its Deprecated note")
	}
	if uses := doc.Tests[m+"internal/service#Lending.Borrow"]; len(uses) != 2 {
		t.Errorf("Lending.Borrow: %d test uses, want 2 (TestBorrow, ExampleLending_Borrow)", len(uses))
	}
}

func TestGlobs(t *testing.T) {
	g, err := compileGlobs([]string{"**/features/**", "cmd/*"})
	if err != nil {
		t.Fatal(err)
	}
	for dir, want := range map[string]bool{
		"internal/features":     true,
		"features":              true,
		"internal/features/sub": true,
		"cmd/api":               true,
		"cmd":                   false,
		"internal/feature":      false,
	} {
		if got := g.match(dir); got != want {
			t.Errorf("match(%q) = %v, want %v", dir, got, want)
		}
	}
}
