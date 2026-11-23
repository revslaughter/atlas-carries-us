// Path globs for the include, exclude and tests lists of a request. The rules
// match the Go adapter's: "**" matches any number of folders, "*" matches
// within one folder, and "?" matches one character.

/** A compiled list of globs. */
export interface Globs {
  /** The number of patterns in the list. */
  readonly size: number;
  /** Reports whether a path matches any pattern. A folder also matches a pattern that names its contents. */
  match(path: string): boolean;
}

/** Turns one glob into an anchored regular expression. */
function globRegExp(p: string): RegExp {
  let re = "^";
  for (let i = 0; i < p.length; i++) {
    const c = p[i];
    if (c === "*" && p[i + 1] === "*") {
      i++;
      if (p[i + 1] === "/") {
        i++;
        re += "(?:.*/)?";
      } else {
        re += ".*";
      }
    } else if (c === "*") {
      re += "[^/]*";
    } else if (c === "?") {
      re += "[^/]";
    } else {
      re += c.replace(/[.+^${}()|[\]\\]/g, "\\$&");
    }
  }
  return new RegExp(re + "$");
}

/** Compiles path globs. A pattern that ends in "**" also matches the folder whose contents it names. */
export function compileGlobs(patterns: readonly string[]): Globs {
  const list = patterns.map(p => ({ re: globRegExp(p), contents: p.endsWith("**") }));
  return {
    size: list.length,
    match(path: string): boolean {
      return list.some(g => g.re.test(path) || (g.contents && g.re.test(path + "/")));
    },
  };
}
