"""Build Aidoku's standard source-list index and versioned package/icon paths."""
from pathlib import Path
from zipfile import ZipFile
import json
import shutil

root = Path(__file__).resolve().parent.parent
output = root / "public"
with ZipFile(root / "dist/Pixiv.aix") as package:
    info = json.loads(package.read("Payload/source.json"))["info"]
    stem = f"{info['id']}-v{info['version']}"
    (output / "icons").mkdir(parents=True, exist_ok=True)
    (output / "sources").mkdir(parents=True, exist_ok=True)
    (output / f"icons/{stem}.png").write_bytes(package.read("Payload/icon.png"))
shutil.copyfile(root / "dist/Pixiv.aix", output / f"sources/{stem}.aix")
item = {
    "id": info["id"], "name": info["name"], "version": info["version"],
    "iconURL": f"icons/{stem}.png", "downloadURL": f"sources/{stem}.aix",
    "languages": info["languages"], "contentRating": info.get("contentRating", 0),
    "baseURL": info.get("url"),
}
for key in ("altNames", "minAppVersion", "maxAppVersion"):
    if key in info:
        item[key] = info[key]
index = {"name": "Pixiv for Aidoku", "sources": [item]}
(output / "index.json").write_text(json.dumps(index, ensure_ascii=False, indent="\t") + "\n", encoding="utf-8")
(output / "index.min.json").write_text(json.dumps(index, ensure_ascii=False, separators=(",", ":")), encoding="utf-8")
for entry in index["sources"]:
    assert (output / entry["downloadURL"]).is_file()
    assert (output / entry["iconURL"]).is_file()
print(f"Built {output / 'index.min.json'}")
