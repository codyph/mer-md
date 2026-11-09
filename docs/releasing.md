# Releases

Commit and push the project to GitHub, including `.cargo/`, `.github/`, `scripts/`, `Cargo.toml`, and `Cargo.lock`. `.gitignore` excludes local tools, build output, and generated archives.

For the first release:

```sh
git tag v0.4.0
git push origin v0.4.0
```

The [release workflow](../.github/workflows/release.yml) tests and builds each platform, packages the executables, and publishes a GitHub Release with these downloads and their SHA-256 checksums:

| Platform | Download |
| --- | --- |
| Windows x86_64 | `mer-md-windows-x86_64.zip` |
| Linux x86_64 | `mer-md-linux-x86_64.tar.gz` |
| macOS Apple Silicon | `mer-md-macos-aarch64.tar.gz` |
| macOS Intel | `mer-md-macos-x86_64.tar.gz` |

Watch the run under **Actions → Build portable releases**. Once it succeeds, the downloads appear under **Releases**. Each archive contains the executable, usage notes, demo, and dependency licences. The workflow uses GitHub's automatic token; no personal access token is needed.

To test the builds without publishing, use **Actions → Build portable releases → Run workflow** on the default branch. Download the resulting workflow artifacts. See [GitHub's manual workflow instructions](https://docs.github.com/en/actions/how-tos/manage-workflow-runs/manually-run-a-workflow).

For later releases, update the version in `Cargo.toml`, run `cargo check` to update `Cargo.lock`, and commit and push both files. Push a matching `vVERSION` tag.

## Release helpers

| Script | Purpose |
| --- | --- |
| `scripts/package.py` | Create platform archives and checksums |
| `scripts/third_party_licenses.py` | Collect the locked dependencies' licence texts |

Both use Python's standard library and run on the build machines. Python is not shipped with the executable.

Linux uses static musl; Windows uses the static MSVC runtime. macOS builds run on native Intel and Apple Silicon runners.
