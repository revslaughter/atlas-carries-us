package extract

import (
	"encoding/json"
	"fmt"
	"go/ast"
	"go/constant"
	"go/types"
	"strings"

	"golang.org/x/tools/go/packages"

	"github.com/revslaughter/atlas-carries-us/adapters/go/internal/schema"
)

// Entry-point plugins find the ways into the code: HTTP routes and CLI
// commands. Each one switches on from the config file.
//
//   - go-chi: r.Get("/x", h), r.Method("GET", "/x", h), r.Handle("/x", h),
//     with r.Route("/prefix", func(r chi.Router) { … }) nesting.
//   - go-nethttp: mux.HandleFunc("GET /x", h) and http.HandleFunc("/x", h).
//   - cobra: &cobra.Command{Use: "serve", RunE: run}.
//
// A plugin that cannot tell the handler (an inline function) records the
// registering function as the handler and marks the entry point inline.

type prefixRule struct {
	// InFunc names a function or method. Routes registered inside it gain
	// Prefix. Use this for routers that the code mounts under a path.
	InFunc string `json:"inFunc"`
	Prefix string `json:"prefix"`
}

type routeOptions struct {
	Prefixes []prefixRule `json:"prefixes"`
}

type plugins struct {
	chi     *routeOptions
	nethttp *routeOptions
	cobra   bool
}

func newPlugins(cfgs []PluginConfig) (*plugins, error) {
	pl := &plugins{}
	for _, c := range cfgs {
		var o routeOptions
		if len(c.Options) > 0 && string(c.Options) != "null" {
			if err := json.Unmarshal(c.Options, &o); err != nil {
				return nil, fmt.Errorf("options of plugin %s: %w", c.Plugin, err)
			}
		}
		switch c.Plugin {
		case "go-chi":
			pl.chi = &o
		case "go-nethttp":
			pl.nethttp = &o
		case "cobra":
			pl.cobra = true
		default:
			return nil, fmt.Errorf("the Go adapter has no entry-point plugin named %q (known: go-chi, go-nethttp, cobra)", c.Plugin)
		}
	}
	return pl, nil
}

func (o *routeOptions) prefixFor(funcName string) string {
	if o == nil {
		return ""
	}
	for _, r := range o.Prefixes {
		if r.InFunc == funcName {
			return r.Prefix
		}
	}
	return ""
}

var chiVerbs = map[string]string{
	"Get": "GET", "Post": "POST", "Put": "PUT", "Patch": "PATCH", "Delete": "DELETE",
	"Head": "HEAD", "Options": "OPTIONS", "Connect": "CONNECT", "Trace": "TRACE",
}

func (pl *plugins) walk(ex *extractor, p *packages.Package, body ast.Node, caller, funcName string) {
	if pl.chi == nil && pl.nethttp == nil && !pl.cobra {
		return
	}
	var visit func(n ast.Node, prefix string)
	visit = func(n ast.Node, prefix string) {
		ast.Inspect(n, func(x ast.Node) bool {
			switch x := x.(type) {
			case *ast.CallExpr:
				if pl.chi != nil {
					if sub, lit := chiRoute(p, x); lit != nil {
						visit(lit.Body, prefix+sub)
						return false
					}
					if ep := chiRegistration(ex, p, x, prefix, caller); ep != nil {
						ex.entries = append(ex.entries, ep)
					}
				}
				if pl.nethttp != nil {
					if ep := netHTTPRegistration(ex, p, x, pl.nethttp.prefixFor(funcName), caller); ep != nil {
						ex.entries = append(ex.entries, ep)
					}
				}
			case *ast.CompositeLit:
				if pl.cobra {
					if ep := cobraCommand(ex, p, x, caller); ep != nil {
						ex.entries = append(ex.entries, ep)
					}
				}
			}
			return true
		})
	}
	visit(body, pl.chi.prefixFor(funcName))
}

func isChi(p *packages.Package, e ast.Expr) bool {
	tv, ok := p.TypesInfo.Types[e]
	return ok && tv.Type != nil && strings.Contains(tv.Type.String(), "github.com/go-chi/chi")
}

func constString(p *packages.Package, e ast.Expr) (string, bool) {
	tv, ok := p.TypesInfo.Types[e]
	if !ok || tv.Value == nil || tv.Value.Kind() != constant.String {
		return "", false
	}
	return constant.StringVal(tv.Value), true
}

// chiRoute recognises r.Route("/prefix", func(r chi.Router) { … }).
func chiRoute(p *packages.Package, ce *ast.CallExpr) (string, *ast.FuncLit) {
	sel, ok := ast.Unparen(ce.Fun).(*ast.SelectorExpr)
	if !ok || sel.Sel.Name != "Route" || len(ce.Args) != 2 || !isChi(p, sel.X) {
		return "", nil
	}
	pat, ok := constString(p, ce.Args[0])
	if !ok {
		return "", nil
	}
	lit, _ := ce.Args[1].(*ast.FuncLit)
	return pat, lit
}

