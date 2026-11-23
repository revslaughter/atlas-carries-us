// Package extract reads the Go packages of a module with the type checker and
// builds an Atlas Carries Us document (schema version 1) from them.
//
// [Run] loads the packages with golang.org/x/tools/go/packages, which runs
// go list and the type checker, so every fact comes from the compiler and
// every edge counts as exact. The extraction then runs in three passes:
//
//  1. Declarations: packages, files, types, fields, interface methods,
//     functions, constants and variables.
//  2. Relations: methods with bodies, field and signature types, implements,
//     calls, entry points and test usage.
//  3. Docs: every doc comment turns into the normal form once all symbols
//     exist, so doc links can resolve to symbol IDs.
//
// IDs come from import paths ("go:" + package path + "#" + name), so a link
// to a symbol keeps working after a rebuild. [docs/adapters.md] records the
// decisions and the limits.
//
// [docs/adapters.md]: https://github.com/revslaughter/atlas-carries-us/blob/main/docs/adapters.md
package extract

import (
	"encoding/json"
	"fmt"
	"go/ast"
	"go/token"
	"go/types"
	"os"
	"path/filepath"
	"sort"
	"strings"

	"golang.org/x/tools/go/packages"

	"github.com/revslaughter/atlas-carries-us/adapters/go/internal/schema"
)

// AdapterVersion names this adapter's release.
const AdapterVersion = "0.1.0"

// Request holds what the acu command asks the adapter to read. It arrives as
// JSON on standard input (protocol 1, see docs/adapters.md).
type Request struct {
	// Protocol holds the protocol version; this adapter speaks 1.
	Protocol int `json:"protocol"`
	// RepoRoot holds the repository root; every path in the output is relative to it.
	RepoRoot string `json:"repoRoot"`
	// Root holds the folder to read: the module's root.
	Root string `json:"root"`
	// Include lists globs of package folders to keep; an empty list keeps all.
	Include []string `json:"include"`
	// Exclude lists globs of package folders to leave out.
	Exclude []string `json:"exclude"`
	// Options holds the settings that only this adapter reads.
	Options Options `json:"options"`
	// EntryPoints lists the plugins to switch on.
	EntryPoints []PluginConfig `json:"entryPoints"`
}

// Options holds the settings that only the Go adapter reads.
type Options struct {
	// BuildTags go to the build as -tags.
	BuildTags []string `json:"buildTags"`
	// Patterns name the packages to load. The default is ./...
	Patterns []string `json:"patterns"`
}

// PluginConfig switches on one entry-point plugin.
type PluginConfig struct {
	// Plugin names the plugin: "go-chi", "go-nethttp" or "cobra".
	Plugin string `json:"plugin"`
	// Options holds the plugin's options as JSON; the plugin parses them.
	Options json.RawMessage `json:"options"`
}

// edgeKey identifies one edge, so a repeated use raises a count instead of
// adding a second edge.
type edgeKey struct{ from, to, kind string }

// pendingDoc is a doc comment that waits for pass 3, when every symbol exists
// and its doc links can resolve. set stores the result on its owner.
type pendingDoc struct {
	set     func(*schema.Doc)
	raw     string
	pkgPath string
}

// extractor holds the state of one run: the loaded packages, the containers
// and symbols found so far, and the indexes that the later passes need.
type extractor struct {
	// req holds the request as it arrived.
	req Request
	// repoRoot holds the absolute repository root.
	repoRoot string
	// module holds the main module's path, such as "github.com/go-chi/chi/v5".
	module string
	// in lists the packages that the map holds, sorted by path.
	in []*packages.Package
	// inMod reports whether a package path belongs to the map.
	inMod map[string]bool
	// conts maps a container ID to its container.
	conts map[string]*schema.Container
	// syms maps a symbol ID to its symbol.
	syms map[string]*schema.Symbol
	// byObj maps a type checker object to the ID of its symbol.
	byObj map[types.Object]string
	// named lists the named types, for the implements check.
	named []*types.TypeName
	// ifaces lists the interfaces that declare methods.
	ifaces []*types.TypeName
	// edges maps an edge's key to the edge, so repeats raise its count.
	edges map[edgeKey]*schema.Edge
	// docs lists the doc comments that wait for pass 3.
	docs []pendingDoc
	// pkgByName maps a package name to its path, so doc links such as
	// [store.Store] resolve.
	pkgByName map[string]string
	// entries lists the entry points that the plugins found.
	entries []*schema.EntryPoint
	// testUses maps a symbol ID to its tests, keyed by file and name.
	testUses map[string]map[string]schema.TestUse
	// plugins holds the plugins that the request switches on.
	plugins *plugins
	// notes lists extra limits of this run for the output's notes.
	notes []string
}

