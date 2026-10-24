"""Package a built source using Aidoku's Payload/*.aix convention (stdlib only)."""
from pathlib import Path
from zipfile import ZIP_DEFLATED, ZipFile
import json
import sys

root = Path(__file__).resolve().parent.parent
binary = root / "target/wasm32-unknown-unknown/release/aidoku_pixiv.wasm"
if not binary.is_file() or binary.read_bytes()[:8] != b"\0asm\x01\0\0\0":
    sys.exit("Build first: cargo build --release --locked")
resources = root / "sources/multi.pixiv/res"
for name in ("source.json", "settings.json", "filters.json"):
    json.loads((resources / name).read_text(encoding="utf-8"))
for name in ("icon.png", "THIRD-PARTY-NOTICES.txt"):
    if not (resources / name).is_file():
        sys.exit(f"Missing {name}; generate resources and run python scripts/licenses.py first.")
output = root / "dist/Pixiv.aix"
output.parent.mkdir(exist_ok=True)
with ZipFile(output, "w", compression=ZIP_DEFLATED) as package:
    package.write(binary, "Payload/main.wasm")
    package.write(root / "LICENSE", "Payload/LICENSE")
    for path in sorted(resources.iterdir()):
        if path.is_file():
            package.write(path, "Payload/" + path.name)
with ZipFile(output) as package:
    assert package.testzip() is None
    assert "Payload/icon.png" in package.namelist()
print(f"Created {output} ({output.stat().st_size:,} bytes)")
