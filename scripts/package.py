"""Package a portable executable and its licences."""

import argparse
import hashlib
import os
from pathlib import Path
import subprocess
import sys
import tarfile
import zipfile


PLATFORMS = {
    "x86_64-pc-windows-msvc": "windows-x86_64",
    "x86_64-unknown-linux-musl": "linux-x86_64",
    "aarch64-apple-darwin": "macos-aarch64",
    "x86_64-apple-darwin": "macos-x86_64",
}


def main():
    root = Path(__file__).resolve().parent.parent
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--target", required=True, choices=PLATFORMS, help="Rust target")
    parser.add_argument("--binary", type=Path, help="Executable path")
    args = parser.parse_args()
    windows = args.target.endswith("windows-msvc")
    binary = (args.binary or root / "target" / args.target / "release" / ("mer-md.exe" if windows else "mer-md")).resolve(strict=True)
    subprocess.run([sys.executable, str(root / "scripts/third_party_licenses.py"), "--target", args.target], cwd=root, check=True)
    files = [(binary, binary.name)]
    files.append((root / "docs/release.md", "README.md"))
    files += [(root / name, name) for name in ["LICENSE", "THIRD_PARTY_NOTICES", "THIRD_PARTY_LICENSES.txt"]]
    files += [(path, path.relative_to(root).as_posix()) for path in sorted((root / "demo").rglob("*")) if path.is_file()]
    assert len(list((root / "demo/docs/diagrams").glob("*.svg"))) == 6, "Render the demo before packaging"
    output = root / "artifacts" / f"mer-md-{PLATFORMS[args.target]}{'.zip' if windows else '.tar.gz'}"
    output.parent.mkdir(exist_ok=True)
    temporary = output.with_name(output.name + ".tmp")
    if windows:
        with zipfile.ZipFile(temporary, "w", zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
            for path, name in files:
                archive.write(path, name)
        with zipfile.ZipFile(temporary) as archive:
            assert archive.testzip() is None
            assert archive.read(binary.name) == binary.read_bytes()
    else:
        with tarfile.open(temporary, "w:gz", compresslevel=9) as archive:
            for path, name in files:
                info = archive.gettarinfo(str(path), arcname=name)
                info.mode = 0o755 if path == binary else 0o644
                info.uid = info.gid = 0
                info.uname = info.gname = ""
                with path.open("rb") as source:
                    archive.addfile(info, source)
        with tarfile.open(temporary) as archive:
            assert archive.getmember(binary.name).mode == 0o755
            assert archive.extractfile(binary.name).read() == binary.read_bytes()
    os.replace(temporary, output)
    digest = hashlib.sha256(output.read_bytes()).hexdigest()
    output.with_name(output.name + ".sha256").write_text(f"{digest}  {output.name}\n", encoding="ascii")
    print(f"{output.name}: {output.stat().st_size:,} bytes")


if __name__ == "__main__":
    main()