// Run loads the packages that req names and returns the document.
func Run(req Request) (*schema.Document, error) {
	root, err := filepath.Abs(req.Root)
	if err != nil {
		return nil, err
	}
	repoRoot := req.RepoRoot
	if repoRoot == "" {
		repoRoot = root
	}
	repoRoot, err = filepath.Abs(repoRoot)
	if err != nil {
		return nil, err
	}
	pl, err := newPlugins(req.EntryPoints)
	if err != nil {
		return nil, err
	}
	cfg := &packages.Config{
		Mode: packages.NeedName | packages.NeedFiles | packages.NeedImports | packages.NeedTypes |
			packages.NeedSyntax | packages.NeedTypesInfo | packages.NeedModule,
		Dir:   root,
		Tests: true,
	}
	// The adapter reads code; it never needs version-control stamps. Without
	// this flag, go list asks git for the repository's state and fails when
	// another user owns the checkout (git's "dubious ownership" check).
	cfg.BuildFlags = []string{"-buildvcs=false"}
	if len(req.Options.BuildTags) > 0 {
		cfg.BuildFlags = append(cfg.BuildFlags, "-tags", strings.Join(req.Options.BuildTags, ","))
	}
	patterns := req.Options.Patterns
	if len(patterns) == 0 {
		patterns = []string{"./..."}
	}
	pkgs, err := packages.Load(cfg, patterns...)
	if err != nil {
		return nil, err
	}
	ex := &extractor{
		req: req, repoRoot: repoRoot, inMod: map[string]bool{},
		conts: map[string]*schema.Container{}, syms: map[string]*schema.Symbol{},
		byObj: map[types.Object]string{}, edges: map[edgeKey]*schema.Edge{},
		pkgByName: map[string]string{}, testUses: map[string]map[string]schema.TestUse{},
		plugins: pl,
	}
	include, err := compileGlobs(req.Include)
	if err != nil {
		return nil, err
	}
	exclude, err := compileGlobs(req.Exclude)
	if err != nil {
		return nil, err
	}
	var keep, testPkgs []*packages.Package
	loadErrors := 0
	for _, p := range pkgs {
		if p.Module != nil && ex.module == "" && p.Module.Main {
			ex.module = p.Module.Path
		}
		if len(p.Syntax) == 0 || p.Types == nil {
			continue
		}
		dir := ex.relDir(p)
		if exclude.match(dir) || (len(include) > 0 && !include.match(dir)) {
			continue
		}
		if len(p.Errors) > 0 {
			loadErrors++
			for _, e := range p.Errors {
				fmt.Fprintf(os.Stderr, "acu-adapter-go: %s: %v\n", p.ID, e)
			}
		}
		isMain := p.ID == p.PkgPath && !strings.HasSuffix(p.PkgPath, ".test") && !strings.HasSuffix(p.PkgPath, "_test")
		switch {
		case isMain:
			keep = append(keep, p)
		case !strings.HasSuffix(p.PkgPath, ".test"):
			testPkgs = append(testPkgs, p)
		}
	}
	if len(keep) == 0 {
		return nil, fmt.Errorf("no Go packages found under %s", root)
	}
	if loadErrors > 0 {
		ex.notes = append(ex.notes, fmt.Sprintf("%d package variants had type errors; their facts may be incomplete.", loadErrors))
	}
	sort.Slice(keep, func(i, j int) bool { return keep[i].PkgPath < keep[j].PkgPath })
	ex.in = keep
	for _, p := range keep {
		ex.inMod[p.PkgPath] = true
		if _, dup := ex.pkgByName[p.Name]; !dup {
			ex.pkgByName[p.Name] = p.PkgPath
		}
	}
	ex.declarations()
	ex.relations()
	ex.tests(testPkgs)
	ex.resolveDocs()
	return ex.document(), nil
}

// relDir returns a package's folder relative to the repository root.
func (ex *extractor) relDir(p *packages.Package) string {
	files := p.GoFiles
	if len(files) == 0 {
		files = p.CompiledGoFiles
	}
	if len(files) == 0 {
		return ""
	}
	r, err := filepath.Rel(ex.repoRoot, filepath.Dir(files[0]))
	if err != nil {
		return ""
	}
	return filepath.ToSlash(r)
}

/* IDs. See docs/schema.md, section 3. */

// symID returns a symbol's ID: its package path, "#", and its path inside
// the package, such as "Mux.Get".
func symID(pkgPath, path string) string { return "go:" + pkgPath + "#" + path }

// pkgID returns the ID of a package's container.
func pkgID(pkgPath string) string { return "go:pkg:" + pkgPath }

// fileID returns the ID of a file's container, from its path relative to the
// repository root.
func fileID(rel string) string { return "go:file:" + rel }

// modID returns the ID of a module's container.
func modID(mod string) string { return "go:mod:" + mod }

// rel returns the file of a position, relative to the repository root, and
// its line.
func (ex *extractor) rel(fset *token.FileSet, pos token.Pos) (string, int) {
	p := fset.Position(pos)
	r, err := filepath.Rel(ex.repoRoot, p.Filename)
	if err != nil {
		r = p.Filename
	}
	return filepath.ToSlash(r), p.Line
}

// unitName returns a package's display name: its path inside the module,
// such as "internal/store", or its package name at the module's root.
func (ex *extractor) unitName(p *packages.Package) string {
	if ex.module == "" || !strings.HasPrefix(p.PkgPath, ex.module) {
		return p.PkgPath
	}
	r := strings.TrimPrefix(strings.TrimPrefix(p.PkgPath, ex.module), "/")
	if r == "" {
		return p.Name
	}
	return r
}

// qual writes type names as a reader of package self writes them: bare for
// its own types, with the package name for others.
func qual(self *types.Package) types.Qualifier {
	return func(p *types.Package) string {
		if p == self {
			return ""
		}
		return p.Name()
	}
}

// addSym adds a symbol. A second symbol with the same ID gains a suffix,
// "@file:line", as docs/schema.md section 3 asks.
func (ex *extractor) addSym(s *schema.Symbol) *schema.Symbol {
	if old, dup := ex.syms[s.ID]; dup {
		// Two declarations share a qualified name (rare: build-tag twins).
		// Keep the first and give the second a position suffix.
		s.ID = fmt.Sprintf("%s@%s:%d", s.ID, filepath.Base(s.Location.Path), s.Location.Line)
		_ = old
	}
	ex.syms[s.ID] = s
	return s
}

