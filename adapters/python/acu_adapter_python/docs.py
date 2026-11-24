"""Turns Python docstrings into the data format's normal form.

The parser detects the docstring style (Sphinx fields, NumPy sections or
Google sections) and fills the doc's fields from it. The body text holds
reStructuredText, which becomes blocks and the inline subset of
docs/schema.md section 8.3.
"""

from __future__ import annotations

import re
from typing import Callable, Optional

#: Resolves a name that a docstring mentions to a symbol or container ID.
Resolver = Callable[[str], Optional[str]]

_GOOGLE_SECTIONS = {
    "args": "params",
    "arguments": "params",
    "parameters": "params",
    "params": "params",
    "keyword args": "params",
    "keyword arguments": "params",
    "other parameters": "params",
    "returns": "returns",
    "return": "returns",
    "yields": "returns",
    "yield": "returns",
    "raises": "throws",
    "raise": "throws",
    "exceptions": "throws",
    "examples": "examples",
    "example": "examples",
    "see also": "seeAlso",
    "note": "heading",
    "notes": "heading",
    "warning": "heading",
    "warnings": "heading",
    "attributes": "heading",
    "methods": "heading",
    "todo": "heading",
    "references": "heading",
    "warns": "heading",
}

_GOOGLE_HEADER = re.compile(r"^(?P<name>[A-Z][A-Za-z ]*?):\s*$")
_NUMPY_UNDERLINE = re.compile(r"^-{3,}\s*$")
_SPHINX_FIELD = re.compile(r"^:(?P<field>[A-Za-z]+)(?:\s+(?P<arg>[^:]+?))?:\s*(?P<text>.*)$")
_SPHINX_FIELDS = {
    "param",
    "parameter",
    "arg",
    "argument",
    "key",
    "keyword",
    "type",
    "returns",
    "return",
    "rtype",
    "raises",
    "raise",
    "except",
    "exception",
    "yields",
    "yield",
    "ytype",
    "var",
    "ivar",
    "cvar",
    "vartype",
    "meta",
}
_DIRECTIVE = re.compile(r"^\.\.\s+(?P<name>[A-Za-z][\w:-]*)::\s*(?P<arg>.*)$")
_BULLET = re.compile(r"^(?P<marker>[-*+]|\d+[.)]|#\.)\s+(?P<text>.*)$")
_ADORNMENT = re.compile(r"^([=\-~^\"'`#*+<>:.])\1{2,}\s*$")
_ROLE = re.compile(
    r":(?:py:)?(?P<role>[a-z]+):`(?P<target>[^`]+)`"
    r"|``(?P<literal>.+?)``"
    r"|`(?P<label>[^`<]+?)\s*<(?P<url>[^`>]+)>`__?"
    r"|`(?P<named>[^`]+)`__?"
    r"|`(?P<default>[^`]+)`"
    r"|\*\*(?P<strong>[^*]+)\*\*"
    r"|\*(?P<emph>[^*\s][^*]*)\*"
)
_SYMBOL_ROLES = {"class", "func", "meth", "attr", "exc", "mod", "data", "obj", "const", "type", "deco", "function", "method"}
_ADMONITIONS = {
    "note",
    "warning",
    "tip",
    "important",
    "attention",
    "caution",
    "danger",
    "hint",
    "error",
    "admonition",
    "seealso",
}


def escape(text: str) -> str:
    """Escapes the characters that the inline subset treats as markup."""
    return re.sub(r"([\\`*\[\]])", r"\\\1", text)


def code_span(text: str) -> str:
    """Writes text as one code span of the inline subset."""
    return "`" + text.replace("\\", "\\\\").replace("`", "\\`") + "`"


def inline(text: str, resolve: Resolver) -> str:
    """Turns one stretch of reStructuredText into the inline subset.

    Sphinx roles and single-backtick names become ``sym:`` links when
    ``resolve`` finds the name they mention, and code spans otherwise.
    """
    text = re.sub(r"\s+", " ", text).strip()
    out: list[str] = []
    pos = 0
    for m in _ROLE.finditer(text):
        out.append(_plain(text[pos : m.start()]))
        pos = m.end()
        if m.group("role"):
            out.append(_role(m.group("role"), m.group("target"), resolve))
        elif m.group("literal") is not None:
            out.append(code_span(m.group("literal")))
        elif m.group("url"):
            out.append("[" + escape(m.group("label").strip()) + "](" + m.group("url").strip() + ")")
        elif m.group("named") is not None and m.group(0).endswith("_"):
            out.append(escape(m.group("named")))
        elif m.group("default") is not None or m.group("named") is not None:
            out.append(_role("obj", m.group("default") or m.group("named"), resolve))
        elif m.group("strong"):
            out.append("**" + escape(m.group("strong")) + "**")
        elif m.group("emph"):
            out.append("*" + escape(m.group("emph")) + "*")
    out.append(_plain(text[pos:]))
    return "".join(out)


