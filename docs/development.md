# Development

## Layout

| Path | Purpose |
| --- | --- |
| `src/cli.rs` | Arguments and help |
| `src/project.rs` | File discovery, caching, safe writes |
| `src/document.rs` | Markdown, source comments, stable identities |
| `src/render/` | Layout, colours, SVG processing |
| `tests/` | CLI, rendering, and diagram fixtures |
| `demo/` | Mermaid source and rendered SVGs |
| `scripts/` | Release packaging and dependency licences |
| `.cargo/config.toml` | Static Windows runtime configuration |
| `.github/workflows/release.yml` | Platform tests, builds, and releases |

## Checks

```sh
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
```

The CLI tests cover source edits, repeated rendering, restoration, relative links, and overwrite protection. Diagram tests cover 44 cases; geometric checks protect label placement and SVG dimensions.

Render the checked-in demo with the built executable: `target/release/mer-md demo` (`mer-md.exe` on Windows).

Python is used only by the two [release helpers](releasing.md). It is not required to build, test, or run the application. Local tool downloads, compiled output, and generated archives are excluded by `.gitignore`.
