"""Collect the pinned Rust dependency licenses for binary redistribution."""
from pathlib import Path
import os
import tomllib

root = Path(__file__).resolve().parent.parent
cargo_home = Path(os.environ.get("CARGO_HOME", str(Path.home() / ".cargo")))
if not (cargo_home / "registry/src").is_dir() and (root / ".tools/cargo/registry/src").is_dir():
    cargo_home = root / ".tools/cargo"
packages = tomllib.loads((root / "Cargo.lock").read_text(encoding="utf-8"))["package"]
sections = ["Third-party notices for Aidoku Pixiv\n\nIncludes the source SDK and locked Rust dependencies (including build dependencies).\n\n"]
sections.append("Aidoku SDK — https://github.com/Aidoku/aidoku-rs\n" + (root / "vendor/aidoku/LICENSE").read_text(encoding="utf-8"))
for package in packages:
    if "source" not in package:
        continue
    name, version = package["name"], package["version"]
    candidates = list((cargo_home / "registry/src").glob(f"*/{name}-{version}"))
    if not candidates:
        raise SystemExit(f"Missing cached dependency {name}-{version}; run cargo fetch --locked first.")
    directory = candidates[0]
    metadata = tomllib.loads((directory / "Cargo.toml").read_text(encoding="utf-8"))["package"]
    sections.append(f"\n{'=' * 72}\n{name} {version}\nLicense: {metadata.get('license', 'see license files')}\nSource: https://crates.io/crates/{name}/{version}\n")
    files = sorted(p for p in directory.rglob("*") if p.is_file() and (p.name.upper().startswith("LICENSE") or p.name.upper().startswith("COPYING") or p.name.upper().startswith("NOTICE")))
    for path in files:
        sections.append(f"\n--- {path.relative_to(directory)} ---\n" + path.read_text(encoding="utf-8", errors="replace"))
(root / "sources/multi.pixiv/res/THIRD-PARTY-NOTICES.txt").write_text("\n".join(sections), encoding="utf-8")
print(f"Collected notices for {len(packages) - 1} dependencies.")