def _plain(text: str) -> str:
    """Escapes plain text and drops reStructuredText's backslash escapes."""
    text = re.sub(r"\\(.)", r"\1", text)
    return escape(text)


def _role(role: str, target: str, resolve: Resolver) -> str:
    """Writes one Sphinx role as a symbol link, a code span or plain text."""
    target = target.strip()
    label = None
    m = re.match(r"^(?P<label>.+?)\s*<(?P<target>[^>]+)>$", target)
    if m:
        label, target = m.group("label"), m.group("target")
    short = target.startswith("~")
    target = target.lstrip("~!.")
    if label is None:
        label = target.rsplit(".", 1)[-1] if short else target
    if role not in _SYMBOL_ROLES:
        return escape(label)
    found = resolve(target.removesuffix("()"))
    if found:
        return "[" + escape(label) + "](sym:" + found + ")"
    return code_span(label)


def summary_of(text: str) -> str:
    """Returns the first sentence of a paragraph of reStructuredText."""
    text = re.sub(r"\s+", " ", text).strip()
    for m in re.finditer(r"[.!?](?=\s|$)", text):
        before = text[: m.start()]
        if re.search(r"(\be\.g|\bi\.e|\betc|\bvs|\bcf|\bNo)$", before, re.I):
            continue
        if before.count("`") % 2:
            continue
        return text[: m.end()]
    return text


def _indent(line: str) -> int:
    """Counts the spaces in front of a line."""
    return len(line) - len(line.lstrip(" "))


def _dedent(lines: list[str]) -> list[str]:
    """Removes the indent that every non-blank line shares."""
    widths = [_indent(x) for x in lines if x.strip()]
    cut = min(widths) if widths else 0
    return [x[cut:] if x.strip() else "" for x in lines]


def _trim(lines: list[str]) -> list[str]:
    """Drops blank lines at both ends."""
    while lines and not lines[0].strip():
        lines = lines[1:]
    while lines and not lines[-1].strip():
        lines = lines[:-1]
    return lines


def detect_format(lines: list[str]) -> str:
    """Names the docstring style that the lines follow."""
    for line in lines:
        m = _SPHINX_FIELD.match(line.strip())
        if m and m.group("field") in _SPHINX_FIELDS:
            return "docstring-sphinx"
    for i in range(len(lines) - 1):
        name = lines[i].strip().lower()
        if name in _GOOGLE_SECTIONS and _NUMPY_UNDERLINE.match(lines[i + 1].strip()):
            return "docstring-numpy"
    for i, line in enumerate(lines):
        m = _GOOGLE_HEADER.match(line)
        if m and m.group("name").lower() in _GOOGLE_SECTIONS and _indent(line) == 0:
            if i + 1 < len(lines) and _indent(lines[i + 1]) > 0 and lines[i + 1].strip():
                return "docstring-google"
    return "plain"


class _Doc:
    """Collects the fields of one doc while the parser reads it."""

    def __init__(self, raw: str, fmt: str, resolve: Resolver) -> None:
        """Starts an empty doc for the text ``raw`` in the style ``fmt``."""
        self.raw = raw
        self.format = fmt
        self.resolve = resolve
        self.blocks: list[dict] = []
        self.params: dict[str, str] = {}
        self.returns: list[str] = []
        self.throws: list[dict] = []
        self.examples: list[dict] = []
        self.see_also: list[str] = []
        self.deprecated: Optional[str] = None
        self.since: Optional[str] = None
        self.tags: list[dict] = []
        self.heading_chars: list[str] = []

    def text(self, s: str) -> str:
        """Turns text into the inline subset with this doc's resolver."""
        return inline(s, self.resolve)

    def result(self) -> dict:
        """Returns the doc in the data format's shape."""
        first = next((b for b in self.blocks if b["type"] == "paragraph"), None)
        summary = ""
        if first is not None:
            summary = self.text(summary_of(first["_raw"]))
        blocks = [{k: v for k, v in b.items() if not k.startswith("_")} for b in self.blocks]
        doc: dict = {"raw": self.raw, "format": self.format, "summary": summary, "description": blocks}
        if self.params:
            doc["params"] = dict(sorted(self.params.items()))
        if self.returns:
            doc["returns"] = " ".join(self.returns)
        if self.throws:
            doc["throws"] = self.throws
        if self.examples:
            doc["examples"] = self.examples
        if self.see_also:
            doc["seeAlso"] = self.see_also
        if self.deprecated is not None:
            doc["deprecated"] = self.deprecated
        if self.since is not None:
            doc["since"] = self.since
        if self.tags:
            doc["tags"] = self.tags
        return doc


