"""Runs the Python adapter: protocol 1 on standard input, or flags by hand."""

from __future__ import annotations

import argparse
import datetime
import json
import sys

from .extract import LIMITS, NAME, extract
from .plugins import KNOWN, UnknownPlugin

#: The text that ``-help`` prints after the flags.
EPILOG = "Plugins: " + ", ".join(KNOWN) + ".\n\nLimits:\n" + "\n".join(f"  {i}. {x}" for i, x in enumerate(LIMITS, 1))


def main(argv: list[str] | None = None) -> int:
    """Reads a request, runs the adapter and writes the document to standard output."""
    parser = argparse.ArgumentParser(
        prog=NAME,
        description="Reads Python code and writes an Atlas Carries Us document. Without --root, it reads a protocol-1 request from standard input.",
        epilog=EPILOG,
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    parser.add_argument("--root", help="the folder to read")
    parser.add_argument("--repo-root", help="the repository root; paths in the output start there (default: --root)")
    parser.add_argument("--include", action="append", default=[], help="a glob of files to read (repeatable)")
    parser.add_argument("--exclude", action="append", default=[], help="a glob of files to leave out (repeatable)")
    parser.add_argument("--tests", action="append", default=[], help="a glob of test files (repeatable)")
    parser.add_argument("--plugin", action="append", default=[], help="an entry-point plugin to run (repeatable)")
    parser.add_argument("--out", help="write the document to this file instead of standard output")
    args = parser.parse_args(argv)
    if args.root:
        request = {
            "protocol": 1,
            "root": args.root,
            "repoRoot": args.repo_root or args.root,
            "include": args.include,
            "exclude": args.exclude,
            "tests": args.tests,
            "entryPoints": [{"plugin": p} for p in args.plugin],
        }
    else:
        try:
            request = json.load(sys.stdin)
        except json.JSONDecodeError as err:
            print(f"{NAME}: standard input holds no JSON request ({err}); pipe a protocol-1 request, or pass --root.", file=sys.stderr)
            return 2
    if request.get("protocol") != 1:
        print(f"{NAME}: the request speaks protocol {request.get('protocol')!r}; this adapter speaks protocol 1 only.", file=sys.stderr)
        return 2
    try:
        doc = extract(request)
    except UnknownPlugin as err:
        print(f"{NAME}: {err}.", file=sys.stderr)
        return 2
    doc["meta"]["generated"] = datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
    text = json.dumps(doc, ensure_ascii=False, indent=2) + "\n"
    if args.out:
        with open(args.out, "w", encoding="utf-8") as f:
            f.write(text)
    else:
        sys.stdout.write(text)
    return 0


#: Runs the adapter when Python runs this module.
if __name__ == "__main__":
    sys.exit(main())