func chiRegistration(ex *extractor, p *packages.Package, ce *ast.CallExpr, prefix, caller string) *schema.EntryPoint {
	sel, ok := ast.Unparen(ce.Fun).(*ast.SelectorExpr)
	if !ok || !isChi(p, sel.X) {
		return nil
	}
	var method, pattern string
	var handler ast.Expr
	name := sel.Sel.Name
	switch {
	case chiVerbs[name] != "" && len(ce.Args) == 2:
		method = chiVerbs[name]
		pattern, ok = constString(p, ce.Args[0])
		handler = ce.Args[1]
	case (name == "Method" || name == "MethodFunc") && len(ce.Args) == 3:
		m, ok1 := constString(p, ce.Args[0])
		pattern, ok = constString(p, ce.Args[1])
		ok = ok && ok1
		method = strings.ToUpper(m)
		handler = ce.Args[2]
	case (name == "Handle" || name == "HandleFunc") && len(ce.Args) == 2:
		method = "ANY"
		pattern, ok = constString(p, ce.Args[0])
		handler = ce.Args[1]
	default:
		return nil
	}
	if !ok {
		return nil
	}
	return ex.httpEntry(p, ce, method, prefix+pattern, handler, caller, "go-chi")
}

func netHTTPRegistration(ex *extractor, p *packages.Package, ce *ast.CallExpr, prefix, caller string) *schema.EntryPoint {
	sel, ok := ast.Unparen(ce.Fun).(*ast.SelectorExpr)
	if !ok || (sel.Sel.Name != "HandleFunc" && sel.Sel.Name != "Handle") || len(ce.Args) != 2 {
		return nil
	}
	isMux := false
	if id, ok := sel.X.(*ast.Ident); ok {
		if pn, ok := p.TypesInfo.Uses[id].(*types.PkgName); ok && pn.Imported().Path() == "net/http" {
			isMux = true
		}
	}
	if tv, ok := p.TypesInfo.Types[sel.X]; ok && tv.Type != nil && tv.Type.String() == "*net/http.ServeMux" {
		isMux = true
	}
	if !isMux {
		return nil
	}
	pat, ok := constString(p, ce.Args[0])
	if !ok {
		return nil
	}
	method, path := "ANY", pat
	if i := strings.IndexByte(pat, ' '); i > 0 {
		method, path = strings.ToUpper(pat[:i]), strings.TrimSpace(pat[i+1:])
	}
	return ex.httpEntry(p, ce, method, prefix+path, ce.Args[1], caller, "go-nethttp")
}

func (ex *extractor) httpEntry(p *packages.Package, at ast.Node, method, path string, handler ast.Expr, caller, plugin string) *schema.EntryPoint {
	rel, line := ex.rel(p.Fset, at.Pos())
	name := method + " " + path
	ep := &schema.EntryPoint{ID: "http:" + name, Kind: "http", Name: name, Method: method, Path: path,
		Registrar: caller, RegisteredAt: schema.Location{Path: rel, Line: line}, Plugin: plugin}
	ex.setHandler(p, ep, handler, caller)
	return ep
}

// setHandler names the function that serves an entry point. A handler from
// outside the module (such as pprof.Index) keeps the registering function as
// the handler and names the outside function in ExternalHandler.
func (ex *extractor) setHandler(p *packages.Package, ep *schema.EntryPoint, handler ast.Expr, caller string) {
	if _, lit := handler.(*ast.FuncLit); !lit {
		ep.Handler = ex.firstModuleFunc(p, handler)
		if ep.Handler == "" {
			ep.ExternalHandler = shorten(types.ExprString(handler))
		}
	}
	if ep.Handler == "" {
		ep.Handler = caller
		ep.Inline = ep.ExternalHandler == ""
	}
}

func cobraCommand(ex *extractor, p *packages.Package, cl *ast.CompositeLit, caller string) *schema.EntryPoint {
	tv, ok := p.TypesInfo.Types[cl]
	if !ok || tv.Type == nil || tv.Type.String() != "github.com/spf13/cobra.Command" {
		return nil
	}
	var use string
	var run ast.Expr
	for _, el := range cl.Elts {
		kv, ok := el.(*ast.KeyValueExpr)
		if !ok {
			continue
		}
		key, _ := kv.Key.(*ast.Ident)
		if key == nil {
			continue
		}
		switch key.Name {
		case "Use":
			use, _ = constString(p, kv.Value)
		case "RunE":
			run = kv.Value
		case "Run":
			if run == nil {
				run = kv.Value
			}
		}
	}
	fields := strings.Fields(use)
	if run == nil || len(fields) == 0 {
		return nil
	}
	rel, line := ex.rel(p.Fset, cl.Pos())
	ep := &schema.EntryPoint{ID: "cli:" + fields[0], Kind: "cli", Name: fields[0], Registrar: caller,
		RegisteredAt: schema.Location{Path: rel, Line: line}, Plugin: "cobra"}
	ex.setHandler(p, ep, run, caller)
	return ep
}

// firstModuleFunc returns the ID of the first module function or method that
// an expression names, such as h.getPost in http.HandlerFunc(h.getPost).
func (ex *extractor) firstModuleFunc(p *packages.Package, e ast.Expr) string {
	found := ""
	ast.Inspect(e, func(n ast.Node) bool {
		if found != "" {
			return false
		}
		if _, lit := n.(*ast.FuncLit); lit {
			return false
		}
		if id, ok := n.(*ast.Ident); ok {
			if f, ok := p.TypesInfo.Uses[id].(*types.Func); ok {
				if fid := ex.funcID(f); fid != "" && ex.syms[fid] != nil {
					found = fid
				}
			}
		}
		return true
	})
	return found
}
