//! The session secret of `acu serve` and `acu studio` (issue #42). acu
//! makes a random token at start and prints an address that holds it. The
//! browser trades the token for a cookie once; an MCP client sends it as a
//! bearer token. Every call to `/api/` and `/mcp` must carry it, so another
//! process or user on the computer cannot drive the server.

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

/// One server's token.
#[derive(Clone)]
pub struct Secret {
    /// The token: 64 hex digits, or the value of [`ENV`].
    token: String,
}

impl std::fmt::Debug for Secret {
    /// Writes the type without the token, so no log shows it by accident.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Secret(..)")
    }
}

impl Secret {
    /// Makes a token from 32 random bytes of the operating system, written
    /// as 64 hex digits.
    pub fn random() -> Result<Secret, String> {
        let mut bytes = [0u8; 32];
        getrandom::fill(&mut bytes).map_err(|e| {
            format!("the system gave no random bytes for the session token ({e}); set {ENV} to a token of your own")
        })?;
        let token = bytes.iter().map(|b| format!("{b:02x}")).collect();
        Ok(Secret { token })
    }

    /// Takes the token from [`ENV`] when it holds one, else makes a random
    /// one. A value that [`Secret::parse`] refuses stops acu. It then
    /// removes [`ENV`] from acu's environment, so no adapter, build script
    /// or git that acu starts inherits the token. Call it before acu starts
    /// any thread.
    pub fn from_env() -> Result<Secret, String> {
        let got = std::env::var(ENV);
        std::env::remove_var(ENV);
        match got {
            Ok(v) if !v.is_empty() => Secret::parse(&v),
            Ok(_) | Err(std::env::VarError::NotPresent) => Secret::random(),
            Err(std::env::VarError::NotUnicode(_)) => Err(format!(
                "{ENV} holds bytes that form no text; unset it to get a random token"
            )),
        }
    }

