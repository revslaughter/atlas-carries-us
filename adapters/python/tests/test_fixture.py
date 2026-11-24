"""Runs the adapter on the invented fixture: golden file, schema and named facts.

Rewrite the golden file after a deliberate change with
``UPDATE_GOLDEN=1 python3 -m unittest discover -s adapters/python/tests``,
then read the diff.
"""

from __future__ import annotations

import json
import os
import subprocess
import sys
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
ADAPTER = os.path.dirname(HERE)
sys.path.insert(0, ADAPTER)
sys.path.insert(0, HERE)

from acu_adapter_python.extract import extract  # noqa: E402
from schema_check import validate  # noqa: E402

FIXTURE = os.path.join(ADAPTER, "testdata", "fixture")
GOLDEN = os.path.join(ADAPTER, "testdata", "golden", "fixture.json")
SCHEMA = os.path.join(ADAPTER, "..", "..", "schema", "atlas.schema.json")
PLUGINS = ["click", "django", "fastapi", "flask", "python-main", "typer"]


def run_fixture() -> dict:
    """Maps the fixture with every plugin, as acu would ask."""
    request = {
        "protocol": 1,
        "repoRoot": FIXTURE,
        "root": FIXTURE,
        "include": [],
        "exclude": [],
        "tests": [],
        "options": {},
        "entryPoints": [{"plugin": p} for p in PLUGINS],
    }
    return extract(request)


DOC = run_fixture()


def text(doc: dict) -> str:
    """Writes a document as the golden file holds it."""
    return json.dumps(doc, ensure_ascii=False, indent=2) + "\n"


class Golden(unittest.TestCase):
    """Compares the output with the golden file, byte for byte."""

    def test_output_matches_the_golden_file(self) -> None:
        """The fixture's map equals testdata/golden/fixture.json."""
        got = text(DOC)
        if os.environ.get("UPDATE_GOLDEN"):
            with open(GOLDEN, "w", encoding="utf-8") as f:
                f.write(got)
        with open(GOLDEN, encoding="utf-8") as f:
            want = f.read()
        self.assertEqual(want, got, "the output differs from the golden file; run with UPDATE_GOLDEN=1 after a deliberate change")

    def test_two_runs_give_the_same_bytes(self) -> None:
        """The output stays the same across runs."""
        self.assertEqual(text(DOC), text(run_fixture()))


class Schema(unittest.TestCase):
    """Validates the output against schema/atlas.schema.json."""

    def test_output_follows_the_schema(self) -> None:
        """Every field has the shape that the schema asks for."""
        with open(SCHEMA, encoding="utf-8") as f:
            schema = json.load(f)
        doc = dict(DOC)
        doc["meta"] = dict(doc["meta"], generated="2000-01-01T00:00:00Z")
        self.assertEqual([], validate(doc, schema, schema))

    def test_ids_differ(self) -> None:
        """No two containers, symbols or entry points share an ID."""
        ids = [c["id"] for c in DOC["containers"]] + [s["id"] for s in DOC["symbols"]] + [e["id"] for e in DOC.get("entryPoints", [])]
        self.assertEqual(len(ids), len(set(ids)))

    def test_edges_point_at_known_ids(self) -> None:
        """Every edge, test and handler names a container or symbol of the map."""
        known = {c["id"] for c in DOC["containers"]} | {s["id"] for s in DOC["symbols"]}
        for e in DOC["edges"]:
            self.assertIn(e["from"], known)
            self.assertIn(e["to"], known)
        for e in DOC["entryPoints"]:
            self.assertIn(e["handler"], known)
        for key in DOC["tests"]:
            self.assertIn(key, known)


def has_edge(kind: str, frm: str, to: str, confidence: str = "exact") -> bool:
    """Reports whether the fixture's map holds one edge."""
    return any(e["kind"] == kind and e["from"] == frm and e["to"] == to and e["confidence"] == confidence for e in DOC["edges"])


def entry(name: str) -> dict:
    """Returns the entry point with this name."""
    found = [e for e in DOC["entryPoints"] if e["name"] == name]
    assert len(found) == 1, f"no single entry point named {name!r}; found {[e['name'] for e in DOC['entryPoints']]}"
    return found[0]


def symbol(sym_id: str) -> dict:
    """Returns the symbol with this ID."""
    return next(s for s in DOC["symbols"] if s["id"] == sym_id)


L = "py:lending."


