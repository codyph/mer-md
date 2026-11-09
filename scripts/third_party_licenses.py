"""Collect dependency licence texts for release archives."""

import json
import argparse
from pathlib import Path
import subprocess


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--target", help="Rust target triple (default: compiler host)")
    args = parser.parse_args()
    target = args.target
    if target is None:
        version = subprocess.check_output(["rustc", "-vV"], text=True, encoding="utf-8")
        target = next(line.removeprefix("host: ") for line in version.splitlines() if line.startswith("host: "))
    metadata = json.loads(subprocess.check_output(
        ["cargo", "metadata", "--locked", "--format-version", "1", "--filter-platform", target],
        text=True, encoding="utf-8"
    ))
    workspace = set(metadata["workspace_members"])
    # cargo tree filters disabled dependencies from the metadata graph.
    tree = subprocess.check_output(
        ["cargo", "tree", "--locked", "--target", target, "--edges", "normal,build",
         "--prefix", "none", "--format", "{p}", "--color", "never"],
        text=True, encoding="utf-8"
    )
    active = set()
    for line in tree.splitlines():
        fields = line.split()
        if len(fields) < 2 or not fields[1].startswith("v"):
            raise SystemExit(f"Unexpected cargo tree package: {line!r}")
        active.add((fields[0], fields[1][1:]))
    included = {
        package["id"] for package in metadata["packages"]
        if (package["name"], package["version"]) in active
    }
    known = {(package["name"], package["version"]) for package in metadata["packages"]}
    if active - known:
        raise SystemExit(f"Dependency tree packages missing from cargo metadata: {active - known}")
    sections = ["Third-party Rust dependency licences\nVersions are from Cargo.lock.\n"]
    missing = []
    for package in sorted(metadata["packages"], key=lambda p: (p["name"], p["version"])):
        if package["id"] in workspace or package["id"] not in included:
            continue
        directory = Path(package["manifest_path"]).parent
        # Include licence texts for upstream components embedded in crates.
        legal_prefixes = (
            "LICENSE", "LICENCE", "COPYING", "NOTICE", "UNLICENSE",
            "THIRD_PARTY_NOTICES", "THIRD-PARTY-NOTICES", "THIRDPARTYNOTICES",
        )
        candidates = {
            path for path in directory.rglob("*")
            if path.is_file() and (
                path.name.upper().startswith(legal_prefixes)
                or any(part.upper() in {"LICENSES", "LICENCES"}
                       for part in path.relative_to(directory).parts[:-1])
            )
        }
        if package.get("license_file"):
            candidates.add(directory / package["license_file"])
        if not candidates:
            missing.append(f'{package["name"]} {package["version"]}')
            continue
        sections.append(
            f'\n{"=" * 72}\n{package["name"]} {package["version"]}\n'
            f'License: {package.get("license") or "See licence text"}\n'
            f'Source: {package.get("repository") or package.get("homepage") or package["source"]}\n'
        )
        for path in sorted(candidates):
            label = path.relative_to(directory).as_posix()
            sections.append(f'\n--- {label} ---\n{path.read_text(encoding="utf-8")}\n')
    if missing:
        raise SystemExit("Missing dependency licence texts: " + ", ".join(missing))
    output = Path(__file__).resolve().parent.parent / "THIRD_PARTY_LICENSES.txt"
    output.write_text("".join(sections), encoding="utf-8", newline="\n")
    print(f"Collected licences for {len(included - workspace)} dependencies ({target}): {output.name}")


if __name__ == "__main__":
    main()
