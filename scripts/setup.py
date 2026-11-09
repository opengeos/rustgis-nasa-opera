#!/usr/bin/env python3
"""Makes this template your plugin: picks a starter and names it.

    python scripts/setup.py --starter tab --id my-plugin --name "My Plugin" --author "Your Name"

Starters (in starters/), each a complete plugin:

    commands   commands in a group on the Plugins tab (the default in the template)
    tab        a ribbon tab of its own, with groups, a menu and a tool button
    panel      a pane beside the map with a slider, a checkbox, texts and buttons
    tools      geoprocessing tools in the Tools pane

It copies the starter to plugin.json and src/lib.rs, sets the plugin's id, name, author and
homepage (from the git remote unless given), and names the crate after the id. Run it again
to switch starters: it overwrites plugin.json and src/lib.rs. The sections are independent,
so a plugin can have commands, tools, a tab and a panel at once: copy what you need from the
other starters.
"""

from __future__ import annotations

import argparse
import json
import re
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
STARTERS = ROOT / "starters"
ID_RE = re.compile(r"^[a-z0-9][a-z0-9-]*$")


def git_remote() -> str | None:
    """The GitHub address of the `origin` remote, as https://github.com/<owner>/<repo>.

    Returns:
        The address, or None when there is no GitHub remote.
    """
    try:
        url = subprocess.run(["git", "remote", "get-url", "origin"], cwd=ROOT, capture_output=True, text=True, check=True).stdout.strip()
    except (OSError, subprocess.CalledProcessError):
        return None
    m = re.match(r"^(?:https://github\.com/|git@github\.com:)([^/]+)/(.+?)(?:\.git)?/?$", url)
    return f"https://github.com/{m.group(1)}/{m.group(2)}" if m else None


def main() -> int:
    """Copies the chosen starter into place and names it.

    Returns:
        The exit status.
    """
    starters = sorted(p.name for p in STARTERS.iterdir() if p.is_dir())
    remote = git_remote()
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--starter", choices=starters, required=True, help="what the plugin adds to RustGIS")
    ap.add_argument("--id", help="lower-case letters, digits and hyphens (default: the repository's name)")
    ap.add_argument("--name", help="the name people see (default: from the id)")
    ap.add_argument("--author", help="default: the repository's owner")
    ap.add_argument("--homepage", default=remote, help="an https:// address (default: the git remote)")
    ap.add_argument("--description", help="what it does, in a sentence or two")
    args = ap.parse_args()

    repo = remote.rsplit("/", 1)[-1].lower() if remote else None
    plugin_id = args.id or (re.sub(r"^rustgis-", "", repo) if repo else None)
    if not plugin_id:
        ap.error("--id is needed: there is no GitHub remote to take it from")
    if not ID_RE.match(plugin_id):
        ap.error(f"--id {plugin_id!r} must be lower-case letters, digits and hyphens")
    if args.homepage and not args.homepage.startswith("https://"):
        ap.error("--homepage must be an https:// address")

    # Edited as text, so the starter's layout stays as it is.
    text = (STARTERS / args.starter / "plugin.json").read_text(encoding="utf-8")
    name = args.name or plugin_id.replace("-", " ").title()
    author = args.author or (remote.split("/")[-2] if remote else None)
    fields = {"id": plugin_id, "name": name, "author": author, "homepage": args.homepage, "description": args.description}
    for key, value in fields.items():
        if value:
            text = re.sub(rf'(?m)^  "{key}": ".*",$', lambda _, k=key, v=value: f'  "{k}": {json.dumps(v, ensure_ascii=False)},', text, count=1)
    # The starters' "My Plugin" (toolbox, tab and panel labels) becomes the plugin's name, which
    # has no translation yet.
    if name != "My Plugin":
        text = re.sub(r'(?m)^ *"My Plugin": "[^"]*",\n', "", text)
    text = text.replace('"My Plugin"', json.dumps(name, ensure_ascii=False))
    manifest = json.loads(text)
    (ROOT / "plugin.json").write_text(text, encoding="utf-8")

    lib = (STARTERS / args.starter / "lib.rs").read_text(encoding="utf-8")
    lib = lib.replace("My Plugin", manifest["name"]).replace("plugin:my-plugin/", f"plugin:{plugin_id}/").replace("dist/my-plugin", f"dist/{plugin_id}")
    (ROOT / "src" / "lib.rs").write_text(lib, encoding="utf-8")

    cargo = ROOT / "Cargo.toml"
    cargo.write_text(re.sub(r'(?m)^name = ".*"$', f'name = "{plugin_id}"', cargo.read_text(encoding="utf-8"), count=1), encoding="utf-8")

    print(f"{manifest['name']} ({plugin_id}), from the '{args.starter}' starter: plugin.json, src/lib.rs and Cargo.toml.")
    print("Next: python scripts/package.py, then load dist/" + plugin_id + " in RustGIS (see the README).")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
