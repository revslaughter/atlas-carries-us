// Path globs for the include and exclude lists of a request. "**" matches
// any number of folders, "*" matches within one folder, and "?" matches one
// character. The TypeScript adapter follows the same rules.

package extract

import (
	"fmt"
	"regexp"
	"strings"
)

// glob is one compiled pattern.
type glob struct {
	re *regexp.Regexp
	// contents is true for a pattern that ends in "**". Such a pattern also
	// matches the folder whose contents it names.
	contents bool
}

// globs is a list of compiled patterns; a path matches the list when it
// matches any one of them.
type globs []glob

// compileGlobs turns path globs into regular expressions. "**" matches any
// number of folders, "*" matches within one folder, "?" matches one character.
func compileGlobs(patterns []string) (globs, error) {
	var out globs
	for _, p := range patterns {
		re, err := globRegexp(p)
		if err != nil {
			return nil, fmt.Errorf("bad glob %q: %w", p, err)
		}
		out = append(out, glob{re: re, contents: strings.HasSuffix(p, "**")})
	}
	return out, nil
}

// globRegexp turns one glob into an anchored regular expression.
func globRegexp(p string) (*regexp.Regexp, error) {
	var b strings.Builder
	b.WriteString("^")
	for i := 0; i < len(p); i++ {
		c := p[i]
		switch {
		case c == '*' && i+1 < len(p) && p[i+1] == '*':
			i++
			if i+1 < len(p) && p[i+1] == '/' {
				i++
				b.WriteString("(?:.*/)?")
			} else {
				b.WriteString(".*")
			}
		case c == '*':
			b.WriteString("[^/]*")
		case c == '?':
			b.WriteString("[^/]")
		default:
			b.WriteString(regexp.QuoteMeta(string(c)))
		}
	}
	b.WriteString("$")
	return regexp.Compile(b.String())
}

// match reports whether a folder path matches. The folder also matches a
// pattern that names its contents, so "**/features/**" leaves out the folder
// "app/features" itself.
func (g globs) match(dir string) bool {
	for _, x := range g {
		if x.re.MatchString(dir) || (x.contents && x.re.MatchString(dir+"/")) {
			return true
		}
	}
	return false
}