def parse(raw: str, resolve: Resolver) -> dict:
    """Parses a cleaned docstring into the data format's doc object."""
    lines = raw.split("\n")
    fmt = detect_format(lines)
    doc = _Doc(raw, fmt, resolve)
    if fmt == "docstring-sphinx":
        body = _sphinx_fields(lines, doc)
    elif fmt == "docstring-numpy":
        body = _numpy_sections(lines, doc)
    elif fmt == "docstring-google":
        body = _google_sections(lines, doc)
    else:
        body = lines
    _blocks(body, doc)
    return doc.result()


def _sphinx_fields(lines: list[str], doc: _Doc) -> list[str]:
    """Reads Sphinx field lists into the doc and returns the other lines."""
    body: list[str] = []
    i = 0
    while i < len(lines):
        line = lines[i]
        m = _SPHINX_FIELD.match(line.strip()) if _indent(line) == 0 else None
        if not m or m.group("field") not in _SPHINX_FIELDS:
            body.append(line)
            i += 1
            continue
        text = [m.group("text")]
        i += 1
        while i < len(lines) and lines[i].strip() and _indent(lines[i]) > 0:
            text.append(lines[i].strip())
            i += 1
        field, arg, joined = m.group("field"), (m.group("arg") or "").strip(), " ".join(text)
        if field in {"param", "parameter", "arg", "argument", "key", "keyword"}:
            name = arg.split()[-1] if arg else ""
            if name:
                doc.params[name.lstrip("*")] = doc.text(joined)
        elif field in {"returns", "return", "yields", "yield"}:
            doc.returns.append(doc.text(joined))
        elif field in {"raises", "raise", "except", "exception"}:
            entry = {"text": doc.text(joined)}
            if arg:
                entry = {"type": arg, "text": doc.text(joined)}
            doc.throws.append(entry)
        elif field in {"var", "ivar", "cvar"}:
            doc.tags.append({"name": field, "text": doc.text((arg + ": " if arg else "") + joined)})
    return body


def _section_body(lines: list[str], start: int) -> tuple[list[str], int]:
    """Collects the indented lines under a Google section header."""
    out: list[str] = []
    i = start
    while i < len(lines) and (not lines[i].strip() or _indent(lines[i]) > 0):
        out.append(lines[i])
        i += 1
    return _trim(_dedent(out)), i


def _entries(lines: list[str]) -> list[tuple[str, list[str]]]:
    """Splits a section into entries: a head line and its indented text."""
    out: list[tuple[str, list[str]]] = []
    for line in lines:
        if not line.strip():
            if out:
                out[-1][1].append("")
            continue
        if _indent(line) == 0:
            out.append((line.strip(), []))
        elif out:
            out[-1][1].append(line.strip())
    return [(head, _trim(rest)) for head, rest in out]


def _google_sections(lines: list[str], doc: _Doc) -> list[str]:
    """Reads Google sections into the doc and returns the other lines."""
    body: list[str] = []
    i = 0
    while i < len(lines):
        m = _GOOGLE_HEADER.match(lines[i])
        name = m.group("name").lower() if m else ""
        if not m or name not in _GOOGLE_SECTIONS or _indent(lines[i]) != 0:
            body.append(lines[i])
            i += 1
            continue
        content, i = _section_body(lines, i + 1)
        _section(m.group("name"), _GOOGLE_SECTIONS[name], content, doc, body, google=True)
    return body


