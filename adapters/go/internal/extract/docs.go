// Doc comments: Go's own comment parser (go/doc/comment) reads each doc,
// and this file writes the result in the normal form of docs/schema.md,
// section 8. Doc links such as [Mux.Get] become sym: links.

package extract

import (
	"go/doc"
	"go/doc/comment"
	"strings"

	"github.com/revslaughter/atlas-carries-us/adapters/go/internal/schema"
)

// resolveDocs turns every queued doc comment into the normal form. It runs
// last, so that doc links such as [Mux.Get] can resolve to symbol IDs.
func (ex *extractor) resolveDocs() {
	for _, pd := range ex.docs {
		if d := ex.normalise(pd.raw, pd.pkgPath); d != nil {
			pd.set(d)
		}
	}
}

// normalise parses one doc comment from package pkgPath and returns its
// normal form, or nil for an empty comment. A paragraph that starts
// "Deprecated:" fills the Deprecated field instead of the description.
func (ex *extractor) normalise(raw, pkgPath string) *schema.Doc {
	raw = strings.TrimSpace(raw)
	if raw == "" {
		return nil
	}
	p := &comment.Parser{
		LookupPackage: func(name string) (string, bool) {
			ip, ok := ex.pkgByName[name]
			return ip, ok
		},
		LookupSym: func(recv, name string) bool {
			path := name
			if recv != "" {
				path = recv + "." + name
			}
			return ex.syms[symID(pkgPath, path)] != nil
		},
	}
	parsed := p.Parse(raw)
	out := &schema.Doc{Raw: raw, Format: "godoc", Description: []schema.Block{}}
	for _, b := range parsed.Content {
		switch b := b.(type) {
		case *comment.Paragraph:
			text := ex.inline(b.Text, pkgPath)
			if plain := plainText(b.Text); strings.HasPrefix(plain, "Deprecated:") {
				dep := strings.TrimSpace(strings.TrimPrefix(text, "Deprecated:"))
				out.Deprecated = &dep
				continue
			}
			out.Description = append(out.Description, schema.Block{Type: "paragraph", Text: text})
		case *comment.Heading:
			out.Description = append(out.Description, schema.Block{Type: "heading", Level: 3, Text: ex.inline(b.Text, pkgPath)})
		case *comment.Code:
			code := strings.TrimRight(b.Text, "\n")
			if code != "" {
				out.Description = append(out.Description, schema.Block{Type: "code", Code: code})
			}
		case *comment.List:
			ordered := len(b.Items) > 0 && b.Items[0].Number != ""
			blk := schema.Block{Type: "list", Ordered: &ordered, Items: []string{}}
			for _, it := range b.Items {
				var parts []string
				for _, c := range it.Content {
					if para, ok := c.(*comment.Paragraph); ok {
						parts = append(parts, ex.inline(para.Text, pkgPath))
					}
				}
				blk.Items = append(blk.Items, strings.Join(parts, " "))
			}
			out.Description = append(out.Description, blk)
		}
	}
	out.Summary = escapeInline(new(doc.Package).Synopsis(raw))
	return out
}

// inline writes comment text in the inline subset of docs/schema.md.
func (ex *extractor) inline(ts []comment.Text, pkgPath string) string {
	var b strings.Builder
	for _, t := range ts {
		switch t := t.(type) {
		case comment.Plain:
			b.WriteString(escapeInline(reflow(string(t))))
		case comment.Italic:
			b.WriteString("*" + escapeInline(reflow(string(t))) + "*")
		case *comment.Link:
			b.WriteString("[" + ex.inline(t.Text, pkgPath) + "](" + escapeURL(t.URL) + ")")
		case *comment.DocLink:
			b.WriteString("[" + ex.inline(t.Text, pkgPath) + "](" + ex.docLinkTarget(t, pkgPath) + ")")
		}
	}
	return b.String()
}

// docLinkTarget turns a doc link into a link target: "sym:<id>" when the
// map holds the symbol or package, or else its page on pkg.go.dev.
func (ex *extractor) docLinkTarget(l *comment.DocLink, pkgPath string) string {
	ip := l.ImportPath
	if ip == "" {
		ip = pkgPath
	}
	path := l.Name
	if l.Recv != "" {
		path = l.Recv + "." + l.Name
	}
	if path == "" {
		if ex.conts[pkgID(ip)] != nil {
			return "sym:" + pkgID(ip)
		}
		return "https://pkg.go.dev/" + ip
	}
	if id := symID(ip, path); ex.syms[id] != nil {
		return "sym:" + id
	}
	return "https://pkg.go.dev/" + ip + "#" + path
}

// plainText returns the words of comment text without its markup, so a test
// such as "starts with Deprecated:" can read it.
func plainText(ts []comment.Text) string {
	var b strings.Builder
	for _, t := range ts {
		switch t := t.(type) {
		case comment.Plain:
			b.WriteString(string(t))
		case comment.Italic:
			b.WriteString(string(t))
		case *comment.Link:
			b.WriteString(plainText(t.Text))
		case *comment.DocLink:
			b.WriteString(plainText(t.Text))
		}
	}
	return b.String()
}

// reflow joins the lines of wrapped comment text with spaces.
func reflow(s string) string { return strings.ReplaceAll(s, "\n", " ") }

// inlineEscaper puts a backslash before each character that the inline
// subset gives a meaning to.
var inlineEscaper = strings.NewReplacer(`\`, `\\`, "`", "\\`", `*`, `\*`, `[`, `\[`, `]`, `\]`)

// escapeInline escapes the characters that the inline subset gives meaning to.
func escapeInline(s string) string { return inlineEscaper.Replace(s) }

// escapeURL encodes the characters that would end a link target early in the
// inline subset: parentheses and spaces.
func escapeURL(u string) string {
	return strings.NewReplacer("(", "%28", ")", "%29", " ", "%20").Replace(u)
}
