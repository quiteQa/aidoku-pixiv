"""Validate resources against the official Aidoku SDK schemas."""
from pathlib import Path
import json
import jsonschema

root = Path(__file__).resolve().parent.parent
for name in ("source", "settings", "filters"):
    data = json.loads((root / f"sources/multi.pixiv/res/{name}.json").read_text(encoding="utf-8"))
    schema = json.loads((root / f"vendor/aidoku-schema/{name}.schema.json").read_text(encoding="utf-8"))
    jsonschema.Draft7Validator(schema).validate(data)
    print(f"PASS {name}.json")