// addCont adds a container once, and returns the one that the map holds.
func (ex *extractor) addCont(c *schema.Container) *schema.Container {
	if old, ok := ex.conts[c.ID]; ok {
		return old
	}
	ex.conts[c.ID] = c
	return c
}

// queueDoc keeps a doc comment for pass 3; an empty comment needs no work.
func (ex *extractor) queueDoc(raw, pkgPath string, set func(*schema.Doc)) {
	if strings.TrimSpace(raw) == "" {
		return
	}
	ex.docs = append(ex.docs, pendingDoc{set: set, raw: raw, pkgPath: pkgPath})
}

// addEdge records one use. A repeated use raises the edge's count and keeps
// its first line, so a trace can sort calls in source order.
func (ex *extractor) addEdge(from, to, kind string, line int) {
	// A call from a function to itself (recursion) counts; other self-edges do not.
	if from == "" || to == "" || (from == to && kind != "calls") {
		return
	}
	k := edgeKey{from, to, kind}
	if e, ok := ex.edges[k]; ok {
		e.Count++
		if line > 0 && (e.Line == 0 || line < e.Line) {
			e.Line = line
		}
		return
	}
	ex.edges[k] = &schema.Edge{From: from, To: to, Kind: kind, Confidence: "exact", Line: line, Count: 1}
}

// refs returns the symbol IDs of the module's named types that t mentions.
func (ex *extractor) refs(t types.Type) []string {
	seen := map[string]bool{}
	var walk func(types.Type, int)
	named := func(obj *types.TypeName) {
		if obj.Pkg() != nil && ex.inMod[obj.Pkg().Path()] && obj.Parent() == obj.Pkg().Scope() {
			seen[symID(obj.Pkg().Path(), obj.Name())] = true
		}
	}
	walk = func(t types.Type, depth int) {
		if t == nil || depth > 8 {
			return
		}
		switch x := t.(type) {
		case *types.Named:
			named(x.Origin().Obj())
			if ta := x.TypeArgs(); ta != nil {
				for i := 0; i < ta.Len(); i++ {
					walk(ta.At(i), depth+1)
				}
			}
		case *types.Alias:
			named(x.Obj())
			walk(types.Unalias(x), depth+1)
		case *types.Pointer:
			walk(x.Elem(), depth+1)
		case *types.Slice:
			walk(x.Elem(), depth+1)
		case *types.Array:
			walk(x.Elem(), depth+1)
		case *types.Map:
			walk(x.Key(), depth+1)
			walk(x.Elem(), depth+1)
		case *types.Chan:
			walk(x.Elem(), depth+1)
		case *types.Signature:
			for i := 0; i < x.Params().Len(); i++ {
				walk(x.Params().At(i).Type(), depth+1)
			}
			for i := 0; i < x.Results().Len(); i++ {
				walk(x.Results().At(i).Type(), depth+1)
			}
		}
	}
	walk(t, 0)
	r := make([]string, 0, len(seen))
	for s := range seen {
		r = append(r, s)
	}
	sort.Strings(r)
	return r
}

// params describes a parameter or result list. The last parameter of a
// variadic function reads "...T" and counts as rest.
func (ex *extractor) params(tup *types.Tuple, q types.Qualifier, variadic bool) []schema.Param {
	ps := []schema.Param{}
	for i := 0; i < tup.Len(); i++ {
		v := tup.At(i)
		ts := types.TypeString(v.Type(), q)
		rest := false
		if variadic && i == tup.Len()-1 {
			if s, ok := v.Type().(*types.Slice); ok {
				ts = "..." + types.TypeString(s.Elem(), q)
				rest = true
			}
		}
		ps = append(ps, schema.Param{Name: v.Name(), Type: ts, Refs: ex.refs(v.Type()), Rest: rest})
	}
	return ps
}

// typeParamString writes type parameters as Go does, such as "[K comparable, V any]".
func typeParamString(tps *types.TypeParamList, q types.Qualifier) string {
	if tps == nil || tps.Len() == 0 {
		return ""
	}
	var parts []string
	for i := 0; i < tps.Len(); i++ {
		tp := tps.At(i)
		parts = append(parts, tp.Obj().Name()+" "+types.TypeString(tp.Constraint(), q))
	}
	return "[" + strings.Join(parts, ", ") + "]"
}

// sigText writes a whole signature as Go does, such as
// "func (m *Mux) Get(pattern string, h http.HandlerFunc)".
func sigText(name, recv string, sig *types.Signature, q types.Qualifier) string {
	var b strings.Builder
	b.WriteString("func ")
	if recv != "" {
		b.WriteString("(" + recv + ") ")
	}
	b.WriteString(name)
	b.WriteString(typeParamString(sig.TypeParams(), q))
	s := types.TypeString(sig, q) // "func(a int) (b, error)"
	b.WriteString(strings.TrimPrefix(s, "func"))
	return b.String()
}

// signature describes a function or method for the data format.
func (ex *extractor) signature(name, recv string, sig *types.Signature, q types.Qualifier) *schema.Signature {
	return &schema.Signature{
		TypeParams: typeParamString(sig.TypeParams(), q),
		Params:     ex.params(sig.Params(), q, sig.Variadic()),
		Results:    ex.params(sig.Results(), q, false),
		Text:       sigText(name, recv, sig, q),
	}
}

