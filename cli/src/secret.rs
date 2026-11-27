//! The session secret of `acu serve` and `acu studio` (issue #42). acu
//! makes a random token at start and prints an address that holds it. The
//! browser exchanges the token for a cookie once; an MCP client sends the
//! token as a bearer token. Every call to `/api/` and `/mcp` must carry one
//! of them, so another process or user on the computer cannot send calls
//! to the server.
//!
//! The cookie holds a second random value, not the token. A browser sends a
//! cookie of `127.0.0.1` to every port of it, so another local server can
//! read the cookie; with a value of its own, the cookie opens `/api/` only,
//! never `/mcp`.

use axum::http::{header, HeaderMap};

/// The environment variable that fixes the token, so an MCP client's config
/// stays valid after a restart.
pub const ENV: &str = "ACU_TOKEN";

/// The query parameter that carries the token in the printed address.
pub const QUERY: &str = "token";

/// The fewest characters that a token from [`ENV`] may hold.
pub const MIN_LEN: usize = 32;

/// The most characters that a token from [`ENV`] may hold.
pub const MAX_LEN: usize = 512;

/// One server's token and cookie value.
#[derive(Clone)]
pub struct Secret {
    /// The token: 64 hex digits, or the value of [`ENV`].
    token: String,
    /// The cookie's value: 64 random hex digits, made at each start.
    cookie: String,
}

impl std::fmt::Debug for Secret {
    /// Writes the type without the values, so no log shows them by accident.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Secret(..)")
    }
}

impl Secret {
    /// Makes a token and a cookie value, each from 32 random bytes of the
    /// operating system.
    pub fn random() -> Result<Secret, String> {
        Ok(Secret {
            token: random_hex()?,
            cookie: random_hex()?,
        })
    }

    /// Makes the secret from the value of [`ENV`] that `main` read: a
    /// value that [`Secret::parse`] accepts fixes the token; an empty or
    /// missing value gives a random token.
    pub fn from_value(got: Result<String, std::env::VarError>) -> Result<Secret, String> {
        match got {
            Ok(v) if !v.is_empty() => Secret::parse(&v),
            Ok(_) | Err(std::env::VarError::NotPresent) => Secret::random(),
            Err(std::env::VarError::NotUnicode(_)) => Err(format!(
                "{ENV} holds bytes that form no text; fix it, or unset it to get a random token"
            )),
        }
    }

    /// Checks a token that a person chose: [`MIN_LEN`] to [`MAX_LEN`]
    /// characters, each a letter, a digit, `-`, `.`, `_` or `~`. Those
    /// characters need no escape in an address. The cookie value stays
    /// random.
    pub fn parse(v: &str) -> Result<Secret, String> {
        let ok_char = |c: char| c.is_ascii_alphanumeric() || matches!(c, '-' | '.' | '_' | '~');
        if !(MIN_LEN..=MAX_LEN).contains(&v.len()) || !v.chars().all(ok_char) {
            return Err(format!(
                "{ENV} must hold {MIN_LEN} to {MAX_LEN} characters, each a letter, a digit, '-', '.', '_' or '~'; fix it, or unset it to get a random token"
            ));
        }
        Ok(Secret {
            token: v.to_string(),
            cookie: random_hex()?,
        })
    }

    /// Makes a secret with a known token and cookie value, for tests.
    #[cfg(test)]
    pub fn fixed(token: &str, cookie: &str) -> Secret {
        Secret {
            token: token.to_string(),
            cookie: cookie.to_string(),
        }
    }

    /// Returns the token, for the printed address.
    pub fn token(&self) -> &str {
        &self.token
    }

    /// Says whether `given` equals the token. The time taken depends on the
    /// lengths only, not on where the two first differ.
    pub fn matches(&self, given: &str) -> bool {
        same(self.token.as_bytes(), given.as_bytes())
    }

    /// Says whether a request may reach the API: it carries the cookie of
    /// [`cookie_name`] for `port` with the cookie value, or the token as
    /// `Authorization: Bearer <token>`.
    pub fn admits(&self, headers: &HeaderMap, port: u16) -> bool {
        let name = cookie_name(port);
        let in_cookie = headers
            .get_all(header::COOKIE)
            .iter()
            .filter_map(|v| v.to_str().ok())
            .flat_map(|v| v.split(';'))
            .filter_map(|pair| pair.trim().split_once('='))
            .any(|(k, v)| k == name && same(self.cookie.as_bytes(), v.as_bytes()));
        in_cookie || self.admits_bearer(headers)
    }

