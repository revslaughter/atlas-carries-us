"""Checks a JSON value against the project's JSON Schema, without a package.

It knows the keywords that schema/atlas.schema.json uses: $ref, type,
required, properties, additionalProperties, items, enum, const, oneOf,
minLength, minimum, maximum and pattern.
"""

from __future__ import annotations

import re

_TYPES = {
    "object": dict,
    "array": list,
    "string": str,
    "boolean": bool,
    "null": type(None),
}


def _is_type(value: object, name: str) -> bool:
    """Reports whether a value has the JSON type ``name``."""
    if name == "integer":
        return isinstance(value, int) and not isinstance(value, bool)
    if name == "number":
        return isinstance(value, (int, float)) and not isinstance(value, bool)
    return isinstance(value, _TYPES[name])


def validate(value: object, schema: dict, root: dict, path: str = "$") -> list[str]:
    """Returns one message per rule that the value breaks."""
    if "$ref" in schema:
        ref = schema["$ref"]
        assert ref.startswith("#/$defs/"), ref
        return validate(value, root["$defs"][ref[len("#/$defs/") :]], root, path)
    errors: list[str] = []
    if "type" in schema:
        types = schema["type"] if isinstance(schema["type"], list) else [schema["type"]]
        if not any(_is_type(value, t) for t in types):
            return [f"{path}: wants {schema['type']}, holds {type(value).__name__}"]
    if "const" in schema and value != schema["const"]:
        errors.append(f"{path}: wants {schema['const']!r}")
    if "enum" in schema and value not in schema["enum"]:
        errors.append(f"{path}: {value!r} is not one of {schema['enum']}")
    if "oneOf" in schema:
        passing = [s for s in schema["oneOf"] if not validate(value, s, root, path)]
        if len(passing) != 1:
            errors.append(f"{path}: matches {len(passing)} of the oneOf choices")
    if isinstance(value, str):
        if len(value) < schema.get("minLength", 0):
            errors.append(f"{path}: shorter than {schema['minLength']}")
        if "pattern" in schema and not re.search(schema["pattern"], value):
            errors.append(f"{path}: does not match {schema['pattern']}")
    if isinstance(value, (int, float)) and not isinstance(value, bool):
        if "minimum" in schema and value < schema["minimum"]:
            errors.append(f"{path}: below {schema['minimum']}")
        if "maximum" in schema and value > schema["maximum"]:
            errors.append(f"{path}: above {schema['maximum']}")
    if isinstance(value, dict):
        for key in schema.get("required", []):
            if key not in value:
                errors.append(f"{path}: lacks {key}")
        props = schema.get("properties", {})
        extra = schema.get("additionalProperties", True)
        for key, item in value.items():
            if key in props:
                errors.extend(validate(item, props[key], root, f"{path}.{key}"))
            elif extra is False:
                errors.append(f"{path}: holds the unknown field {key}")
            elif isinstance(extra, dict):
                errors.extend(validate(item, extra, root, f"{path}.{key}"))
    if isinstance(value, list) and "items" in schema:
        for i, item in enumerate(value):
            errors.extend(validate(item, schema["items"], root, f"{path}[{i}]"))
    return errors
