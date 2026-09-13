#!/usr/bin/env python3
"""Keep current release commands and download buttons in sync with Cargo.toml.

Python 3.10+, no third-party packages. Historical compatibility reports, dependency
versions and prose describing when a fix was introduced are intentionally retained.
"""
import argparse
from pathlib import Path
import re
import sys

ROOT = Path(__file__).resolve().parent.parent
VERSION = r"[0-9]+\.[0-9]+\.[0-9]+(?:-[0-9A-Za-z][0-9A-Za-z.-]*)?"
REPO = "STEALTHNET-APP/STEALTHNET-SOFTWARE"
HISTORICAL_DOCS = {"docs/compatibility.md", "docs/repository-preparation.md"}
COMMANDS = (
    r"\bmake[ \t]+update[ \t]+VERSION=",
    r"\bstealthnet[ \t]+update[ \t]+--version[ \t]+",
    r"\b(?:[\w./-]*/)?(?:install|update|stealthnet-install)\.sh[ \t]+--version[ \t]+",
    r"\b(?:[\w./-]*/)?package-release\.py[ \t]+--version[ \t]+",
    r"\bSN_RELEASE_VERSION=",
)


def replace_version(text, prefix, version):
    pattern = rf"({prefix}[\"']?v?){VERSION}(?![0-9A-Za-z.+-])"
    return re.sub(pattern, lambda match: match[1] + version, text)


def workspace_version(root):
    # Only the workspace package table, never a dependency's version field.
    text = (root / "Cargo.toml").read_text(encoding="utf-8")
    table = re.search(r"(?ms)^\[workspace\.package\]\s*\n(.*?)(?=^\[|\Z)", text)
    match = re.search(rf'^version\s*=\s*"({VERSION})"\s*$', table[1], re.M) if table else None
    if not match:
        raise ValueError("Cargo.toml: missing or unsupported workspace.package.version")
    return match[1]


def lock_versions(root, text, version):
    names = set()
    for manifest in (root / "crates").glob("*/Cargo.toml"):
        content = manifest.read_text(encoding="utf-8")
        package = re.search(r"(?ms)^\[package\]\s*\n(.*?)(?=^\[|\Z)", content)
        if package and re.search(r"(?m)^version\.workspace\s*=\s*true\s*$", package[1]):
            name = re.search(r'^name\s*=\s*"([^"]+)"', package[1], re.M)
            if not name:
                raise ValueError(f"{manifest.relative_to(root)}: missing package name")
            names.add(name[1])
    if not names:
        raise ValueError("No crates inheriting the workspace version found")
    found = set()

    def update_package(match):
        block = match[0]
        name = re.search(r'^name = "([^"]+)"$', block, re.M)
        if not name or name[1] not in names or re.search(r"(?m)^source\s*=", block):
            return block
        found.add(name[1])
        updated, count = re.subn(r'^version = "[^"]+"$', f'version = "{version}"', block, flags=re.M)
        if count != 1:
            raise ValueError(f"Cargo.lock: expected one version for {name[1]}")
        return updated

    result = re.sub(r"(?ms)^\[\[package\]\]\n.*?(?=^\[\[package\]\]|\Z)", update_package, text)
    if found != names:
        raise ValueError("Cargo.lock: missing workspace packages: " + ", ".join(sorted(names - found)))
    return result


def public_text(text, path, version):
    lines = []
    for line in text.splitlines(keepends=True):
        for prefix in COMMANDS:
            line = replace_version(line, prefix, version)
        # Do not change git examples for other projects.
        if "git clone " in line and REPO + ".git" in line:
            line = replace_version(line, r"--branch[ \t]+", version)
        # Tagged installer URLs and release download links for this project only.
        line = replace_version(
            line,
            rf"https://(?:github\.com|raw\.githubusercontent\.com)/{REPO}/(?:releases/(?:tag|download)/)?",
            version,
        )
        lines.append(line)
    result = "".join(lines)
    if path.name in {"README.md", "README.ru.md"} or path.suffix == ".svg":
        for prefix in (r"(?m:^\*\*v)", r"Download v", r"Скачать v"):
            result = replace_version(result, prefix, version)
    return result


def planned_changes(root):
    version = workspace_version(root)
    paths = {root / "Cargo.lock", root / "Makefile"}
    paths.update(root.glob("README*.md"))
    paths.update(
        path for path in (root / "docs").rglob("*.md")
        if not any(part.startswith("audit-") or part == "design" for part in path.relative_to(root / "docs").parts[:-1])
    )
    paths.update((root / "docs/media/navigation").glob("*.svg"))
    paths.update(root.glob("*.sh"))
    paths.update((root / "deploy").glob("*.sh"))
    paths.update((root / "web").glob("*-installation*.html"))
    changes = {}
    for path in sorted(paths):
        if path.relative_to(root).as_posix() in HISTORICAL_DOCS:
            continue
        before = path.read_text(encoding="utf-8")
        after = lock_versions(root, before, version) if path.name == "Cargo.lock" else public_text(before, path, version)
        if before != after:
            changes[path] = after
    return version, changes


def main(argv=None, *, root=ROOT):
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--check", action="store_true", help="Fail if current release references are out of sync")
    mode.add_argument("--sync", action="store_true", help="Update references from Cargo.toml; review the resulting diff")
    args = parser.parse_args(argv)
    try:
        version, changes = planned_changes(root)
        for path, content in changes.items():
            if args.sync:
                path.write_text(content, encoding="utf-8")
            print(f"{'Updated' if args.sync else 'Out of sync'}: {path.relative_to(root)}")
        if changes and args.check:
            print("Run python3 devtools/release_version.py --sync and review the diff.", file=sys.stderr)
            return 1
        print(f"Release references match v{version} ({len(changes)} files updated).")
        return 0
    except (OSError, ValueError) as error:
        print(f"Release version check failed: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