    /// Says whether a request carries the token as
    /// `Authorization: Bearer <token>`. The MCP server takes only this
    /// form, as the MCP specification asks, so the cookie never opens it.
    pub fn admits_bearer(&self, headers: &HeaderMap) -> bool {
        bearers(headers).any(|t| self.matches(t))
    }

    /// Makes the `Set-Cookie` value that holds the cookie value for `port`.
    /// The cookie lasts until the browser closes, and no script may read
    /// it. `SameSite=Lax` lets the first visit from a link in a terminal or
    /// a chat page carry it after the redirect; a write from another site
    /// still meets the Origin, Fetch Metadata and custom-header guards.
    pub fn set_cookie(&self, port: u16) -> String {
        format!(
            "{}={}; Path=/; HttpOnly; SameSite=Lax",
            cookie_name(port),
            self.cookie
        )
    }
}

/// Makes 32 random bytes of the operating system, written as 64 hex digits.
fn random_hex() -> Result<String, String> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(|e| {
        format!("the system gave no random bytes for the session token ({e}); try again, or check the system's random source")
    })?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

/// Lists the bearer tokens in a request's `Authorization` headers.
pub fn bearers(headers: &HeaderMap) -> impl Iterator<Item = &str> {
    headers
        .get_all(header::AUTHORIZATION)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .filter_map(|v| v.trim().split_once(' '))
        .filter(|(scheme, _)| scheme.eq_ignore_ascii_case("bearer"))
        .map(|(_, t)| t.trim())
}

/// The cookie's name for a port. A browser sends a cookie of a host to
/// every port of it, so a name per port keeps two servers' cookies apart.
pub fn cookie_name(port: u16) -> String {
    format!("acu-token-{port}")
}

/// Compares two byte strings in a time that depends on their lengths only.
fn same(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let diff = a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y));
    std::hint::black_box(diff) == 0
}

/// Splits a query string into the token, when it holds one, and the rest of
/// the query, without the token. The token's value comes percent-decoded,
/// so a tool that writes `~` as `%7E` still matches; the last `token`
/// parameter counts.
pub fn take_token(query: &str) -> (Option<String>, String) {
    let mut token = None;
    let mut rest = Vec::new();
    for pair in query.split('&').filter(|p| !p.is_empty()) {
        match pair.split_once('=') {
            Some((QUERY, v)) => token = Some(percent_decode(v)),
            None if pair == QUERY => token = Some(String::new()),
            _ => rest.push(pair),
        }
    }
    (token, rest.join("&"))
}

