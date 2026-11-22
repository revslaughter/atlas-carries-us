package extract

import (
	"go/ast"
	"go/types"
	"strings"

	"golang.org/x/tools/go/packages"

	"github.com/revslaughter/atlas-carries-us/adapters/go/internal/schema"
)

// keyOf maps an object from any package variant (test variants included) to
// a symbol ID. Test variants hold their own copies of each object, so the
// lookup goes by package path and name, not by object identity.
func (ex *extractor) keyOf(obj types.Object) string {
	if obj == nil || obj.Pkg() == nil || !ex.inMod[obj.Pkg().Path()] {
		return ""
	}
	path := obj.Pkg().Path()
	switch o := obj.(type) {
	case *types.TypeName:
		if o.Parent() == o.Pkg().Scope() {
			return symID(path, o.Name())
		}
	case *types.Func:
		return ex.funcID(o)
	case *types.Var:
		if o.IsField() {
			return ""
		}
		if o.Parent() == o.Pkg().Scope() {
			return symID(path, o.Name())
		}
	case *types.Const:
		if o.Parent() == o.Pkg().Scope() {
			return symID(path, o.Name())
		}
	}
	return ""
}

var testPrefixes = []string{"Test", "Example", "Fuzz", "Benchmark"}

// tests records which test functions use each symbol.
func (ex *extractor) tests(tps []*packages.Package) {
	for _, p := range tps {
		for _, f := range p.Syntax {
			fn := p.Fset.Position(f.Pos()).Filename
			if !strings.HasSuffix(fn, "_test.go") {
				continue
			}
			for _, d := range f.Decls {
				fd, ok := d.(*ast.FuncDecl)
				if !ok || fd.Recv != nil || fd.Body == nil {
					continue
				}
				name := fd.Name.Name
				isTest := false
				for _, pre := range testPrefixes {
					if strings.HasPrefix(name, pre) {
						isTest = true
					}
				}
				if !isTest {
					continue
				}
				rel, line := ex.rel(p.Fset, fd.Pos())
				ast.Inspect(fd.Body, func(n ast.Node) bool {
					id, ok := n.(*ast.Ident)
					if !ok {
						return true
					}
					key := ex.keyOf(p.TypesInfo.Uses[id])
					if key == "" || ex.syms[key] == nil {
						return true
					}
					if ex.testUses[key] == nil {
						ex.testUses[key] = map[string]schema.TestUse{}
					}
					ex.testUses[key][rel+":"+name] = schema.TestUse{Name: name, Location: schema.Location{Path: rel, Line: line}}
					return true
				})
			}
		}
	}
}
