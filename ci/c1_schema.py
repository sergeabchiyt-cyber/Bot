#!/usr/bin/env python3
"""Minimal JSON-Schema validator for the signed C1 contract.

`contracts/c1-market-feed.schema.json` is the normative document; this module
exists so CI can *validate against it* without adding a dependency to the
runner (GitHub's ubuntu-latest images have no `jsonschema` installed, and the
engine must not need a Python package to prove its own contract).

Supported keywords (the subset the C1 schema uses, draft 2020-12 semantics):

    type (string or list), const, enum, required, properties, items,
    additionalProperties, minimum, maximum, exclusiveMinimum, exclusiveMaximum,
    allOf, anyOf, oneOf, not, if / then / else

Anything else in a schema is ignored, which is deliberate: an unknown keyword
must not silently turn into a passing check, so `describe_unsupported()` lists
what this validator skips and `verify_contract.py` fails if the C1 schema ever
grows a keyword that is not covered here.
"""

import json

SUPPORTED = {
    "$schema", "$id", "title", "description",
    "type", "const", "enum", "required", "properties", "items",
    "additionalProperties", "minimum", "maximum", "exclusiveMinimum",
    "exclusiveMaximum", "allOf", "anyOf", "oneOf", "not", "if", "then", "else",
}

SINGLE_SUBSCHEMA = {"items", "additionalProperties", "not", "if", "then", "else"}
LIST_SUBSCHEMA = {"allOf", "anyOf", "oneOf"}

_TYPES = {
    "object": dict,
    "array": list,
    "string": str,
    "boolean": bool,
    "null": type(None),
}


def _is_type(value, name):
    if name == "integer":
        return isinstance(value, int) and not isinstance(value, bool)
    if name == "number":
        return isinstance(value, (int, float)) and not isinstance(value, bool)
    expected = _TYPES.get(name)
    if expected is None:
        raise ValueError(f"unsupported JSON-Schema type: {name!r}")
    if name == "boolean":
        return isinstance(value, bool)
    return isinstance(value, expected)


def _equal(a, b):
    """JSON equality: `true` is not `1`, and `1.0 == 1`."""
    if isinstance(a, bool) or isinstance(b, bool):
        return isinstance(a, bool) and isinstance(b, bool) and a == b
    return a == b


def unsupported_keywords(schema, path="$"):
    """Every schema keyword below `path` that this validator does not enforce."""
    found = []
    if not isinstance(schema, dict):
        return found
    for key, value in schema.items():
        if key not in SUPPORTED and not key.startswith("$"):
            found.append(f"{path}.{key}")
        if not isinstance(value, (dict, list)):
            continue
        if key in SINGLE_SUBSCHEMA:
            found += unsupported_keywords(value, f"{path}.{key}")
        elif key in LIST_SUBSCHEMA:
            for index, sub in enumerate(value):
                found += unsupported_keywords(sub, f"{path}.{key}[{index}]")
        elif key == "properties":
            for name, sub in value.items():
                found += unsupported_keywords(sub, f"{path}.properties.{name}")
    return found


def errors(instance, schema, path="$"):
    """All schema violations of `instance`, as `"<path>: <reason>"` strings."""
    out = []

    def check(value, sub, here):
        if not isinstance(sub, dict):
            return

        if "type" in sub:
            names = sub["type"] if isinstance(sub["type"], list) else [sub["type"]]
            if not any(_is_type(value, name) for name in names):
                out.append(f"{here}: expected type {names}, got {type(value).__name__}")
                return

        if "const" in sub and not _equal(value, sub["const"]):
            out.append(f"{here}: expected const {sub['const']!r}, got {value!r}")

        if "enum" in sub and not any(_equal(value, option) for option in sub["enum"]):
            out.append(f"{here}: {value!r} is not one of {sub['enum']}")

        if isinstance(value, dict):
            for name in sub.get("required", []):
                if name not in value:
                    out.append(f"{here}: missing required property {name!r}")
            properties = sub.get("properties", {})
            for name, child in properties.items():
                if name in value:
                    check(value[name], child, f"{here}.{name}")
            extra = sub.get("additionalProperties")
            if extra is False:
                for name in value:
                    if name not in properties:
                        out.append(f"{here}: unexpected property {name!r}")
            elif isinstance(extra, dict):
                for name, child in value.items():
                    if name not in properties:
                        check(child, extra, f"{here}.{name}")

        if isinstance(value, list) and isinstance(sub.get("items"), dict):
            for index, item in enumerate(value):
                check(item, sub["items"], f"{here}[{index}]")

        if isinstance(value, (int, float)) and not isinstance(value, bool):
            if "minimum" in sub and value < sub["minimum"]:
                out.append(f"{here}: {value} < minimum {sub['minimum']}")
            if "maximum" in sub and value > sub["maximum"]:
                out.append(f"{here}: {value} > maximum {sub['maximum']}")
            if "exclusiveMinimum" in sub and value <= sub["exclusiveMinimum"]:
                out.append(f"{here}: {value} <= exclusiveMinimum {sub['exclusiveMinimum']}")
            if "exclusiveMaximum" in sub and value >= sub["exclusiveMaximum"]:
                out.append(f"{here}: {value} >= exclusiveMaximum {sub['exclusiveMaximum']}")

        for sub_schema in sub.get("allOf", []):
            check(value, sub_schema, here)

        if "anyOf" in sub:
            if not any(not errors(value, option, here) for option in sub["anyOf"]):
                out.append(f"{here}: matches none of the anyOf branches")

        if "oneOf" in sub:
            matches = sum(1 for option in sub["oneOf"] if not errors(value, option, here))
            if matches != 1:
                out.append(f"{here}: matches {matches} of the oneOf branches (need exactly 1)")

        if "not" in sub and not errors(value, sub["not"], here):
            out.append(f"{here}: matches a `not` schema (forbidden shape)")

        if "if" in sub:
            if not errors(value, sub["if"], here):
                check(value, sub.get("then", {}), here)
            else:
                check(value, sub.get("else", {}), here)

    check(instance, schema, path)
    return out


def load(path):
    with open(path, encoding="utf-8") as handle:
        return json.load(handle)


def validate(instance, schema, label="instance"):
    """Raise AssertionError with every violation, or return the instance."""
    problems = errors(instance, schema)
    if problems:
        shown = "\n  ".join(problems[:12])
        more = "" if len(problems) <= 12 else f"\n  ... and {len(problems) - 12} more"
        raise AssertionError(f"{label} violates the C1 schema:\n  {shown}{more}")
    return instance


FORBIDDEN_KEYS = (
    "side",
    "stop_loss",
    "take_profit",
    "lots",
    "risk_usd",
    "action",
    "venue",
    "trade_intent",
    "intent_id",
    "sl",
    "tp",
    "entry",
    "leverage",
    "position",
    "account",
    "equity",
)


def forbidden_keys(document, path="$"):
    """Recursively find trade/execution vocabulary anywhere in a payload.

    The schema only forbids these at the top level of a frame; a market feed
    must not carry them *anywhere*, so every recorded payload is swept.
    """
    found = []
    if isinstance(document, dict):
        for key, value in document.items():
            if key in FORBIDDEN_KEYS:
                found.append(f"{path}.{key}")
            found += forbidden_keys(value, f"{path}.{key}")
    elif isinstance(document, list):
        for index, value in enumerate(document):
            found += forbidden_keys(value, f"{path}[{index}]")
    return found