def _numpy_sections(lines: list[str], doc: _Doc) -> list[str]:
    """Reads NumPy sections into the doc and returns the other lines."""
    body: list[str] = []
    i = 0
    while i < len(lines):
        name = lines[i].strip()
        underlined = i + 1 < len(lines) and _NUMPY_UNDERLINE.match(lines[i + 1].strip())
        if not underlined or name.lower() not in _GOOGLE_SECTIONS:
            body.append(lines[i])
            i += 1
            continue
        j = i + 2
        content: list[str] = []
        while j < len(lines):
            nxt = lines[j].strip().lower()
            if nxt in _GOOGLE_SECTIONS and j + 1 < len(lines) and _NUMPY_UNDERLINE.match(lines[j + 1].strip()):
                break
            content.append(lines[j])
            j += 1
        i = j
        _section(name, _GOOGLE_SECTIONS[name.lower()], _trim(content), doc, body, google=False)
    return body


def _section(title: str, field: str, content: list[str], doc: _Doc, body: list[str], google: bool) -> None:
    """Puts one Google or NumPy section into its field of the doc."""
    if field == "params":
        for head, rest in _entries(content):
            if google:
                m = re.match(r"^(?P<name>\*{0,2}[\w.]+)\s*(?:\((?P<type>[^)]*)\))?\s*:\s*(?P<text>.*)$", head)
                if not m:
                    continue
                name, text = m.group("name"), [m.group("text")] + rest
            else:
                name, text = head.split(":", 1)[0].strip(), rest
            for one in name.split(","):
                if one.strip():
                    doc.params[one.strip().lstrip("*")] = doc.text(" ".join(text))
    elif field == "returns":
        if google:
            text = " ".join(x.strip() for x in content)
            m = re.match(r"^(?P<type>[^:\s][^:]*?):\s+(?P<text>.+)$", text)
            if m and len(m.group("type").split()) <= 3:
                text = m.group("text")
            doc.returns.append(doc.text(text))
        else:
            for head, rest in _entries(content):
                doc.returns.append(doc.text(" ".join(rest) if rest else head))
    elif field == "throws":
        for head, rest in _entries(content):
            if google:
                m = re.match(r"^(?P<type>[\w.]+)\s*:\s*(?P<text>.*)$", head)
                if m:
                    doc.throws.append({"type": m.group("type"), "text": doc.text(" ".join([m.group("text")] + rest))})
                else:
                    doc.throws.append({"text": doc.text(" ".join([head] + rest))})
            else:
                doc.throws.append({"type": head, "text": doc.text(" ".join(rest))})
    elif field == "examples":
        code = "\n".join(content).strip("\n")
        if code:
            doc.examples.append({"code": code, "language": "python"})
    elif field == "seeAlso":
        for head, rest in _entries(content):
            names, _, text = head.partition(":")
            for name in names.split(","):
                if name.strip():
                    tail = " ".join([text.strip()] + rest).strip()
                    entry = _role("obj", name.strip(), doc.resolve)
                    doc.see_also.append(entry + (" " + doc.text(tail) if tail else ""))
    else:
        body.extend(["", title, "=" * len(title), ""] + content + [""])


