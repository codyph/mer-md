# mer-md

Turn Mermaid in Markdown into SVGs. Portable, editable, and fully offline.

[Download](https://github.com/codyph/mer-md/releases) · [See the demo](demo/README.md)

## Usage

Extract the download for your platform, then run:

```powershell
.\mer-md.exe .                    # Current project
.\mer-md.exe ../path/to/project   # Another project
.\mer-md.exe README.md            # One Markdown file
```

On Linux and macOS, use `./mer-md` instead. With no path, the current directory is used. Directories are searched recursively for `.md` and `.markdown` files.

## Output

Running `mer-md.exe project` creates SVGs under `project/docs/diagrams/`:

```text
project/
├── README.md
└── docs/
    ├── guide.markdown
    └── diagrams/
        ├── d-a1b2c3d4e5f6.svg
        └── d-b2c3d4e5f6a1.svg
```

Each Mermaid block becomes a centred image with a relative link. SVGs have transparent backgrounds and automatic light/dark colours. Selecting one file puts `docs/diagrams/` beside that file.

Edit the Mermaid code preserved in its adjacent `mer-md:source` HTML comment, then run again. The same SVG is updated without adding another image. Arrows escaped as `--&gt;` can be edited using ordinary `-->` syntax.

To restore Mermaid code blocks:

```powershell
.\mer-md.exe . --restore
```

SVGs remain available for reuse. Commit the Markdown and SVGs together.

Each run reports files scanned, files updated, diagrams rendered or cached, and elapsed time. On Windows, a terminal opened for the executable closes three seconds after completion.

## Compatibility

- Windows x86_64, Linux x86_64, and macOS Intel/Apple Silicon.
- No Node.js, browser, or rendering service required. Layout fonts are embedded.
- Linux builds statically link musl; a compatible x86_64 CPU and Linux kernel are required.

Rendering uses [Merman](https://github.com/Latias94/merman); layouts and syntax support can differ from Mermaid.js. Mermaid fences must be outside lists and blockquotes. Explicit Mermaid colours stay fixed.

Image centring requires HTML support, as provided by GitHub. Bitbucket Data Center's default renderer [escapes HTML](https://confluence.atlassian.com/bitbucketserver/markdown-syntax-guide-776639995.html).

## Build

Requires Rust 1.95+ and platform build tools.

```sh
cargo test --locked
cargo build --release --locked
```

See [THIRD_PARTY_NOTICES](THIRD_PARTY_NOTICES) for dependency licences.
