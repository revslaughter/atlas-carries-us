<!-- This note holds the research for the session secret of acu serve and acu studio (issue #42) and the Host check of acu serve (issue #43), with a source for each claim, as of Nov 2026. -->

# Session secret and Host check (as of Nov 2026)

This note answers six questions for issues #42 and #43. Each section
matches one question. Source links to code name a commit or a released
version, so the quoted lines stay findable.

Pinned sources:

- Jupyter Server at commit `c4c5a51`:
  [`auth/identity.py`](https://github.com/jupyter-server/jupyter_server/blob/c4c5a51d2cf77d31a266376582ee05c4d661ad4b/jupyter_server/auth/identity.py),
  [`base/handlers.py`](https://github.com/jupyter-server/jupyter_server/blob/c4c5a51d2cf77d31a266376582ee05c4d661ad4b/jupyter_server/base/handlers.py),
  [`serverapp.py`](https://github.com/jupyter-server/jupyter_server/blob/c4c5a51d2cf77d31a266376582ee05c4d661ad4b/jupyter_server/serverapp.py),
  [`auth/login.py`](https://github.com/jupyter-server/jupyter_server/blob/c4c5a51d2cf77d31a266376582ee05c4d661ad4b/jupyter_server/auth/login.py).
- Tornado at commit `dbaa6c5`:
  [`httputil.py`](https://github.com/tornadoweb/tornado/blob/dbaa6c5bc90b48f1f317517530241560f1767bf8/tornado/httputil.py).
- Vite at commit `7d0c054`:
  [`hostCheck.ts`](https://github.com/vitejs/vite/blob/7d0c054f41ba1107a1d7d9ddae23930d9d6b4f8e/packages/vite/src/node/server/middlewares/hostCheck.ts),
  and the package it calls, host-validation-middleware at commit `0f8f544`:
  [`isHostAllowed.ts`](https://github.com/sapphi-red/host-validation-middleware/blob/0f8f544cf755e1e789c00a9e005d6953e064e4cc/src/isHostAllowed.ts).
- webpack-dev-server at commit `dc21a20`:
  [`lib/Server.js`](https://github.com/webpack/webpack-dev-server/blob/dc21a20bede359c0db8120de077cf938b6c7edc8/lib/Server.js).
- rmcp 3.10.0, the version that `cli/Cargo.toml` pins:
  [`src/transport/streamable_http_server/tower.rs`](https://docs.rs/crate/rmcp/3.10.0/source/src/transport/streamable_http_server/tower.rs).

## 1. Jupyter Server's token

### 1.1 Token

1. Jupyter makes the token from 24 random bytes of `os.urandom`, written as
   48 lowercase hex digits (192 bits):
   `binascii.hexlify(os.urandom(24)).decode("ascii")` (`identity.py`,
   `_token_default`).
2. The environment variable `JUPYTER_TOKEN` fixes the token. A second
   variable, `JUPYTER_TOKEN_FILE`, names a file that holds it. Jupyter reads
   the variable first, then the file, then makes a random token
   (`identity.py`, `_token_default`).
3. Jupyter puts no rule on the form of a token from `JUPYTER_TOKEN`. The
   source checks no length and no alphabet (`identity.py`).
4. An empty token turns authentication off. The help text calls that "NOT
   RECOMMENDED" (`identity.py`, the `token` trait).

### 1.2 Where Jupyter reads the token

1. Jupyter reads the query parameter `token` first. Without it, Jupyter
   reads the `Authorization` header with the pattern
   `(token|bearer)\s+(.+)`, case-insensitive. So Jupyter accepts both
   `Authorization: token <t>` and `Authorization: Bearer <t>`
   (`identity.py`, `auth_header_pat` and `get_token`).
2. The legacy login handler accepts only `token <t>`
   (`login.py`, `auth_header_pat = re.compile(r"token\s+(.+)", ...)`).
3. The operator documents show only the form `Authorization: token abcdef...`
   ([Jupyter Server security](https://jupyter-server.readthedocs.io/en/latest/operators/security.html)).

### 1.3 Comparison

1. Jupyter compares with plain `==`: `if user_token == token:`
   (`identity.py`, `get_user_token`). The password form also uses `==`
   (`identity.py`, `process_login_form`). So Jupyter does not compare in
   constant time. acu should not copy that line.

### 1.4 From the URL token to a cookie

1. Jupyter sets the login cookie on the same response that carries the
   valid token (`identity.py`, `_get_user` calls `set_login_cookie` when
   `token_user` holds a user). The source shows no redirect that strips the
   token from the address.
2. The cookie holds no token. It holds a user model, signed by Tornado's
   `set_secure_cookie` with an HMAC key, the `cookie_secret`
   (`identity.py`, `set_login_cookie`). Jupyter keeps that key in the file
   `jupyter_cookie_secret` in the runtime folder, written with
   `secure_write` (`serverapp.py`, `cookie_secret_file`).
3. The cookie name holds the host and the port:
   `_non_alphanum.sub("-", f"username-{handler.request.host}")`. For
   `127.0.0.1:8888` that gives `username-127-0-0-1-8888`. The docstring says
   the name avoids "collisions for multiple servers on one hostname with
   different ports" (`identity.py`, `get_cookie_name`).
4. Attributes: `HttpOnly` by default; `Secure` only when the request came
   over https; `Path` set to the server's base URL. Jupyter sets no
   `SameSite` by default, because `cookie_options` starts empty
   (`identity.py`, `set_login_cookie` and the `cookie_options` trait).
5. A new port needs the token again, because the cookie name changes
   ([Jupyter Server security](https://jupyter-server.readthedocs.io/en/latest/operators/security.html)).

### 1.5 What a token request skips in Jupyter

1. A token-authenticated request skips the XSRF check
   (`handlers.py`, `check_xsrf_cookie`: "Token-authenticated requests do not
   need additional XSRF-check").
2. It skips the Origin check (`identity.py`, `should_check_origin`).
3. Jupyter even answers it with `Access-Control-Allow-Origin` set to the
   request's `Origin` (`handlers.py`, "allow token-authenticated requests
   cross-origin by default").
4. acu should not copy these three points. A bearer token proves the caller
   knows the secret. It does not make a browser request from another site
   safe to answer with readable data.

### 1.6 Other parts of Jupyter's design

1. **Redirect file.** When Jupyter opens a browser, it writes
   `jpserver-<pid>-open.html` in the runtime folder and opens that file. So
   the token stays off the browser's command line. The help text says this
   stops "other users on a multi-user system from running code in your
   Jupyter session as you" (`serverapp.py`, `use_redirect_file`).
2. **Host check.** Jupyter strips the port from `Host`, then accepts any
   loopback IP literal and each name in `local_hostnames` (default
   `["localhost"]`) (`handlers.py`, `check_host`). It turns the check off
   (`allow_remote_access`) by default when the server listens on an address
   other than loopback (`serverapp.py`, `_default_allow_remote`). Issue #43
   follows the same split.
3. **Missing Host.** Tornado refuses an HTTP/1.1 request without `Host`
   ("Missing Host header"). For HTTP/1.0 it assumes `127.0.0.1`
   (`httputil.py`, `HTTPServerRequest.__init__`).

### 1.7 Known weaknesses

1. Cookies do not stay on one port (section 2). Jupyter's per-port cookie
   name stops two servers from overwriting each other's cookie. It does not
   stop a service on another port of the same host from reading the cookie.
   The signed cookie gives no help here, because a thief replays the whole
   signed value.
2. The comparison with `==` (1.3).
3. Token-authenticated requests get CORS and skip XSRF (1.5).

## 2. Cookies and ports

### 2.1 A cookie goes to every port

1. RFC 6265 section 8.5: "Cookies do not provide isolation by port. If a
   cookie is readable by a service running on one port, the cookie is also
   readable by a service running on another port of the same server. If a
   cookie is writable by a service on one port, the cookie is also writable
   by a service running on another port of the same server." It adds:
   "servers SHOULD NOT both run mutually distrusting services on different
   ports of the same host"
   ([RFC 6265 section 8.5](https://www.rfc-editor.org/rfc/rfc6265#section-8.5)).
2. So a cookie that acu sets on `127.0.0.1:4321` goes with each request to
   `127.0.0.1:<any port>`. It does not go to `localhost`, because the
   browser keys cookies by host name, and `localhost` differs from
   `127.0.0.1` ([RFC 6265 section 5.4](https://www.rfc-editor.org/rfc/rfc6265#section-5.4)).

### 2.2 Two acu servers on two ports

1. With one fixed cookie name, the second server's cookie replaces the
   first one's. The first tab then fails. A name per port, as Jupyter uses
   (1.4.3), prevents that.
2. Each server still receives the other's cookie. A server that accepts any
   cookie whose value matches its token stays correct, because the other
   server's token differs.

### 2.3 A hostile listener on another local port

1. It receives the acu cookie whenever the browser sends a request to that
   port on the same host name (2.1). That happens when the person opens the
   hostile port's page. It also happens when a page on the same site sends a
   request there (2.3.3).
2. It can also write a cookie with acu's name (RFC 6265 section 8.5). The
   worst case: it replaces acu's cookie with a wrong value, and the person
   must open the printed address again. It cannot plant a working token,
   because it does not know one.
3. `SameSite=Strict` does not help here. Same-site ignores the port:
   "Websites that have the same scheme and the same eTLD+1 are considered
   same-site", and the comparison tables say "ports don't matter"
   ([web.dev](https://web.dev/articles/same-site-same-origin)).
   `http://127.0.0.1:9999` and `http://127.0.0.1:4321` count as one site.
4. `SameSite=Strict` does help against other sites: a request that a page
   on `attacker.example` starts carries no acu cookie, not even on a
   top-level navigation
   ([MDN Set-Cookie](https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Headers/Set-Cookie#samesitesamesite-value)).
5. No cookie attribute closes 2.3.1 on plain http. RFC 6265 states the
   rule as a "SHOULD NOT" for the operator (2.1.1). acu can only state the
   limit in `docs/studio.md`.

### 2.4 Secure and the `__Host-` prefix on loopback

1. Secure Contexts treats `http://127.0.0.1`, `http://localhost` and
   `http://*.localhost` as potentially trustworthy
   ([W3C Secure Contexts](https://www.w3.org/TR/secure-contexts/#is-origin-trustworthy),
   [MDN](https://developer.mozilla.org/en-US/docs/Web/Security/Defenses/Secure_Contexts)).
2. The cookie rules follow that only in part. An httpwg issue tested
   `http://localhost` in 2023: Chrome and Firefox keep a `Secure` cookie;
   Safari does not. Only Firefox keeps a `__Host-` or `__Secure-` cookie.
   The issue closed in March 2024 with a change to the draft that lets
   potentially trustworthy origins count
   ([httpwg/http-extensions#2605](https://github.com/httpwg/http-extensions/issues/2605)).
   Firefox made the change in version 75
   ([Mozilla bug 1618113](https://bugzilla.mozilla.org/show_bug.cgi?id=1618113)).
   This note found no newer test of all three browsers.
3. Consequence: `Secure` or `__Host-` would make Safari drop the cookie,
   and `__Host-` would make Chrome drop it too (as of the 2023 test). Both
   would then fail the login. Neither attribute adds protection on
   loopback, because no network carries the traffic. Leave both off.

## 3. A token in the URL

### 3.1 Where a URL token leaks

1. OWASP lists the `Referer` header, web logs, shared systems, browser
   history, the browser cache and shoulder surfing. It says that "Simply
   using HTTPS does not resolve this vulnerability"
   ([OWASP](https://community.owasp.org/vulnerabilities/Information_exposure_through_query_strings_in_url)).
2. RFC 6750 says: "Bearer tokens SHOULD NOT be passed in page URLs (for
   example, as query string parameters)." Its reason: browsers and servers
   may keep URLs in history and logs
   ([RFC 6750 section 5.3](https://www.rfc-editor.org/rfc/rfc6750#section-5.3)).
   It also says that a client that uses the query form SHOULD send
   `Cache-Control: no-store`
   ([RFC 6750 section 2.3](https://www.rfc-editor.org/rfc/rfc6750#section-2.3)).
3. The MCP spec forbids the query form for MCP: "Access tokens MUST NOT be
   included in the URI query string"
   ([MCP authorization, 2025-06-18](https://modelcontextprotocol.io/specification/2025-06-18/basic/authorization)).
   So acu must read `?token=` on page addresses only, never on `/mcp`.

### 3.2 How each leak applies to acu

1. **Referer.** acu sends `Referrer-Policy: no-referrer` on its pages
   (`cli/src/site.rs`, `reply`). The viewer loads no third-party resource.
   So no `Referer` carries the token
   ([MDN Referrer-Policy](https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Headers/Referrer-Policy)).
2. **Server logs.** acu writes no access log. A proxy in front of acu (SSH
   gives none; a Codespaces forward may) could log the first request.
3. **History.** A 303 redirect to the address without the token keeps the
   token out of the address bar, out of bookmarks, and out of later copy and
   paste. It does not promise that the browser drops the first address from
   its history. This note found no browser document that promises it. The
   token dies when acu stops, unless `ACU_TOKEN` fixes it. A fixed token
   makes each such leak last.
4. **Cache.** acu sends `Cache-Control: no-store` on every response
   (`cli/src/site.rs`, `reply`). Send it on the 303 too.
5. **Terminal.** The printed line stays in scrollback and in any file or
   journal that standard error goes to. Jupyter prints its token the same
   way ([Jupyter Server security](https://jupyter-server.readthedocs.io/en/latest/operators/security.html)).

### 3.3 Best practice

1. Exchange the URL token once for a cookie, then redirect with 303 to the
   same path without the `token` parameter. Keep the other parameters.
   (This note's own design, as live-service.md 6.2.5 proposed; Jupyter
   skips the redirect, 1.4.1.)
2. Check `Host` before the exchange, so a page that DNS rebinding points at
   acu gets no cookie.
3. Build the `Location` from the path alone. Refuse a path that starts with
   `//` or holds `\`, because a browser reads those as another host.
4. Answer a wrong token with 401 and no cookie.

## 4. MCP clients and a static bearer token

### 4.1 How a person gives the header

| Client         | How                                                                                                                                                                                     | Source                                                                                              |
| -------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------- |
| Claude Code    | `claude mcp add --transport http <name> <url> --header "Authorization: Bearer <token>"`; or `.mcp.json` with `"type": "http"` and `"headers": {"Authorization": "Bearer ${ACU_TOKEN}"}` | [Claude Code MCP docs](https://code.claude.com/docs/en/mcp)                                         |
| VS Code        | `mcp.json` with `"type": "http"`, `"headers": {"Authorization": "Bearer ${input:acu-token}"}` and an `inputs` entry of type `promptString` with `"password": true`                      | [VS Code MCP configuration](https://code.visualstudio.com/docs/copilot/reference/mcp-configuration) |
| Cursor         | `mcp.json` with `"url"` and `"headers": {"Authorization": "Bearer ${env:ACU_TOKEN}"}`                                                                                                   | [Cursor MCP docs](https://cursor.com/docs/context/mcp)                                              |
| Claude Desktop | The config file takes local (stdio) servers. A remote server with a header goes through a bridge such as `npx mcp-remote <url> --header "Authorization: Bearer ..."`                    | [mcp-remote](https://github.com/geelen/mcp-remote)                                                  |

Notes:

1. Claude Code expands `${VAR}` in `headers`. It reads some credential
   variables as empty (`ANTHROPIC_API_KEY`, `NPM_TOKEN` and others). A name
   outside that set, such as `ACU_TOKEN`, "expands as written"
   ([Claude Code MCP docs](https://code.claude.com/docs/en/mcp)).
2. For Claude Desktop, `acu mcp` (standard input and output) needs no
   token. The MCP spec says that an stdio server "SHOULD NOT follow this
   specification, and instead retrieve credentials from the environment"
   ([MCP authorization, 2025-06-18](https://modelcontextprotocol.io/specification/2025-06-18/basic/authorization)).
   Point Desktop users to `acu mcp`, not to a bridge.

### 4.2 What the spec says on a missing token

1. "Authorization is OPTIONAL for MCP implementations"
   ([MCP authorization, 2025-06-18](https://modelcontextprotocol.io/specification/2025-06-18/basic/authorization)).
2. When a server takes part in MCP authorization, it acts as an OAuth
   resource server. It "MUST use the HTTP header `WWW-Authenticate` when
   returning a 401 Unauthorized to indicate the location of the resource
   server metadata URL". Clients "MUST be able to parse `WWW-Authenticate`
   headers and respond appropriately to HTTP 401". The error table gives 401
   for "Authorization required or token invalid" (same source).
3. RFC 6750 says a 401 for missing credentials "MUST include" a
   `WWW-Authenticate` header, and "SHOULD NOT include an error code" when
   the request held no credentials. Its example:
   `WWW-Authenticate: Bearer realm="example"`. A wrong token gets
   `error="invalid_token"` with 401
   ([RFC 6750 section 3](https://www.rfc-editor.org/rfc/rfc6750#section-3),
   [section 3.1](https://www.rfc-editor.org/rfc/rfc6750#section-3.1)).
4. acu offers no OAuth. So acu does not take part in MCP authorization, and
   the `resource_metadata` rule does not bind it. RFC 6750 still asks for
   `WWW-Authenticate` on a 401.

### 4.3 How clients react to a 401

1. Claude Code, with a configured `Authorization` header: "a 401 or 403
   while connecting doesn't flag the server". It "reports the connection as
   failed instead of falling back to OAuth"
   ([Claude Code MCP docs](https://code.claude.com/docs/en/mcp)).
2. Claude Code, without a configured header: it "marks a remote server as
   needing authentication when the server responds with 401 Unauthorized or
   403 Forbidden", and then offers an OAuth sign-in (same source).
3. Open bug reports say Claude Code sometimes ignores a static header and
   starts OAuth discovery
   ([claude-code#59467](https://github.com/anthropics/claude-code/issues/59467),
   [claude-code#80785](https://github.com/anthropics/claude-code/issues/80785)).
   This note did not check them.
4. A client that starts discovery without `resource_metadata` probes
   `/.well-known/oauth-protected-resource` and related paths. Zuplo advises a
   404 on those paths, so the client "falls back to the header you
   configured"
   ([Zuplo](https://zuplo.com/learn/mcp/errors/configured-headers-ignored-when-oauth-discovery-present)).
5. The VS Code and Cursor pages say nothing about a 401 without OAuth
   metadata (sources in 4.1). Cursor finds OAuth servers through RFC 9728
   metadata ([Cursor MCP docs](https://cursor.com/docs/context/mcp)).

### 4.4 The answer that works best for acu

1. Status 401 on a missing or wrong token on `/mcp`.
2. `WWW-Authenticate: Bearer realm="acu"` with no `resource_metadata`. Add
   `error="invalid_token"` only when the request held a bearer token.
3. A body that names the fix in plain text or JSON: the header to send and
   where acu printed the token. Clients show the HTTP status; some show the
   body.
4. A 404 on every `/.well-known/oauth-*` and `/.well-known/openid-configuration`
   path. acu's fallback already answers an unknown path with 404
   (`cli/src/site.rs`, `respond`). Keep it so, and keep those paths outside
   the token check, so a probe learns nothing and gets no 401.
5. Do not use 403 for a missing token. Claude Code treats 401 and 403 alike
   (4.3.2), and 403 already means "wrong Host" or "wrong Origin" in acu.

## 5. Host checks in other dev servers

### 5.1 Vite

1. `server.allowedHosts` defaults to `[]`. "localhost and domains under
   .localhost and all IP addresses are allowed by default." A leading dot
   admits the name and its subdomains. `true` admits any host and "allows
   any website to send requests to your dev server through DNS rebinding
   attacks." "When using HTTPS, this check is skipped"
   ([Vite server options](https://vite.dev/config/server-options)).
2. Vite also admits the `server.host` value and the host name of
   `server.origin` (`hostCheck.ts`, `getAdditionalAllowedHosts`).
3. Vite compares the host name only, without the port. It admits each IP
   literal, with the comment "DNS rebinding attacks does not happen with IP
   addresses". It admits a request with no `Host` at all
   (`isHostAllowed.ts`: `if (hostHeader === undefined) return true`).

### 5.2 webpack-dev-server

1. `allowedHosts` takes `"auto"` (the default), `"all"`, or a list. A leading
   dot works as in Vite (`Server.js`, `isHostAllowed`).
2. It compares the host name only: "we don't care about port not matching"
   (`Server.js`, `isValidHost`).
3. It refuses a missing `Host` (`if (!header) return false`). It refuses a
   `Host` that holds `/`, `\`, `?`, `#`, `@` or white space.
4. It admits each IPv4 and IPv6 literal, `localhost`, `*.localhost` and the
   `host` option (`Server.js`, `isValidHost`).

### 5.3 Jupyter and rmcp

1. Jupyter: no port, any loopback IP, the names in `local_hostnames`
   (1.6.2).
2. rmcp 3.10.0 compares the host name, and the port only when the allowed
   entry names one: `Some(port) => host.port == Some(port), None => true`.
   An empty list admits every host. A missing `Host` (and no `:authority`)
   gets 400 (`tower.rs`, `host_is_allowed` and `parse_host_header`).
3. Before this change, acu's `http_service` passed `localhost`,
   `127.0.0.1` and `::1` without a port (`cli/src/mcp/mod.rs`), so `/mcp`
   admitted those names on any port. acu now passes each name with the
   port, from the same list as the API (`Studio::hosts`).
   The studio's own check names the port (`cli/src/studio/mod.rs`,
   `hosts`).

### 5.4 With or without the port

1. DNS rebinding works through a name that the attacker controls. The port
   in `Host` adds nothing against it: a rebinding page uses the server's
   real port anyway. So Vite, webpack-dev-server and Jupyter drop the port
   (5.1, 5.2, 1.6.2).
2. A forward changes the port that the browser sees. With
   `ssh -L 8000:127.0.0.1:4321`, the browser sends `Host: localhost:8000`.
   VS Code also picks another local port when another process holds the
   first one ([VS Code port forwarding](https://code.visualstudio.com/docs/debugtest/port-forwarding)).
   A check that demands `:4321` fails there.
3. A browser leaves out the default port. A page at `https://name.example/`
   sends `Host: name.example`, with no `:443`
   ([RFC 9110 section 4.2.3](https://www.rfc-editor.org/rfc/rfc9110#section-4.2.3),
   [RFC 9110 section 7.2](https://www.rfc-editor.org/rfc/rfc9110#section-7.2)).
   A rule `name.example:443` must therefore also match `name.example`.

### 5.5 Missing Host and IP literals

1. HTTP/1.1 requires `Host`. A server "MUST respond with a 400 (Bad Request)
   status code to any HTTP/1.1 request message that lacks a Host header
   field" ([RFC 9112 section 3.2](https://www.rfc-editor.org/rfc/rfc9112#section-3.2)).
   Vite admits it, webpack-dev-server and rmcp refuse it (5.1 to 5.3). A
   browser always sends `Host`, so a refusal costs nothing.
2. An IP literal cannot come from DNS rebinding. Admitting
   `127.0.0.1:<any port>` or `[::1]:<any port>` stays safe for that threat.

### 5.6 GitHub Codespaces

1. A forwarded port shows at `https://CODESPACENAME-PORT.app.github.dev`.
   The forward speaks HTTP to the app by default. A private port needs the
   person's GitHub sign-in, or the header `X-Github-Token`
   ([GitHub Docs](https://docs.github.com/en/codespaces/developing-in-a-codespace/forwarding-ports-in-your-codespace)).
2. The domain changed from `preview.app.github.dev` in July 2023
   ([GitHub changelog](https://github.blog/changelog/2023-07-14-codespaces-port-forwarding-domain-name-updates)).
3. GitHub's pages do not say which `Host` the app receives. A Next.js
   report on Microsoft dev tunnels, the forward type that Codespaces uses,
   shows the tunnel name in `X-Forwarded-Host` beside a `localhost:3000`
   value. That suggests a forward may rewrite one header and not the other
   ([next.js#58019](https://github.com/vercel/next.js/issues/58019), seen
   through a search summary only; GitHub refused a direct read).
   This note could not test a codespace. Treat the exact `Host` as
   unverified. acu's refusal should name the `Host` it received, so a
   person can copy it into `--allow-host`.
4. The browser's `Origin` there reads `https://<name>-<port>.app.github.dev`,
   with the https scheme and no port. A check that accepts only
   `http://<allowed host>` refuses it.
5. VS Code desktop also forwards a private port to a local port such as
   `127.0.0.1:4000` (same GitHub source). That route needs no
   `--allow-host` when the local port equals acu's port.

## 6. Other points for a security review

### 6.1 The token on `/mcp`

1. rmcp 3.10.0 has no server-side authentication. Its `auth` feature serves
   clients only (`rmcp` 3.10.0 `Cargo.toml` features and
   `src/transport/auth.rs`). So acu must check the token in a layer in
   front of the rmcp service, before rmcp reads the body.
2. rmcp checks `Host` and `Origin` first (`tower.rs`,
   `validate_dns_rebinding_headers` in `handle`). acu's layer runs before
   that. Keep the order Host, then token: a foreign `Host` should get 403
   before acu says anything about the token.
3. The MCP spec says clients "MUST use the Authorization request header"
   and send it "in every HTTP request"
   ([MCP authorization, 2025-06-18](https://modelcontextprotocol.io/specification/2025-06-18/basic/authorization)).
   No MCP client needs the cookie. A cookie on `/mcp` only adds a
   browser route, and `/mcp` lacks the Fetch Metadata and custom-header
   guards of `/api/` (`docs/studio.md`, guard 5). Accept only the bearer
   header on `/mcp`.

### 6.2 Constant-time comparison

1. Compare all bytes with XOR and OR, and return after the loop. Python's
   `hmac.compare_digest` documents the same aim: it "uses an approach
   designed to prevent timing analysis by avoiding content-based short
   circuiting behaviour"
   ([Python hmac](https://docs.python.org/3/library/hmac.html#hmac.compare_digest)).
2. A length check that returns early leaks the length only. A random token
   has a public length (64 hex digits). A token from `ACU_TOKEN` leaks its
   length; that stays acceptable.
3. The compiler may still shorten a loop. The `subtle` crate calls its own
   guards "best-effort" for that reason
   ([subtle docs](https://docs.rs/subtle/latest/subtle/)). acu has no
   `subtle` in its tree. Adding it would need the maintainer's agreement
   (AGENTS.md, "Ask first" 3). `std::hint::black_box` on the result gives a
   similar best effort.

### 6.3 Brute force and rate limits

1. A random token of 32 bytes gives 2^256 values. No rate of guesses over
   loopback reaches it. Jupyter has no rate limit either (no such code in
   `identity.py` or `login.py`).
2. A person's `ACU_TOKEN` may hold less entropy. A length floor helps
   little against a chosen word. Print a line that says a random token
   needs no variable, and that `ACU_TOKEN` suits MCP client configs only.

### 6.4 Where the token lives

1. **Command line.** Never accept the token as a flag. Other users can read
   a process's arguments (`ps`). Jupyter's redirect file exists for the same
   reason (1.6.1).
2. **Environment.** On Linux, `/proc/<pid>/environ` stays readable by the
   owner only
   ([proc_pid_environ(5)](https://man7.org/linux/man-pages/man5/proc_pid_environ.5.html)).
   But child processes inherit the environment. acu starts git, the
   adapters, and through them rustdoc, rust-analyzer, build scripts and
   proc macros of the user's project. Those run the project's code
   (`docs/research/notes/live-service.md`, 3.1). Remove `ACU_TOKEN` from
   every child's environment. The project's code runs as the same user and
   could still do harm, but it should not get a ready key to the studio.
3. **Standard error.** acu prints the token there, as Jupyter does (3.2.5).
   Say in the docs that a redirected or logged standard error holds the
   token.
4. **Debug output.** Keep the token out of `Debug` and error text.

### 6.5 Port squatting and `localhost`

1. acu binds `127.0.0.1` only. Another process can bind `[::1]:4321` at
   the same time. A browser or client that resolves `localhost` may try
   `::1` first and reach that process, with acu's cookie or bearer token.
   So print `http://127.0.0.1:<port>/`, never `localhost`, and use
   `127.0.0.1` in every example client config.
2. When acu does not run, any process can listen on its port. An MCP client
   with a fixed `ACU_TOKEN` in its config then sends the token to that
   process. A random token per start avoids this for the browser. For MCP
   configs, the docs should state the risk.
3. Windows: a server that binds a specific address with no socket options
   stops another socket on the same specific address, but a second socket
   may bind the wildcard address and take "all interfaces except the
   specific address". Microsoft advises `SO_EXCLUSIVEADDRUSE` for every
   server
   ([Microsoft Learn](https://learn.microsoft.com/en-us/windows/win32/winsock/using-so-reuseaddr-and-so-exclusiveaddruse)).
   acu's 127.0.0.1 bind keeps its own address; point 6.5.1 still applies.

### 6.6 The cookie value

1. Jupyter stores a signed user model, not the token (1.4.2). acu may store
   the token itself. A cookie thief (2.3.1) then also holds the bearer
   token for `/mcp`. With 6.1.3 (bearer only on `/mcp`), a derived cookie
   value, such as an HMAC of the token with a fixed label, keeps a stolen
   cookie from opening `/mcp`. It still opens `/api/`.
2. Accept the request when any `Cookie` pair with acu's name matches. A
   browser can send two cookies of one name (another path, or one that a
   hostile port wrote, 2.3.2).
3. Leave out `Max-Age` and `Expires`, so the cookie ends with the browser
   session. A restart of acu with a new token makes the old cookie fail; the
   viewer should then say "open the address that acu printed".

### 6.7 SameSite=Strict and the first visit

1. A link to the printed address from another site (a web terminal, a chat)
   counts as a cross-site navigation. The 303 response still stores the
   cookie. Whether the browser sends a Strict cookie on the redirected
   request depends on the redirect chain
   ([MDN Set-Cookie](https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Headers/Set-Cookie#samesitesamesite-value)).
2. The viewer's page itself needs no token in `acu serve`. Its later
   fetches to `/api/` and `/data/atlas.json` go from the page's own origin,
   so they carry the cookie. So the first visit works either way.
3. acu chose `SameSite=Lax` in the end, so the first visit from a link
   keeps the cookie after the 303. `Lax` sends a cookie with a top-level
   GET that another site starts
   ([MDN Set-Cookie](https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Headers/Set-Cookie#samesitesamesite-value)).
   The cost: every GET call of the API must change nothing. Writes from
   another site still meet the `Origin`, Fetch Metadata and custom-header
   guards ([`docs/studio.md`](../../studio.md#decisions-on-the-token-and-the-host)).

### 6.8 Order of checks

1. Host, then Origin, then token, then the changing-call guards, then the
   body. A foreign `Host` or `Origin` gets 403 and learns nothing about the
   token. acu applies this order to `/api/` and to `/mcp`.
2. Exchange `?token=` only after the Host check (3.3.2).
3. Never answer `OPTIONS` with `Access-Control-Allow-*`, with or without a
   token (contrast 1.5.3).