// docText returns the text of the first comment group that holds any, so a
// caller can list a doc comment first and a line comment after it.
func docText(groups ...*ast.CommentGroup) string {
	for _, g := range groups {
		if g != nil {
			if t := strings.TrimSpace(g.Text()); t != "" {
				return t
			}
		}
	}
	return ""
}

// recvNamed returns the named type of a method's receiver, through a
// pointer or an alias, or nil for a function.
func recvNamed(sig *types.Signature) *types.TypeName {
	if sig.Recv() == nil {
		return nil
	}
	t := sig.Recv().Type()
	if p, ok := t.(*types.Pointer); ok {
		t = p.Elem()
	}
	switch n := t.(type) {
	case *types.Named:
		return n.Origin().Obj()
	case *types.Alias:
		if nn, ok := types.Unalias(n).(*types.Named); ok {
			return nn.Origin().Obj()
		}
	}
	return nil
}

// funcID returns the symbol ID of a function or method object, or "".
func (ex *extractor) funcID(f *types.Func) string {
	if f == nil || f.Pkg() == nil || !ex.inMod[f.Pkg().Path()] {
		return ""
	}
	f = f.Origin()
	sig := f.Type().(*types.Signature)
	if tn := recvNamed(sig); tn != nil {
		if tn.Pkg() == nil {
			return ""
		}
		return symID(tn.Pkg().Path(), tn.Name()+"."+f.Name())
	}
	if sig.Recv() != nil {
		return ""
	}
	if id, ok := ex.byObj[f]; ok {
		return id
	}
	return symID(f.Pkg().Path(), f.Name())
}

/* Pass 1: declarations. */

// declarations runs pass 1: the module, each package and its doc, each file
// and its comment, and the declarations in each file.
func (ex *extractor) declarations() {
	if ex.module != "" {
		modDir := ""
		for _, p := range ex.in {
			if p.Module != nil && p.Module.Path == ex.module {
				if r, err := filepath.Rel(ex.repoRoot, p.Module.Dir); err == nil && r != "." {
					modDir = filepath.ToSlash(r)
				}
				break
			}
		}
		ex.addCont(&schema.Container{ID: modID(ex.module), Kind: "module", LanguageKind: "module",
			Name: ex.module, Path: modDir, Language: "go"})
	}
	for _, p := range ex.in {
		q := qual(p.Types)
		pc := ex.addCont(&schema.Container{ID: pkgID(p.PkgPath), Kind: "package", LanguageKind: "package",
			Name: ex.unitName(p), Path: ex.relDir(p), Language: "go", Unit: true})
		if ex.module != "" {
			pc.Parent = modID(ex.module)
		}
		var ext []string
		for ip := range p.Imports {
			if ex.inMod[ip] {
				ex.addEdge(pc.ID, pkgID(ip), "imports", 0)
			} else if strings.Contains(strings.Split(ip, "/")[0], ".") {
				ext = append(ext, ip)
			}
		}
		sort.Strings(ext)
		pc.ExternalImports = ext
		// The package doc: a file comment that starts "Package <name>" (or
		// "Command" for a program). Without one, the comment attached to the
		// package clause counts, as go doc treats it, with doc.go first.
		attached := map[*ast.File]string{}
		pkgDoc, pkgDocFile := "", (*ast.File)(nil)
		for _, f := range p.Syntax {
			if f.Doc == nil {
				continue
			}
			t := strings.TrimSpace(f.Doc.Text())
			if isLicence(t) {
				continue
			}
			attached[f] = t
			if pkgDoc == "" && isPackageDoc(t, p.Name) {
				pkgDoc, pkgDocFile = t, f
			}
		}
		if pkgDoc == "" {
			for _, f := range p.Syntax {
				t, ok := attached[f]
				if !ok {
					continue
				}
				if pkgDocFile == nil || filepath.Base(p.Fset.Position(f.Pos()).Filename) == "doc.go" {
					pkgDoc, pkgDocFile = t, f
				}
			}
		}
		for _, f := range p.Syntax {
			rel, _ := ex.rel(p.Fset, f.Pos())
			fd := ""
			if f != pkgDocFile {
				fd = attached[f]
			}
			// A comment above the package clause, apart from it, still
			// describes the file, unless it holds a licence or a build line.
			if fd == "" && f != pkgDocFile && len(f.Comments) > 0 && f.Comments[0].End() < f.Package && f.Comments[0] != f.Doc {
				fd = strings.TrimSpace(f.Comments[0].Text())
				if strings.HasPrefix(fd, "go:build") || strings.HasPrefix(fd, "+build") || strings.HasPrefix(fd, "Code generated") || isLicence(fd) {
					fd = ""
				}
			}
			fc := ex.addCont(&schema.Container{ID: fileID(rel), Kind: "file", LanguageKind: "file",
				Name: filepath.Base(rel), Path: rel, Parent: pc.ID, Language: "go"})
			ex.queueDoc(fd, p.PkgPath, func(d *schema.Doc) { fc.Doc = d })
			ex.collectFile(p, f, rel, q)
		}
		ex.queueDoc(pkgDoc, p.PkgPath, func(d *schema.Doc) { pc.Doc = d })
	}
}

// isLicence reports whether a comment holds a licence header, which
// documents no code.
func isLicence(text string) bool {
	return strings.HasPrefix(text, "Copyright") || strings.HasPrefix(text, "SPDX-License-Identifier") ||
		strings.Contains(text, "Licensed under") || strings.Contains(text, "license that can be found in the LICENSE file")
}

