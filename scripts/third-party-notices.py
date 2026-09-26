"""Writes THIRD_PARTY_NOTICES.md: bundled programs, then every Rust crate and npm package
that ends up in the Windows build, with its license. Run after changing dependencies:

    python scripts/third-party-notices.py
"""
import json
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

HEADER = """# Third-party notices

Plugcam is licensed under the Apache License 2.0 (see [LICENSE](LICENSE)). It ships with or is
built from the following third-party software. The installer puts the full license texts of the
bundled programs in its `licenses` folder.

## Bundled programs

| Component | Version | License | Source |
| --- | --- | --- | --- |
| scrcpy-server (runs on the phone, pushed over adb) | 4.1 | Apache-2.0, © Genymobile, © Romain Vimont | https://github.com/Genymobile/scrcpy |
| adb, AdbWinApi.dll, AdbWinUsbApi.dll (Android SDK Platform-Tools) | latest at build time | Apache-2.0 and others, see `platform-tools-NOTICE.txt` | https://developer.android.com/tools/releases/platform-tools |
| Softcam (the virtual camera, forked as plugcam_cam.dll) | fork of 1.x | MIT, © 2020 tshino | https://github.com/tshino/softcam |

scrcpy-server is used unmodified. The Softcam fork changes the camera name, class ID and shared
memory names (see [vcam/PLUGCAM.md](vcam/PLUGCAM.md)).
"""


def rust_crates():
    meta = json.loads(subprocess.check_output(
        ["cargo", "metadata", "--format-version", "1", "--filter-platform", "x86_64-pc-windows-msvc",
         "--manifest-path", str(ROOT / "src-tauri" / "Cargo.toml")]))
    packages = {p["id"]: p for p in meta["packages"]}
    nodes = {n["id"]: n for n in meta["resolve"]["nodes"]}
    root = meta["resolve"]["root"]
    seen, stack = set(), [root]
    while stack:
        pid = stack.pop()
        for dep in nodes[pid]["deps"]:
            # Only what is compiled into the program: skip build scripts' and tests' deps.
            if any(k["kind"] is None for k in dep["dep_kinds"]) and dep["pkg"] not in seen:
                seen.add(dep["pkg"])
                stack.append(dep["pkg"])
    rows = []
    for pid in seen:
        p = packages[pid]
        rows.append((p["name"], p["version"], p.get("license") or "see crate", p.get("repository") or ""))
    return sorted(rows, key=lambda r: (r[0].lower(), r[1]))


def npm_packages():
    rows = []
    for name in ["svelte", "@tauri-apps/api", "@lucide/svelte"]:
        pkg = json.loads((ROOT / "node_modules" / name / "package.json").read_text(encoding="utf-8"))
        repo = pkg.get("repository")
        repo = repo.get("url") if isinstance(repo, dict) else repo
        rows.append((name, pkg["version"], pkg.get("license", ""), (repo or "").removeprefix("git+")))
    return rows


def table(rows):
    lines = ["| Package | Version | License | Repository |", "| --- | --- | --- | --- |"]
    lines += [f"| {n} | {v} | {l} | {r} |" for n, v, l, r in rows]
    return "\n".join(lines)


crates = rust_crates()
text = HEADER + f"""
## JavaScript packages in the window

{table(npm_packages())}

## Rust crates compiled into plugcam.exe ({len(crates)})

{table(crates)}
"""
(ROOT / "THIRD_PARTY_NOTICES.md").write_text(text, encoding="utf-8", newline="\n")
print(f"THIRD_PARTY_NOTICES.md: {len(crates)} crates")
