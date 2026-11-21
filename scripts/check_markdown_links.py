#!/usr/bin/env python3
"""Check that relative Markdown links point at files that exist.

Links to HTTP(S) URLs, mailto addresses and pure anchors are ignored. Anchors on
relative links are stripped before the file is checked. Run from anywhere:

    python3 scripts/check_markdown_links.py [root]

The script exits non-zero and prints every broken link when a target is missing.
"""

from __future__ import annotations

import os
import re
import sys

SKIP_DIRS = {".git", "target", "node_modules"}
LINK = re.compile(r"\]\(([^)\s]+)\)")


def markdown_files(root: str) -> list[str]:
    found: list[str] = []
    for directory, dirnames, filenames in os.walk(root):
        dirnames[:] = sorted(name for name in dirnames if name not in SKIP_DIRS)
        for name in sorted(filenames):
            if name.endswith(".md"):
                found.append(os.path.join(directory, name))
    return found


def broken_links(root: str) -> list[str]:
    broken: list[str] = []
    for path in markdown_files(root):
        with open(path, encoding="utf-8") as handle:
            text = handle.read()
        for target in LINK.findall(text):
            if target.startswith(("http://", "https://", "mailto:", "#")):
                continue
            local = target.split("#", 1)[0]
            if not local:
                continue
            resolved = os.path.normpath(os.path.join(os.path.dirname(path), local))
            if not os.path.exists(resolved):
                broken.append(f"{os.path.relpath(path, root)}: {target}")
    return broken


def main(argv: list[str]) -> int:
    root = argv[1] if len(argv) > 1 else "."
    broken = broken_links(root)
    print(f"checked {len(markdown_files(root))} Markdown files")
    if broken:
        print(f"broken links: {len(broken)}")
        for entry in broken:
            print(f"  {entry}")
        return 1
    print("broken links: 0")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