/// Decodes `%XX` sequences in a query value; other bytes stay as they
/// stand, and bytes that form no text turn into replacement characters.
fn percent_decode(v: &str) -> String {
    let b = v.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        let hex = |c: u8| (c as char).to_digit(16);
        if b[i] == b'%' && i + 2 < b.len() {
            if let (Some(h), Some(l)) = (hex(b[i + 1]), hex(b[i + 2])) {
                out.push((h * 16 + l) as u8);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use axum::http::HeaderValue;

    /// A token of the allowed form, for tests.
    pub const TEST: &str = "0123456789abcdef0123456789abcdef";

    /// The cookie value of every test secret.
    pub const COOKIE: &str = "fedcba9876543210fedcba9876543210";

    #[test]
    fn a_random_secret_holds_two_different_values_of_64_hex_digits() {
        let a = Secret::random().unwrap();
        let b = Secret::random().unwrap();
        assert_eq!(a.token().len(), 64);
        assert!(a.token().chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(a.token(), b.token());
        assert_ne!(a.token(), a.cookie);
        assert_eq!(a.cookie.len(), 64);
        assert_eq!(format!("{a:?}"), "Secret(..)");
    }

    #[test]
    fn a_chosen_token_needs_the_safe_form_and_keeps_a_random_cookie() {
        let s = Secret::parse(TEST).unwrap();
        assert_eq!(s.token(), TEST);
        assert_eq!(s.cookie.len(), 64);
        assert_ne!(s.cookie, TEST);
        assert!(Secret::parse("a-b.c_d~e".repeat(4).as_str()).is_ok());
        assert!(Secret::parse("short").is_err());
        assert!(Secret::parse(&"x".repeat(MAX_LEN + 1)).is_err());
        assert!(Secret::parse(&format!("{TEST};x=y")).is_err());
        assert!(Secret::parse(&format!("{TEST} ")).is_err());
        assert!(Secret::parse(&format!("{TEST}é")).is_err());
    }

    #[test]
    fn the_variable_fixes_the_token_and_an_empty_one_counts_as_unset() {
        use std::env::VarError;
        assert_eq!(Secret::from_value(Ok(TEST.into())).unwrap().token(), TEST);
        assert_eq!(
            Secret::from_value(Ok(String::new())).unwrap().token().len(),
            64
        );
        assert_eq!(
            Secret::from_value(Err(VarError::NotPresent))
                .unwrap()
                .token()
                .len(),
            64
        );
        assert!(Secret::from_value(Ok("short".into())).is_err());
        assert!(Secret::from_value(Err(VarError::NotUnicode("x".into()))).is_err());
    }

    #[test]
    fn matching_needs_the_whole_token() {
        let s = Secret::fixed(TEST, COOKIE);
        assert!(s.matches(TEST));
        assert!(!s.matches(&TEST[1..]));
        assert!(!s.matches(&format!("{TEST}0")));
        assert!(!s.matches(""));
        assert!(!s.matches(&TEST.replace('0', "1")));
    }

    #[test]
    fn the_api_takes_the_cookie_value_or_the_bearer_token() {
        let s = Secret::fixed(TEST, COOKIE);
        let with = |name: header::HeaderName, v: String| {
            let mut h = HeaderMap::new();
            h.insert(name, HeaderValue::from_str(&v).unwrap());
            h
        };
        assert!(!s.admits(&HeaderMap::new(), 4321));
        let h = with(header::COOKIE, format!("other=1; acu-token-4321={COOKIE}"));
        assert!(s.admits(&h, 4321));
        // A cookie for another port does not count.
        assert!(!s.admits(&h, 4322));
        // The token in the cookie does not count: the cookie holds its own value.
        assert!(!s.admits(
            &with(header::COOKIE, format!("acu-token-4321={TEST}")),
            4321
        ));
        let mut h = with(header::COOKIE, "acu-token-4321=wrong".into());
        h.append(
            header::COOKIE,
            HeaderValue::from_str(&format!("acu-token-4321={COOKIE}")).unwrap(),
        );
        assert!(s.admits(&h, 4321));
        assert!(s.admits(&with(header::AUTHORIZATION, format!("Bearer {TEST}")), 4321));
        assert!(s.admits(
            &with(header::AUTHORIZATION, format!("bearer  {TEST}")),
            4321
        ));
        assert!(!s.admits(&with(header::AUTHORIZATION, format!("Basic {TEST}")), 4321));
        assert!(!s.admits(&with(header::AUTHORIZATION, "Bearer".into()), 4321));
        assert!(!s.admits(
            &with(header::AUTHORIZATION, format!("Bearer {COOKIE}")),
            4321
        ));
    }

    #[test]
    fn mcp_takes_the_bearer_token_only() {
        let s = Secret::fixed(TEST, COOKIE);
        let mut h = HeaderMap::new();
        h.insert(
            header::COOKIE,
            HeaderValue::from_str(&format!("acu-token-4321={COOKIE}")).unwrap(),
        );
        assert!(!s.admits_bearer(&h));
        h.insert(
            header::AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {TEST}")).unwrap(),
        );
        assert!(s.admits_bearer(&h));
    }

    #[test]
    fn the_cookie_holds_its_own_value_and_stays_out_of_scripts() {
        let s = Secret::fixed(TEST, COOKIE);
        assert_eq!(
            s.set_cookie(4321),
            format!("acu-token-4321={COOKIE}; Path=/; HttpOnly; SameSite=Lax")
        );
    }

    #[test]
    fn the_query_gives_up_the_token_and_keeps_the_rest() {
        let t = |q: &str| take_token(q);
        assert_eq!(t(""), (None, String::new()));
        assert_eq!(t("token=abc"), (Some("abc".into()), String::new()));
        assert_eq!(
            t("a=1&token=abc&b=2"),
            (Some("abc".into()), "a=1&b=2".to_string())
        );
        assert_eq!(t("token"), (Some(String::new()), String::new()));
        assert_eq!(t("tokens=1"), (None, "tokens=1".to_string()));
        // The last token counts; percent escapes decode.
        assert_eq!(t("token=a&token=b").0.as_deref(), Some("b"));
        assert_eq!(t("token=a%7Eb%7e").0.as_deref(), Some("a~b~"));
        assert_eq!(t("token=50%").0.as_deref(), Some("50%"));
        assert_eq!(t("token=%4").0.as_deref(), Some("%4"));
        assert_eq!(t("token=%zz").0.as_deref(), Some("%zz"));
    }
}
