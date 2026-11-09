# mer-md

Turn Mermaid in Markdown into SVGs. Portable, editable, and fully offline.

No Node.js, browser, or rendering service required. [See the demo](demo/README.md).

## Usage

Download the executable for your platform from Releases and run it against a project:

```powershell
.\mer-md.exe .
.\mer-md.exe ../path/to/project
.\mer-md.exe README.md
```

On Linux and macOS:

```sh
chmod +x mer-md
./mer-md .
```

With no path, `mer-md` uses the current directory. Directories are searched recursively for `.md` and `.markdown` files.

Each Mermaid block becomes a centred SVG image. The selected directory contains the generated `docs/diagrams/`; selecting a single file uses its parent directory. Image paths are relative to each Markdown file.

Edit the Mermaid code preserved in the adjacent HTML comment and run the command again. The same SVG is updated without adding another image. Comment delimiters are escaped as `--&gt;` and decoded when rendering; ordinary Mermaid arrows can be typed when editing.

To restore Mermaid fences:

```sh
mer-md . --restore
```

SVGs remain available for reuse. Commit the Markdown and generated SVGs together.

```text
mer-md [PATH] [--restore]

PATH        Markdown file or project directory (default: .)
--restore   Restore Mermaid code blocks
-h, --help  Show help
```

## Output

- Transparent SVGs with automatic light and dark colours.
- Centred images at their natural size, scaled down by responsive viewers.
- Readable source comments, stable filenames, and cached rendering.

Image centring requires HTML support, as provided by GitHub. Bitbucket Data Center's default renderer [escapes HTML](https://confluence.atlassian.com/bitbucketserver/markdown-syntax-guide-776639995.html). Automatic colours follow the browser's preference; colours explicitly set in Mermaid stay fixed.

Mermaid fences must be outside lists and blockquotes. Rendering uses [Merman](https://github.com/Latias94/merman); layouts and syntax support can differ from Mermaid.js. The [44-case corpus](tests/fixtures/diagrams/README.md) covers the supported diagram families.

## Platforms

| Platform | Archive |
| --- | --- |
| Windows x86_64 | `mer-md-windows-x86_64.zip` |
| Linux x86_64 | `mer-md-linux-x86_64.tar.gz` |
| macOS Apple Silicon | `mer-md-macos-aarch64.tar.gz` |
| macOS Intel | `mer-md-macos-x86_64.tar.gz` |

Linux releases statically link musl to avoid a dependency on the distribution's glibc version. A compatible x86_64 CPU and Linux kernel are still required. macOS releases are built and tested on native GitHub runners.

## Development

Requires Rust 1.95+ and platform build tools.

```sh
cargo test --locked
cargo build --release --locked
```

See [development](docs/development.md) for the source layout and checks, and [releasing](docs/releasing.md) for downloadable builds. Distributed dependencies are listed in [THIRD_PARTY_NOTICES](THIRD_PARTY_NOTICES).