// isPackageDoc reports whether a file comment documents the package. Go
// writes "Package name …" for a library and "Command name …" for a program.
func isPackageDoc(text, name string) bool {
	return strings.HasPrefix(text, "Package "+name) || (name == "main" && strings.HasPrefix(text, "Command "))
}

// collectFile declares a file's types, constants, variables and functions.
// Methods wait for pass 2, once every type they attach to exists.
func (ex *extractor) collectFile(p *packages.Package, f *ast.File, rel string, q types.Qualifier) {
	fset := p.Fset
	parent := fileID(rel)
	base := filepath.Base(rel)
	for _, d := range f.Decls {
		switch d := d.(type) {
		case *ast.GenDecl:
			switch d.Tok {
			case token.TYPE:
				for _, sp := range d.Specs {
					ex.collectType(p, d, sp.(*ast.TypeSpec), rel, parent, q)
				}
			case token.CONST, token.VAR:
				_, declLine := ex.rel(fset, d.Pos())
				group := fmt.Sprintf("%s:%d", base, declLine)
				for _, sp := range d.Specs {
					vs := sp.(*ast.ValueSpec)
					for i, n := range vs.Names {
						if n.Name == "_" {
							continue
						}
						obj := p.TypesInfo.Defs[n]
						if obj == nil {
							continue
						}
						_, line := ex.rel(fset, n.Pos())
						_, end := ex.rel(fset, vs.End())
						kind, lk := "variable", "variable"
						if d.Tok == token.CONST {
							kind, lk = "constant", "constant"
						}
						s := ex.addSym(&schema.Symbol{ID: symID(p.PkgPath, n.Name), Kind: kind, LanguageKind: lk,
							Name: n.Name, Parent: parent, Exported: obj.Exported(),
							Location: schema.Location{Path: rel, Line: line, EndLine: end}, Group: group,
							Type: &schema.TypeText{Text: types.TypeString(obj.Type(), q), Refs: ex.refs(obj.Type())}})
						if c, ok := obj.(*types.Const); ok {
							s.Value = shorten(c.Val().ExactString())
						} else if i < len(vs.Values) {
							s.Value = shorten(types.ExprString(vs.Values[i]))
						}
						raw := docText(vs.Doc, vs.Comment)
						if raw == "" && len(d.Specs) == 1 {
							raw = docText(d.Doc)
						}
						ex.queueDoc(raw, p.PkgPath, func(dd *schema.Doc) { s.Doc = dd })
						ex.byObj[obj] = s.ID
					}
				}
			}
		case *ast.FuncDecl:
			obj, _ := p.TypesInfo.Defs[d.Name].(*types.Func)
			if obj == nil {
				continue
			}
			sig := obj.Type().(*types.Signature)
			if sig.Recv() != nil {
				continue // methods attach in pass 2, once every type exists
			}
			_, line := ex.rel(fset, d.Pos())
			_, end := ex.rel(fset, d.End())
			id := symID(p.PkgPath, obj.Name())
			if obj.Name() == "init" || obj.Name() == "_" {
				id = symID(p.PkgPath, fmt.Sprintf("%s@%s:%d", obj.Name(), base, line))
			}
			s := ex.addSym(&schema.Symbol{ID: id, Kind: "function", LanguageKind: "function", Name: obj.Name(),
				Parent: parent, Exported: obj.Exported(), Location: schema.Location{Path: rel, Line: line, EndLine: end},
				Signature: ex.signature(obj.Name(), "", sig, q)})
			ex.queueDoc(docText(d.Doc), p.PkgPath, func(dd *schema.Doc) { s.Doc = dd })
			ex.byObj[obj] = s.ID
		}
	}
}

// shorten cuts a value to 80 characters, so a long literal does not crowd
// the map.
func shorten(s string) string {
	if len(s) > 80 {
		return s[:77] + "..."
	}
	return s
}

// collectType declares a named type: a struct with its fields, an interface
// with its methods, an alias, or a type over another type.
func (ex *extractor) collectType(p *packages.Package, d *ast.GenDecl, ts *ast.TypeSpec, rel, parent string, q types.Qualifier) {
	fset := p.Fset
	obj, _ := p.TypesInfo.Defs[ts.Name].(*types.TypeName)
	if obj == nil {
		return
	}
	_, line := ex.rel(fset, ts.Pos())
	_, end := ex.rel(fset, ts.End())
	s := &schema.Symbol{ID: symID(p.PkgPath, obj.Name()), Name: obj.Name(), Parent: parent,
		Exported: obj.Exported(), Location: schema.Location{Path: rel, Line: line, EndLine: end}}
	raw := docText(ts.Doc, d.Doc, ts.Comment)
	if len(d.Specs) > 1 && ts.Doc == nil {
		raw = docText(ts.Comment)
	}
	if tp := ts.TypeParams; tp != nil && len(tp.List) > 0 {
		var parts []string
		for _, fl := range tp.List {
			var names []string
			for _, n := range fl.Names {
				names = append(names, n.Name)
			}
			parts = append(parts, strings.Join(names, ", ")+" "+types.ExprString(fl.Type))
		}
		s.TypeParams = "[" + strings.Join(parts, ", ") + "]"
	}
	if obj.IsAlias() {
		s.Kind, s.LanguageKind = "type_alias", "type alias"
		at := types.Unalias(obj.Type())
		s.Type = &schema.TypeText{Text: types.TypeString(at, q), Refs: ex.refs(at)}
	} else {
		switch u := obj.Type().Underlying().(type) {
		case *types.Struct:
			s.Kind, s.LanguageKind = "struct", "struct"
			ex.collectFields(p, ts, s, u, q)
		case *types.Interface:
			s.Kind, s.LanguageKind = "interface", "interface"
			ex.collectInterface(p, ts, s, u, q)
			if u.NumMethods() > 0 {
				ex.ifaces = append(ex.ifaces, obj)
			}
		default:
			s.Kind, s.LanguageKind = "type", "type"
			s.Type = &schema.TypeText{Text: types.TypeString(u, q), Refs: ex.refs(u)}
		}
	}
	if !obj.IsAlias() {
		ex.named = append(ex.named, obj)
	}
	ex.byObj[obj] = s.ID
	ex.addSym(s)
	ex.queueDoc(raw, p.PkgPath, func(dd *schema.Doc) { s.Doc = dd })
}