def _blocks(lines: list[str], doc: _Doc) -> None:
    """Turns reStructuredText body lines into blocks on the doc."""
    lines = _trim(_dedent(lines))
    i = 0
    while i < len(lines):
        line = lines[i]
        stripped = line.strip()
        if not stripped:
            i += 1
            continue
        if stripped.startswith("```"):
            lang = stripped[3:].strip() or "python"
            j = i + 1
            while j < len(lines) and not lines[j].strip().startswith("```"):
                j += 1
            doc.blocks.append({"type": "code", "code": "\n".join(_dedent(lines[i + 1 : j])), "language": lang})
            i = j + 1
            continue
        d = _DIRECTIVE.match(stripped)
        if d:
            j = i + 1
            content = []
            while j < len(lines) and (not lines[j].strip() or _indent(lines[j]) > _indent(line)):
                content.append(lines[j])
                j += 1
            _directive(d.group("name").lower(), d.group("arg").strip(), _trim(_dedent(content)), doc)
            i = j
            continue
        if stripped.startswith(".."):
            j = i + 1
            while j < len(lines) and (not lines[j].strip() or _indent(lines[j]) > _indent(line)):
                j += 1
            i = j
            continue
        if stripped.startswith(">>>"):
            j = i
            while j < len(lines) and lines[j].strip():
                j += 1
            doc.blocks.append({"type": "code", "code": "\n".join(_dedent(lines[i:j])), "language": "pycon"})
            i = j
            continue
        if i + 1 < len(lines) and _ADORNMENT.match(lines[i + 1].strip()) and len(lines[i + 1].strip()) >= len(stripped) - 1:
            ch = lines[i + 1].strip()[0]
            if ch not in doc.heading_chars:
                doc.heading_chars.append(ch)
            level = min(6, 2 + doc.heading_chars.index(ch))
            doc.blocks.append({"type": "heading", "level": level, "text": doc.text(stripped)})
            i += 2
            continue
        if _BULLET.match(stripped) and _indent(line) == 0:
            i = _list(lines, i, doc)
            continue
        if _indent(line) > 0:
            j = i
            while j < len(lines) and (not lines[j].strip() or _indent(lines[j]) > 0):
                j += 1
            doc.blocks.append({"type": "quote", "text": doc.text(" ".join(x.strip() for x in lines[i:j]))})
            i = j
            continue
        j = i
        para: list[str] = []
        while j < len(lines) and lines[j].strip() and _indent(lines[j]) == 0:
            if j > i and (_DIRECTIVE.match(lines[j].strip()) or lines[j].strip().startswith(">>>")):
                break
            para.append(lines[j].strip())
            j += 1
        text = " ".join(para)
        literal = text.endswith("::")
        if literal:
            text = text[:-2].rstrip() + (":" if not text[:-2].endswith(" ") and text[:-2].strip() else "")
        if text.strip():
            doc.blocks.append({"type": "paragraph", "text": doc.text(text), "_raw": text})
        i = j
        if literal:
            while i < len(lines) and not lines[i].strip():
                i += 1
            k = i
            while k < len(lines) and (not lines[k].strip() or _indent(lines[k]) > 0):
                k += 1
            code = _trim(_dedent(lines[i:k]))
            if code:
                doc.blocks.append({"type": "code", "code": "\n".join(code), "language": "python"})
            i = k


def _list(lines: list[str], i: int, doc: _Doc) -> int:
    """Reads one bullet or numbered list from line ``i`` and returns the next line."""
    first = _BULLET.match(lines[i].strip())
    assert first is not None
    ordered = not first.group("marker") in {"-", "*", "+"}
    items: list[list[str]] = []
    while i < len(lines):
        line = lines[i]
        m = _BULLET.match(line.strip()) if _indent(line) == 0 else None
        if m and (m.group("marker") in {"-", "*", "+"}) != ordered:
            items.append([m.group("text")])
        elif line.strip() and _indent(line) > 0 and items:
            items[-1].append(line.strip())
        elif not line.strip():
            nxt = lines[i + 1] if i + 1 < len(lines) else ""
            if not (nxt.strip() and (_indent(nxt) > 0 or _BULLET.match(nxt.strip()))):
                break
        else:
            break
        i += 1
    doc.blocks.append({"type": "list", "ordered": ordered, "items": [doc.text(" ".join(x)) for x in items]})
    return i


def _directive(name: str, arg: str, content: list[str], doc: _Doc) -> None:
    """Turns one reStructuredText directive into blocks or doc fields."""
    text = " ".join(x.strip() for x in content if x.strip())
    if name in {"code-block", "code", "sourcecode", "testcode", "doctest"}:
        code = [x for x in content if not re.match(r"^:[\w-]+:", x.strip())]
        doc.blocks.append({"type": "code", "code": "\n".join(_trim(code)), "language": arg or "python"})
    elif name == "deprecated":
        doc.deprecated = doc.text(" ".join(x for x in [arg, text] if x))
    elif name == "versionadded":
        doc.since = arg
    elif name == "versionchanged":
        doc.tags.append({"name": "versionchanged", "text": doc.text(" ".join(x for x in [arg, text] if x))})
    elif name == "seealso":
        doc.see_also.append(doc.text(" ".join(x for x in [arg, text] if x)))
    elif name in _ADMONITIONS:
        title = arg if name == "admonition" else name.capitalize()
        doc.blocks.append({"type": "quote", "text": doc.text(title + ": " + (arg + " " if name != "admonition" and arg else "") + text)})
    elif text:
        doc.blocks.append({"type": "quote", "text": doc.text(text)})
