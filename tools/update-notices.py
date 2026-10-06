"""Refresh distributable dependency notices after changing either lockfile."""
import hashlib
import json
from pathlib import Path
import subprocess
import urllib.error
import urllib.request

ROOT = Path(__file__).resolve().parents[1]
metadata = json.loads(subprocess.check_output(
    ["cargo", "metadata", "--locked", "--format-version", "1", "--filter-platform",
     "x86_64-pc-windows-msvc", "--manifest-path",
     str(ROOT / "src-tauri/Cargo.toml")], cwd=ROOT, text=True, encoding="utf-8"))
resolved = {node["id"] for node in metadata["resolve"]["nodes"]}
packages = [(p["name"] + " " + p["version"], Path(p["manifest_path"]).parent, p.get("repository"))
            for p in metadata["packages"] if p.get("source") and p["id"] in resolved]
for name in ["svelte", "dompurify", "@tauri-apps/api", "@fontsource-variable/inter"]:
    folder = ROOT / "node_modules" / name
    package = json.loads((folder / "package.json").read_text(encoding="utf-8"))
    packages.append((name + " " + package["version"], folder, None))

groups = {}
missing = []
for name, folder, repository in packages:
    files = [p for p in folder.iterdir() if p.is_file()
             and p.name.upper().startswith(("LICENSE", "LICENCE", "COPYING", "NOTICE"))]
    bodies = [file.read_text(encoding="utf-8", errors="replace").strip() for file in files]
    if not bodies and repository and repository.startswith("https://github.com/"):
        revision = json.loads((folder / ".cargo_vcs_info.json").read_text())["git"]["sha1"]
        base = repository.removeprefix("https://github.com/").removesuffix(".git")
        for filename in ["LICENSE", "LICENSE-MIT", "LICENSE.txt", "LICENSE.md", "COPYING"]:
            source = f"https://raw.githubusercontent.com/{base}/{revision}/{filename}"
            try:
                with urllib.request.urlopen(source, timeout=20) as response:
                    bodies.append(response.read().decode("utf-8").strip())
                name += f" (source: https://github.com/{base}/tree/{revision})"
                break
            except urllib.error.HTTPError as error:
                if error.code != 404:
                    raise
    if not bodies:
        if name == "selectors 0.38.0":
            # Stylo declares MPL-2.0 in source headers and omits the full text in its archive.
            bodies.append((ROOT / "node_modules/dompurify/LICENSE-MPL").read_text(encoding="utf-8"))
            name += f" (source: https://github.com/{base}/tree/{revision}/selectors)"
        else:
            missing.append(name)
    for body in bodies:
        digest = hashlib.sha256(body.encode()).hexdigest()
        group = groups.setdefault(digest, {"body": body, "packages": []})
        group["packages"].append(name)
if missing:
    raise SystemExit("No licence file found for: " + ", ".join(missing))

sections = ["NanoReader dependency notices\n\nNanoReader is MIT licensed. "
            "Its dependencies retain their own licences.\n"
            "This file includes locked Rust dependencies and shipped frontend dependencies.\n"]
for group in groups.values():
    sections.append("\n" + "=" * 72 + "\n" + ", ".join(sorted(set(group["packages"])))
                    + "\n\n" + group["body"] + "\n")
output = "\n".join(line.rstrip() for line in "".join(sections).splitlines()) + "\n"
(ROOT / "THIRD-PARTY-NOTICES.txt").write_text(output, encoding="utf-8")
print(f"Wrote {len(groups)} notice texts for {len(packages)} packages.")