// collectFields declares a struct's fields, with their tags and docs. An
// embedded field gains the modifier "embedded".
func (ex *extractor) collectFields(p *packages.Package, ts *ast.TypeSpec, s *schema.Symbol, u *types.Struct, q types.Qualifier) {
	st, _ := ts.Type.(*ast.StructType)
	if st == nil {
		return
	}
	fi := 0
	for _, fl := range st.Fields.List {
		raw := docText(fl.Doc, fl.Comment)
		tag := ""
		if fl.Tag != nil {
			tag = fl.Tag.Value
		}
		n := len(fl.Names)
		if n == 0 {
			n = 1
		}
		for k := 0; k < n && fi < u.NumFields(); k++ {
			v := u.Field(fi)
			fi++
			if v.Name() == "_" {
				continue
			}
			rel, line := ex.rel(p.Fset, v.Pos())
			f := &schema.Symbol{ID: symID(p.PkgPath, s.Name+"."+v.Name()), Kind: "field", LanguageKind: "field",
				Name: v.Name(), Parent: s.ID, Exported: v.Exported(), Location: schema.Location{Path: rel, Line: line},
				Type: &schema.TypeText{Text: types.TypeString(v.Type(), q), Refs: ex.refs(v.Type())}}
			if v.Embedded() {
				f.LanguageKind = "embedded field"
				f.Modifiers = []string{"embedded"}
			}
			if tag != "" {
				f.Annotations = []string{tag}
			}
			ex.addSym(f)
			ex.queueDoc(raw, p.PkgPath, func(dd *schema.Doc) { f.Doc = dd })
		}
	}
}

// collectInterface declares an interface's methods, and each interface it
// embeds as a member that pass 2 turns into an extends edge.
func (ex *extractor) collectInterface(p *packages.Package, ts *ast.TypeSpec, s *schema.Symbol, u *types.Interface, q types.Qualifier) {
	it, _ := ts.Type.(*ast.InterfaceType)
	docs := map[string]string{}
	if it != nil {
		for _, m := range it.Methods.List {
			dd := docText(m.Doc, m.Comment)
			for _, n := range m.Names {
				docs[n.Name] = dd
			}
			if len(m.Names) == 0 {
				// An embedded interface or a type constraint.
				tv, ok := p.TypesInfo.Types[m.Type]
				if !ok {
					continue
				}
				text := types.TypeString(tv.Type, q)
				rel, line := ex.rel(p.Fset, m.Pos())
				e := ex.addSym(&schema.Symbol{ID: symID(p.PkgPath, s.Name+".embeds:"+text), Kind: "field",
					LanguageKind: "embedded interface", Name: text, Parent: s.ID, Exported: true,
					Location: schema.Location{Path: rel, Line: line}, Modifiers: []string{"embedded"},
					Type: &schema.TypeText{Text: text, Refs: ex.refs(tv.Type)}})
				ex.queueDoc(dd, p.PkgPath, func(x *schema.Doc) { e.Doc = x })
			}
		}
	}
	for i := 0; i < u.NumExplicitMethods(); i++ {
		m := u.ExplicitMethod(i)
		sig := m.Type().(*types.Signature)
		rel, ml := ex.rel(p.Fset, m.Pos())
		ms := ex.addSym(&schema.Symbol{ID: symID(p.PkgPath, s.Name+"."+m.Name()), Kind: "method", LanguageKind: "method",
			Name: m.Name(), Parent: s.ID, Exported: m.Exported(), Location: schema.Location{Path: rel, Line: ml},
			Signature: ex.signature(m.Name(), "", sig, q)})
		ex.queueDoc(docs[m.Name()], p.PkgPath, func(x *schema.Doc) { ms.Doc = x })
	}
}

/* Pass 2: relations. */

