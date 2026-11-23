//! Path globs for the include and exclude lists of a request. "**" matches
//! any number of folders, "*" matches within one folder, and "?" matches one
//! character. The Go and TypeScript adapters follow the same rules.

/// A list of path globs; a path matches the list when it matches any one.
#[derive(Debug, Clone, Default)]
pub struct Globs {
    /// The patterns, as characters.
    patterns: Vec<Vec<char>>,
}

impl Globs {
    /// Compiles a list of patterns.
    pub fn new(patterns: &[String]) -> Self {
        Globs {
            patterns: patterns.iter().map(|p| p.chars().collect()).collect(),
        }
    }

    /// Reports whether the list holds no pattern.
    pub fn is_empty(&self) -> bool {
        self.patterns.is_empty()
    }

    /// Reports whether a path matches any pattern. A folder also matches a
    /// pattern that names its contents, so "**/examples/**" leaves out the
    /// folder "examples" itself.
    pub fn matches(&self, path: &str) -> bool {
        let s: Vec<char> = path.chars().collect();
        let mut dir = s.clone();
        dir.push('/');
        self.patterns
            .iter()
            .any(|p| glob_match(p, &s) || (p.ends_with(&['*', '*']) && glob_match(p, &dir)))
    }
}

/// Matches one pattern against a whole path.
fn glob_match(p: &[char], s: &[char]) -> bool {
    match p.first() {
        None => s.is_empty(),
        Some('*') if p.get(1) == Some(&'*') => {
            if p.get(2) == Some(&'/') {
                let rest = &p[3..];
                glob_match(rest, s)
                    || (0..s.len()).any(|i| s[i] == '/' && glob_match(rest, &s[i + 1..]))
            } else {
                let rest = &p[2..];
                (0..=s.len()).any(|i| glob_match(rest, &s[i..]))
            }
        }
        Some('*') => {
            let rest = &p[1..];
            for i in 0..=s.len() {
                if glob_match(rest, &s[i..]) {
                    return true;
                }
                if i < s.len() && s[i] == '/' {
                    break;
                }
            }
            false
        }
        Some('?') => !s.is_empty() && s[0] != '/' && glob_match(&p[1..], &s[1..]),
        Some(c) => !s.is_empty() && s[0] == *c && glob_match(&p[1..], &s[1..]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Compiles one pattern for a test.
    fn g(p: &str) -> Globs {
        Globs::new(&[p.to_string()])
    }

    #[test]
    fn double_star_matches_any_number_of_folders() {
        assert!(g("**/examples/**").matches("examples"));
        assert!(g("**/examples/**").matches("a/examples/b/c.rs"));
        assert!(g("crates/**").matches("crates/searcher/src/lib.rs"));
        assert!(!g("crates/**").matches("src/main.rs"));
    }

    #[test]
    fn single_star_stays_in_one_folder() {
        assert!(g("crates/*").matches("crates/cli"));
        assert!(!g("crates/*").matches("crates/cli/src"));
        assert!(g("src/?ib.rs").matches("src/lib.rs"));
    }
}