class Facts(unittest.TestCase):
    """Named facts that must hold; each failure reads as a sentence."""

    def test_borrow_calls_find_through_the_store_contract(self) -> None:
        """Library.borrow calls Store.find, typed by the annotation of self.store."""
        self.assertTrue(has_edge("calls", L + "library#Library.borrow", L + "store.base#Store.find"))

    def test_memory_store_overrides_find(self) -> None:
        """MemoryStore.find overrides Store.find."""
        self.assertTrue(has_edge("overrides", L + "store.memory#MemoryStore.find", L + "store.base#Store.find"))

    def test_memory_store_implements_the_protocol(self) -> None:
        """MemoryStore implements the protocol Readable."""
        self.assertTrue(has_edge("implements", L + "store.memory#MemoryStore", L + "store.base#Readable"))

    def test_super_reaches_the_base_method(self) -> None:
        """BranchLibrary.borrow calls Library.borrow through super()."""
        self.assertTrue(has_edge("calls", L + "library#BranchLibrary.borrow", L + "library#Library.borrow"))

    def test_a_constructor_call_points_at_init(self) -> None:
        """open_library calls Library.__init__ when it builds a Library."""
        self.assertTrue(has_edge("calls", "py:lending#open_library", L + "library#Library.__init__"))

    def test_an_untyped_receiver_gives_a_heuristic_call(self) -> None:
        """renew calls MemoryStore.renewal by its unique name, as a heuristic."""
        self.assertTrue(has_edge("calls", L + "library#renew", L + "store.memory#MemoryStore.renewal", "heuristic"))

    def test_a_decorator_counts_as_a_call(self) -> None:
        """Library.borrow calls its decorator audited."""
        self.assertTrue(has_edge("calls", L + "library#Library.borrow", L + "library#audited"))

    def test_a_module_name_bound_to_two_classes_gives_no_exact_call(self) -> None:
        """deliver calls carrier.run, but carrier holds a Courier or a Drone, so no exact edge points at Courier.run."""
        self.assertFalse(has_edge("calls", L + "carriers#deliver", L + "carriers#Courier.run"))

    def test_a_global_write_unsettles_a_module_name(self) -> None:
        """switch_backup rebinds backup through global, so use_backup gets no exact edge to Courier.run."""
        self.assertFalse(has_edge("calls", L + "carriers#use_backup", L + "carriers#Courier.run"))

    def test_an_attribute_that_two_methods_write_gives_no_exact_call(self) -> None:
        """Dispatch.go calls self.carrier.run, but swap writes a Drone, so no exact edge points at Courier.run."""
        self.assertFalse(has_edge("calls", L + "carriers#Dispatch.go", L + "carriers#Courier.run"))

    def test_every_other_binding_form_unsettles_a_name(self) -> None:
        """A loop, a with, +=, a global +=, a global import, a global := and del each leave no exact edge to Courier.run."""
        for user in ["use_looped", "use_wrapped", "use_counted", "use_added", "use_imported", "use_walrused", "use_deleted"]:
            with self.subTest(user=user):
                self.assertFalse(has_edge("calls", L + "carriers#" + user, L + "carriers#Courier.run"), user + " still calls Courier.run")

    def test_an_augmented_write_unsettles_an_attribute(self) -> None:
        """Tally.bump writes self.carrier += 1, so Tally.go gets no exact edge to Courier.run."""
        self.assertFalse(has_edge("calls", L + "carriers#Tally.go", L + "carriers#Courier.run"))

    def test_a_subclass_write_unsettles_a_base_attribute(self) -> None:
        """AirFleet.fly writes a Drone into self.carrier, so Fleet.go gets no exact edge to Courier.run."""
        self.assertFalse(has_edge("calls", L + "carriers#Fleet.go", L + "carriers#Courier.run"))

    def test_a_member_of_an_unexported_class_stays_unexported(self) -> None:
        """__all__ leaves Shelfmark out, so its public method label counts as not exported."""
        self.assertFalse(symbol(L + "domain#Shelfmark")["exported"])
        self.assertFalse(symbol(L + "domain#Shelfmark.label")["exported"])

    def test_a_name_with_one_write_keeps_its_exact_call(self) -> None:
        """send_fixed calls Courier.run through fixed, which nothing rebinds."""
        self.assertTrue(has_edge("calls", L + "carriers#send_fixed", L + "carriers#Courier.run"))

    def test_a_local_that_another_form_rebinds_gives_no_exact_call(self) -> None:
        """A local +=, a nested nonlocal write and a := inside a comprehension each leave no exact edge to Courier.run."""
        for user in ["bump_local", "relay", "pick_last"]:
            with self.subTest(user=user):
                self.assertFalse(has_edge("calls", L + "carriers#" + user, L + "carriers#Courier.run"), user + " still calls Courier.run")

    def test_a_local_with_one_write_keeps_its_exact_call(self) -> None:
        """send_local calls Courier.run through a local that nothing rebinds."""
        self.assertTrue(has_edge("calls", L + "carriers#send_local", L + "carriers#Courier.run"))

    def test_kinds_follow_the_bases(self) -> None:
        """Status is an enum, Readable a protocol, Loan a named tuple, NotAvailable an exception."""
        self.assertEqual(symbol(L + "domain#Status")["kind"], "enum")
        self.assertEqual(symbol(L + "store.base#Readable")["languageKind"], "protocol")
        self.assertEqual(symbol(L + "domain#Loan")["languageKind"], "named tuple")
        self.assertEqual(symbol(L + "domain#NotAvailable")["languageKind"], "exception")

    def test_flask_mounts_the_blueprint_under_its_prefix(self) -> None:
        """GET /api/books/<book_id> runs show, from the blueprint mounted under /api/books."""
        self.assertEqual(entry("GET /api/books/<book_id>")["handler"], L + "web.flask_app#show")

    def test_a_route_inside_a_factory_serves_inline(self) -> None:
        """GET /health serves inline in create_app."""
        e = entry("GET /health")
        self.assertEqual(e["handler"], L + "web.flask_app#create_app")
        self.assertTrue(e.get("inline"))

    def test_fastapi_joins_the_router_prefixes(self) -> None:
        """GET /v1/loans/{member} runs list_loans."""
        self.assertEqual(entry("GET /v1/loans/{member}")["handler"], L + "web.fastapi_app#list_loans")

    def test_django_includes_and_class_views(self) -> None:
        """Django gives ANY /api/loans/ and GET /books/<int:pk>/ from a class view."""
        self.assertEqual(entry("ANY /api/loans/")["handler"], L + "web.views#loan_list")
        self.assertEqual(entry("GET /books/<int:pk>/")["handler"], L + "web.views#BookView.get")

    def test_click_names_join_from_the_script(self) -> None:
        """lendd report count runs count_cmd."""
        self.assertEqual(entry("lendd report count")["handler"], L + "cli.main#count_cmd")

    def test_typer_names_join_through_add_typer(self) -> None:
        """lendctl users add runs add_user."""
        self.assertEqual(entry("lendctl users add")["handler"], L + "cli.admin#add_user")

    def test_main_guard_names_its_one_call(self) -> None:
        """python -m lending.report runs main."""
        self.assertEqual(entry("python -m lending.report")["handler"], L + "report#main")

    def test_pytest_and_unittest_tests_count(self) -> None:
        """A pytest function, a pytest class and a unittest case each record their uses."""
        names = {t["name"] for t in DOC["tests"][L + "library#Library.borrow"]}
        self.assertIn("tests/test_library.py::test_borrow_lends_a_book", names)
        self.assertIn("tests/test_library.py::GiveBackCase::test_refuses_a_missing_book", names)
        self.assertIn(L + "store.memory#MemoryStore.empty", DOC["tests"])

    def test_outside_packages_go_to_external_imports(self) -> None:
        """The web unit imports django, fastapi and flask from outside."""
        unit = next(c for c in DOC["containers"] if c["id"] == "py:pkg:lending.web")
        self.assertEqual(unit["externalImports"], ["django", "fastapi", "flask"])


class Command(unittest.TestCase):
    """Runs the adapter as a program, as acu does."""

    def test_an_unknown_plugin_fails_with_the_known_names(self) -> None:
        """A request for an unknown plugin fails and names the known ones."""
        request = json.dumps({"protocol": 1, "root": FIXTURE, "repoRoot": FIXTURE, "entryPoints": [{"plugin": "rails"}]})
        env = dict(os.environ, PYTHONPATH=ADAPTER)
        out = subprocess.run([sys.executable, "-m", "acu_adapter_python"], input=request, capture_output=True, text=True, env=env)
        self.assertEqual(out.returncode, 2)
        self.assertIn("acu-adapter-python: the entry-point plugin 'rails' does not exist", out.stderr)
        self.assertIn("flask", out.stderr)


if __name__ == "__main__":
    unittest.main()