// relations runs pass 2: it attaches methods to their types, then records
// type edges, implementations and calls.
func (ex *extractor) relations() {
	// Methods with bodies attach to their type symbols.
	for _, p := range ex.in {
		q := qual(p.Types)
		for _, f := range p.Syntax {
			for _, d := range f.Decls {
				fd, ok := d.(*ast.FuncDecl)
				if !ok || fd.Recv == nil {
					continue
				}
				obj, _ := p.TypesInfo.Defs[fd.Name].(*types.Func)
				if obj == nil {
					continue
				}
				sig := obj.Type().(*types.Signature)
				tn := recvNamed(sig)
				if tn == nil {
					continue
				}
				owner := ex.syms[ex.byObj[tn]]
				if owner == nil {
					continue
				}
				rel, line := ex.rel(p.Fset, fd.Pos())
				_, end := ex.rel(p.Fset, fd.End())
				recv := types.TypeString(sig.Recv().Type(), q)
				if n := sig.Recv().Name(); n != "" && n != "_" {
					recv = n + " " + recv
				}
				m := &schema.Symbol{ID: symID(p.PkgPath, owner.Name+"."+obj.Name()), Kind: "method", LanguageKind: "method",
					Name: obj.Name(), Parent: owner.ID, Exported: obj.Exported(),
					Location:  schema.Location{Path: rel, Line: line, EndLine: end},
					Signature: ex.signature(obj.Name(), recv, sig, q)}
				if _, ptr := sig.Recv().Type().(*types.Pointer); ptr {
					m.Modifiers = []string{"pointer receiver"}
				}
				ex.addSym(m)
				ex.byObj[obj] = m.ID
				ex.queueDoc(docText(fd.Doc), p.PkgPath, func(x *schema.Doc) { m.Doc = x })
			}
		}
	}
	// Field, extends, parameter and result edges.
	for _, s := range ex.sortedSyms() {
		switch {
		case s.Kind == "field" && s.LanguageKind == "embedded interface":
			if s.Type != nil {
				for _, r := range s.Type.Refs {
					ex.addEdge(s.Parent, r, "extends", s.Location.Line)
				}
			}
		case s.Kind == "field":
			if s.Type != nil {
				for _, r := range s.Type.Refs {
					ex.addEdge(s.ID, r, "field_type", s.Location.Line)
				}
			}
		case s.Signature != nil:
			for _, pr := range s.Signature.Params {
				for _, r := range pr.Refs {
					ex.addEdge(s.ID, r, "param_type", s.Location.Line)
				}
			}
			for _, pr := range s.Signature.Results {
				for _, r := range pr.Refs {
					ex.addEdge(s.ID, r, "return_type", s.Location.Line)
				}
			}
		}
	}
	ex.implementations()
	ex.calls()
}

// sortedSyms returns the symbols in ID order, so every pass that walks them
// gives the same result on every run.
func (ex *extractor) sortedSyms() []*schema.Symbol {
	list := make([]*schema.Symbol, 0, len(ex.syms))
	for _, s := range ex.syms {
		list = append(list, s)
	}
	sort.Slice(list, func(i, j int) bool { return list[i].ID < list[j].ID })
	return list
}

// implementations records which module types implement which module
// interfaces, by value or by pointer, and which well-known outside
// interfaces each type satisfies.
func (ex *extractor) implementations() {
	std := stdInterfaces(ex.in)
	stdNames := make([]string, 0, len(std))
	for n := range std {
		stdNames = append(stdNames, n)
	}
	sort.Strings(stdNames)
	for _, t := range ex.named {
		if types.IsInterface(t.Type()) {
			continue
		}
		nt, ok := t.Type().(*types.Named)
		if !ok || nt.TypeParams() != nil {
			continue
		}
		tid := ex.byObj[t]
		ptr := types.NewPointer(t.Type())
		for _, i := range ex.ifaces {
			it, ok := i.Type().Underlying().(*types.Interface)
			if !ok || it.NumMethods() == 0 {
				continue
			}
			if in, ok := i.Type().(*types.Named); ok && in.TypeParams() != nil {
				continue
			}
			if types.Implements(t.Type(), it) || types.Implements(ptr, it) {
				ex.addEdge(tid, ex.byObj[i], "implements", 0)
			}
		}
		s := ex.syms[tid]
		for _, name := range stdNames {
			it := std[name]
			if types.Implements(t.Type(), it) || types.Implements(ptr, it) {
				s.Satisfies = append(s.Satisfies, name)
			}
		}
	}
}

// calls walks every function body, and every package-level variable whose
// value holds function literals, and records each call to a module function
// or method. Calls through an interface point at the interface method.
func (ex *extractor) calls() {
	for _, p := range ex.in {
		for _, f := range p.Syntax {
			for _, d := range f.Decls {
				switch d := d.(type) {
				case *ast.FuncDecl:
					if d.Body == nil {
						continue
					}
					obj, _ := p.TypesInfo.Defs[d.Name].(*types.Func)
					caller := ex.byObj[obj]
					if caller == "" {
						continue
					}
					ex.walkCalls(p, d.Body, caller)
					ex.plugins.walk(ex, p, d.Body, caller, d.Name.Name)
				case *ast.GenDecl:
					if d.Tok != token.VAR {
						continue
					}
					for _, sp := range d.Specs {
						vs := sp.(*ast.ValueSpec)
						for i, n := range vs.Names {
							if i >= len(vs.Values) {
								break
							}
							caller := ex.byObj[p.TypesInfo.Defs[n]]
							if caller == "" {
								continue
							}
							ex.walkCalls(p, vs.Values[i], caller)
							ex.plugins.walk(ex, p, vs.Values[i], caller, n.Name)
						}
					}
				}
			}
		}
	}
}

// calleeIdent returns the name that a call expression calls: f in f(),
// Get in r.Get(), Map in Map[K, V](), or nil.
func calleeIdent(fun ast.Expr) *ast.Ident {
	switch fn := ast.Unparen(fun).(type) {
	case *ast.Ident:
		return fn
	case *ast.SelectorExpr:
		return fn.Sel
	case *ast.IndexExpr:
		return calleeIdent(fn.X)
	case *ast.IndexListExpr:
		return calleeIdent(fn.X)
	}
	return nil
}