    /// Checks a token that a person chose: [`MIN_LEN`] to [`MAX_LEN`]
    /// characters, each a letter, a digit, `-`, `.`, `_` or `~`. Those
    /// characters need no escape in an address or a cookie.
    pub fn parse(v: &str) -> Result<Secret, String> {
        let ok_char = |c: char| c.is_ascii_alphanumeric() || matches!(c, '-' | '.' | '_' | '~');
        if !(MIN_LEN..=MAX_LEN).contains(&v.len()) || !v.chars().all(ok_char) {
            return Err(format!(
                "{ENV} must hold {MIN_LEN} to {MAX_LEN} characters, each a letter, a digit, '-', '.', '_' or '~'; fix it, or unset it to get a random token"
            ));
        }
        Ok(Secret {
            token: v.to_string(),
        })
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

    /// Says whether a request carries the token: in the cookie of
    /// [`cookie_name`] for `port`, or as `Authorization: Bearer <token>`.
    pub fn admits(&self, headers: &HeaderMap, port: u16) -> bool {
        let name = cookie_name(port);
        let in_cookie = headers
            .get_all(header::COOKIE)
            .iter()
            .filter_map(|v| v.to_str().ok())
            .flat_map(|v| v.split(';'))
            .filter_map(|pair| pair.trim().split_once('='))
            .any(|(k, v)| k == name && self.matches(v));
        in_cookie || self.admits_bearer(headers)
    }

    /// Says whether a request carries the token as
    /// `Authorization: Bearer <token>`. The MCP server takes only this
    /// form, as the MCP specification asks, so no browser cookie reaches
    /// it.
    pub fn admits_bearer(&self, headers: &HeaderMap) -> bool {
        bearers(headers).any(|t| self.matches(t))
    }
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

impl Secret {
    /// Makes the `Set-Cookie` value that holds the token for `port`. The
    /// cookie lasts until the browser closes, no script may read it, and no
    /// other site's request carries it.
    pub fn set_cookie(&self, port: u16) -> String {
        format!(
            "{}={}; Path=/; HttpOnly; SameSite=Strict",
            cookie_name(port),
            self.token
        )
    }
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
/// the query, without the token.
pub fn take_token(query: &str) -> (Option<&str>, String) {
    let mut token = None;
    let mut rest = Vec::new();
    for pair in query.split('&').filter(|p| !p.is_empty()) {
        match pair.split_once('=') {
            Some((QUERY, v)) => token = Some(v),
            None if pair == QUERY => token = Some(""),
            _ => rest.push(pair),
        }
    }
    (token, rest.join("&"))
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use axum::http::HeaderValue;

    /// A token of the allowed form, for tests.
    pub const TEST: &str = "0123456789abcdef0123456789abcdef";

    #[test]
    fn a_random_token_holds_64_hex_digits_and_differs_each_time() {
        let a = Secret::random().unwrap();
        let b = Secret::random().unwrap();
        assert_eq!(a.token().len(), 64);
        assert!(a.token().chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(a.token(), b.token());
        assert_eq!(format!("{a:?}"), "Secret(..)");
    }

    #[test]
    fn a_chosen_token_needs_the_safe_form() {
        assert!(Secret::parse(TEST).is_ok());
        assert!(Secret::parse("a-b.c_d~e".repeat(4).as_str()).is_ok());
        assert!(Secret::parse("short").is_err());
        assert!(Secret::parse(&"x".repeat(MAX_LEN + 1)).is_err());
        assert!(Secret::parse(&format!("{TEST};x=y")).is_err());
        assert!(Secret::parse(&format!("{TEST} ")).is_err());
        assert!(Secret::parse(&format!("{TEST}é")).is_err());
    }

    #[test]
    fn matching_needs_the_whole_token() {
        let s = Secret::parse(TEST).unwrap();
        assert!(s.matches(TEST));
        assert!(!s.matches(&TEST[1..]));
        assert!(!s.matches(&format!("{TEST}0")));
        assert!(!s.matches(""));
        assert!(!s.matches(&TEST.replace('0', "1")));
    }

    #[test]
    fn a_request_carries_the_token_in_the_cookie_or_as_a_bearer() {
        let s = Secret::parse(TEST).unwrap();
        let mut h = HeaderMap::new();
        assert!(!s.admits(&h, 4321));
        h.insert(
            header::COOKIE,
            HeaderValue::from_str(&format!("other=1; acu-token-4321={TEST}")).unwrap(),
        );
        assert!(s.admits(&h, 4321));
        // A cookie for another port does not count.
        assert!(!s.admits(&h, 4322));
        let mut h = HeaderMap::new();
        h.insert(
            header::COOKIE,
            HeaderValue::from_static("acu-token-4321=wrong"),
        );
        h.append(
            header::COOKIE,
            HeaderValue::from_str(&format!("acu-token-4321={TEST}")).unwrap(),
        );
        assert!(s.admits(&h, 4321));
        let mut h = HeaderMap::new();
        h.insert(
            header::AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {TEST}")).unwrap(),
        );
        assert!(s.admits(&h, 4321));
        h.insert(
            header::AUTHORIZATION,
            HeaderValue::from_str(&format!("bearer  {TEST}")).unwrap(),
        );
        assert!(s.admits(&h, 4321));
        h.insert(
            header::AUTHORIZATION,
            HeaderValue::from_str(&format!("Basic {TEST}")).unwrap(),
        );
        assert!(!s.admits(&h, 4321));
        h.insert(header::AUTHORIZATION, HeaderValue::from_static("Bearer"));
        assert!(!s.admits(&h, 4321));
        // The MCP form takes the bearer token only, never the cookie.
        let mut h = HeaderMap::new();
        h.insert(
            header::COOKIE,
            HeaderValue::from_str(&format!("acu-token-4321={TEST}")).unwrap(),
        );
        assert!(!s.admits_bearer(&h));
        h.insert(
            header::AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {TEST}")).unwrap(),
        );
        assert!(s.admits_bearer(&h));
    }

    #[test]
    fn reading_the_variable_removes_it_from_the_environment() {
        // The one test that touches ACU_TOKEN; no other test reads it.
        std::env::set_var(ENV, TEST);
        let s = Secret::from_env().unwrap();
        assert_eq!(s.token(), TEST);
        assert!(std::env::var_os(ENV).is_none());
        std::env::set_var(ENV, "short");
        assert!(Secret::from_env().is_err());
        assert!(std::env::var_os(ENV).is_none());
        assert_eq!(Secret::from_env().unwrap().token().len(), 64);
    }

    #[test]
    fn the_cookie_stays_out_of_scripts_and_other_sites() {
        let s = Secret::parse(TEST).unwrap();
        assert_eq!(
            s.set_cookie(4321),
            format!("acu-token-4321={TEST}; Path=/; HttpOnly; SameSite=Strict")
        );
    }

    #[test]
    fn the_query_gives_up_the_token_and_keeps_the_rest() {
        assert_eq!(take_token(""), (None, String::new()));
        assert_eq!(take_token("token=abc"), (Some("abc"), String::new()));
        assert_eq!(
            take_token("a=1&token=abc&b=2"),
            (Some("abc"), "a=1&b=2".to_string())
        );
        assert_eq!(take_token("token"), (Some(""), String::new()));
        assert_eq!(take_token("tokens=1"), (None, "tokens=1".to_string()));
    }
}
