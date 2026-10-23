#!/usr/bin/env python3
"""Upstream's interface as the reference for the port's editor (ref-gui).

Commands:
  build    Configure and build migration/reference/gui (ref-gui).
  layout   Write migration/corpus/gui/layout/<W>x<H>.json: the bounds of the
           components listed in migration/corpus/gui/layout_paths.txt, from
           upstream's own layout code at each size in SIZES. sas-gui's layout
           tests compare against these. --check fails if a committed file
           differs instead of writing it.
  frames   Render every view in migration/corpus/gui/views.json to
           OUT/<name>.png. These show upstream's branding, so they are CI
           artifacts only and are never committed.

Without a DISPLAY the script re-runs itself under xvfb-run. Mesa's software
OpenGL (LIBGL_ALWAYS_SOFTWARE=1) is used either way.
"""

import argparse
import json
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
GUI_DIR = REPO / "migration" / "reference" / "gui"
BUILD_DIR = GUI_DIR / "build"
REF_GUI = BUILD_DIR / "ref-gui"
CORPUS = REPO / "migration" / "corpus" / "gui"
LAYOUT_DIR = CORPUS / "layout"
LAYOUT_PATHS = CORPUS / "layout_paths.txt"
VIEWS = CORPUS / "views.json"

# Default size, smaller and larger scales, both letterbox directions and a 2x
# (high-DPI) window.
SIZES = [(1400, 820), (1050, 615), (700, 410), (2800, 1640), (1400, 700), (1000, 820)]
XVFB_SCREEN = "3000x2000x24"


def ensure_display():
    if os.environ.get("DISPLAY"):
        return
    if not shutil.which("xvfb-run"):
        sys.exit("gui_reference.py: no DISPLAY and no xvfb-run")
    os.execvp("xvfb-run", ["xvfb-run", "-a", "-s", f"-screen 0 {XVFB_SCREEN}",
                           sys.executable, *sys.argv])


def build():
    env = dict(os.environ)
    if "CXX" not in env and shutil.which("g++"):
        env["CXX"] = "g++"
    subprocess.run(["cmake", "-S", str(GUI_DIR), "-B", str(BUILD_DIR),
                    "-DCMAKE_BUILD_TYPE=Release"], check=True, env=env)
    jobs = str(os.cpu_count() or 2)
    subprocess.run(["cmake", "--build", str(BUILD_DIR), "-j", jobs], check=True, env=env)


def run_ref_gui(args):
    if not REF_GUI.exists():
        sys.exit(f"gui_reference.py: {REF_GUI} missing; run '{sys.argv[0]} build' first")
    env = dict(os.environ, LIBGL_ALWAYS_SOFTWARE="1")
    subprocess.run([str(REF_GUI), *args], check=True, env=env, timeout=300)


def layout_paths():
    paths = []
    for line in LAYOUT_PATHS.read_text().splitlines():
        line = line.strip()
        if line and not line.startswith("#"):
            paths.append(line)
    return paths


def layout_for(width, height, paths):
    with tempfile.TemporaryDirectory() as tmp:
        dump = Path(tmp) / "layout.jsonl"
        run_ref_gui(["--layout", str(dump), "--width", str(width), "--height", str(height)])
        entries = {}
        for line in dump.read_text().splitlines():
            entry = json.loads(line)
            entries.setdefault(entry["path"], entry)
    missing = [path for path in paths if path not in entries]
    if missing:
        sys.exit(f"gui_reference.py: upstream has no component {missing} at {width}x{height}")
    return {path: {"bounds": entries[path]["bounds"], "visible": entries[path]["visible"]}
            for path in paths}


def layout(check):
    paths = layout_paths()
    LAYOUT_DIR.mkdir(parents=True, exist_ok=True)
    stale = []
    for width, height in SIZES:
        components = layout_for(width, height, paths)
        lines = [f"  {json.dumps(path)}: {json.dumps(entry)}" for path, entry in components.items()]
        text = (f'{{\n "width": {width},\n "height": {height},\n "components": {{\n'
                + ",\n".join(lines) + "\n }\n}\n")
        target = LAYOUT_DIR / f"{width}x{height}.json"
        if check:
            if not target.exists() or target.read_text() != text:
                stale.append(target.relative_to(REPO))
        else:
            target.write_text(text)
    expected = {f"{w}x{h}.json" for w, h in SIZES}
    extra = sorted(p.name for p in LAYOUT_DIR.glob("*.json") if p.name not in expected)
    if check and (stale or extra):
        sys.exit("gui_reference.py: layout fixtures are stale; run "
                 f"'{sys.argv[0]} layout' and commit the result: {stale + extra}")


def frames(out):
    out = Path(out)
    out.mkdir(parents=True, exist_ok=True)
    views = json.loads(VIEWS.read_text())["views"]
    for view in views:
        args = ["--out", str(out / f"{view['name']}.png"),
                "--tab", view.get("tab", "voice"),
                "--width", str(view.get("width", 1400)),
                "--height", str(view.get("height", 820))]
        if view.get("preset"):
            args += ["--preset", str(REPO / view["preset"])]
        run_ref_gui(args)
        print(f"wrote {out / (view['name'] + '.png')}")


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest="command", required=True)
    sub.add_parser("build")
    layout_parser = sub.add_parser("layout")
    layout_parser.add_argument("--check", action="store_true")
    frames_parser = sub.add_parser("frames")
    frames_parser.add_argument("--out", required=True)
    args = parser.parse_args()

    if args.command == "build":
        build()
        return
    ensure_display()
    if args.command == "layout":
        layout(args.check)
    else:
        frames(args.out)


if __name__ == "__main__":
    main()