// walkCalls records each call in body to a module function or method as an
// edge from caller. Calls inside closures count for the enclosing function.
func (ex *extractor) walkCalls(p *packages.Package, body ast.Node, caller string) {
	ast.Inspect(body, func(n ast.Node) bool {
		ce, ok := n.(*ast.CallExpr)
		if !ok {
			return true
		}
		id := calleeIdent(ce.Fun)
		if id == nil {
			return true
		}
		cf, ok := p.TypesInfo.Uses[id].(*types.Func)
		if !ok {
			return true
		}
		callee := ex.funcID(cf)
		if callee == "" || ex.syms[callee] == nil {
			return true
		}
		_, line := ex.rel(p.Fset, ce.Pos())
		ex.addEdge(caller, callee, "calls", line)
		return true
	})
}

// stdInterfaces finds a few well-known standard interfaces in the import graph.
func stdInterfaces(in []*packages.Package) map[string]*types.Interface {
	want := map[string][]string{
		"net/http":      {"Handler"},
		"fmt":           {"Stringer"},
		"io":            {"Reader", "Writer", "Closer"},
		"encoding/json": {"Marshaler", "Unmarshaler"},
		"sort":          {"Interface"},
	}
	res := map[string]*types.Interface{}
	res["error"] = types.Universe.Lookup("error").Type().Underlying().(*types.Interface)
	seen := map[string]bool{}
	var visit func(*types.Package)
	visit = func(tp *types.Package) {
		if tp == nil || seen[tp.Path()] {
			return
		}
		seen[tp.Path()] = true
		if names, ok := want[tp.Path()]; ok {
			for _, n := range names {
				if o := tp.Scope().Lookup(n); o != nil {
					if it, ok := o.Type().Underlying().(*types.Interface); ok {
						res[tp.Name()+"."+n] = it
					}
				}
			}
		}
		for _, i := range tp.Imports() {
			visit(i)
		}
	}
	for _, p := range in {
		visit(p.Types)
	}
	return res
}

/* Output. */

// document assembles the output: it sorts every list, drops edges and refs
// to things outside the map, and gives a repeated entry point ID its suffix.
func (ex *extractor) document() *schema.Document {
	d := &schema.Document{
		Version: schema.Version,
		Meta: schema.Meta{
			SchemaVersion: schema.Version,
			Tool:          schema.Tool{Name: "acu-adapter-go", Version: AdapterVersion},
			Languages:     []string{"go"},
			Adapters: []schema.AdapterInfo{{Name: "go", Version: AdapterVersion, Language: "go",
				Notes: append([]string{
					"Calls through function values, closures passed as arguments, and reflection do not appear.",
					"A call through an interface points at the interface method.",
				}, ex.notes...)}},
		},
		Containers: []*schema.Container{},
		Symbols:    []*schema.Symbol{},
		Edges:      []*schema.Edge{},
	}
	if r, err := filepath.Rel(ex.repoRoot, ex.absRoot()); err == nil && r != "." {
		d.Meta.Adapters[0].Root = filepath.ToSlash(r)
	}
	for _, c := range ex.conts {
		d.Containers = append(d.Containers, c)
	}
	sort.Slice(d.Containers, func(i, j int) bool { return d.Containers[i].ID < d.Containers[j].ID })
	known := func(id string) bool { return ex.syms[id] != nil || ex.conts[id] != nil }
	for _, s := range ex.sortedSyms() {
		filterRefs := func(refs []string) []string {
			var out []string
			for _, r := range refs {
				if ex.syms[r] != nil {
					out = append(out, r)
				}
			}
			return out
		}
		if s.Type != nil {
			s.Type.Refs = filterRefs(s.Type.Refs)
		}
		if s.Signature != nil {
			for i := range s.Signature.Params {
				s.Signature.Params[i].Refs = filterRefs(s.Signature.Params[i].Refs)
			}
			for i := range s.Signature.Results {
				s.Signature.Results[i].Refs = filterRefs(s.Signature.Results[i].Refs)
			}
		}
		d.Symbols = append(d.Symbols, s)
	}
	for _, e := range ex.edges {
		if known(e.From) && known(e.To) {
			if e.Count == 1 {
				e.Count = 0
			}
			d.Edges = append(d.Edges, e)
		}
	}
	sort.Slice(d.Edges, func(i, j int) bool {
		a, b := d.Edges[i], d.Edges[j]
		if a.From != b.From {
			return a.From < b.From
		}
		if a.To != b.To {
			return a.To < b.To
		}
		return a.Kind < b.Kind
	})
	seenEP := map[string]bool{}
	for _, e := range ex.entries {
		if seenEP[e.ID] {
			e.ID = fmt.Sprintf("%s@%s:%d", e.ID, filepath.Base(e.RegisteredAt.Path), e.RegisteredAt.Line)
		}
		seenEP[e.ID] = true
		d.EntryPoints = append(d.EntryPoints, e)
	}
	sort.SliceStable(d.EntryPoints, func(i, j int) bool {
		a, b := d.EntryPoints[i].RegisteredAt, d.EntryPoints[j].RegisteredAt
		if a.Path != b.Path {
			return a.Path < b.Path
		}
		return a.Line < b.Line
	})
	if len(ex.testUses) > 0 {
		d.Tests = map[string][]schema.TestUse{}
		for k, m := range ex.testUses {
			l := make([]schema.TestUse, 0, len(m))
			for _, t := range m {
				l = append(l, t)
			}
			sort.Slice(l, func(i, j int) bool {
				if l[i].Location.Path != l[j].Location.Path {
					return l[i].Location.Path < l[j].Location.Path
				}
				return l[i].Location.Line < l[j].Location.Line
			})
			d.Tests[k] = l
		}
	}
	return d
}

// absRoot returns the folder that the request names, as an absolute path.
func (ex *extractor) absRoot() string {
	r, _ := filepath.Abs(ex.req.Root)
	return r
}
