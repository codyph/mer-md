use crate::cli::Options;
use crate::document::{self, Diagram};
use crate::render;
use anyhow::{Context, Result, bail};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

struct Change {
    path: PathBuf,
    bytes: Vec<u8>,
    original: Option<Vec<u8>>,
}

pub(crate) fn execute(options: &Options) -> Result<()> {
    let selected = fs::symlink_metadata(&options.path)
        .with_context(|| format!("cannot read {}", options.path.display()))?;
    if selected.file_type().is_symlink() {
        bail!("select a real file or directory rather than a symbolic link");
    }
    let target = fs::canonicalize(&options.path)?;
    let base = if target.is_dir() {
        target.as_path()
    } else {
        target.parent().context("file has no parent")?
    };
    let root = base.to_path_buf();
    let files = collect_markdown(&target)?;
    let output_dir = root.join("docs").join("diagrams");
    if !options.restore {
        check_output_directory(&root, &output_dir)?;
    }
    let mut svg_changes = Vec::new();
    let mut markdown_changes = Vec::new();
    let mut seen_ids: HashMap<String, PathBuf> = HashMap::new();
    let mut diagram_count = 0;
    let mut cached_count = 0;

    // Plan every file and render every changed diagram before touching outputs.
    for path in &files {
        let markdown = fs::read_to_string(path).with_context(|| {
            format!(
                "cannot read UTF-8 Markdown {}",
                display_relative(path, &root)
            )
        })?;
        let key = document_key(path, &root);
        let diagrams = document::discover(&markdown, &key).with_context(|| {
            format!(
                "invalid Mermaid section in {}",
                display_relative(path, &root)
            )
        })?;
        diagram_count += diagrams.len();
        let updated = if options.restore {
            document::restore_document(&markdown, &diagrams)?
        } else {
            let mut links = Vec::with_capacity(diagrams.len());
            for diagram in &diagrams {
                if let Some(previous) = seen_ids.insert(diagram.id.clone(), path.clone()) {
                    bail!(
                        "diagram ID {} is shared by {} and {}; remove a copied ID marker to create a new diagram",
                        diagram.id,
                        display_relative(&previous, &root),
                        display_relative(path, &root)
                    );
                }
                let svg_path = output_dir.join(format!("{}.svg", diagram.id));
                check_output_file(&svg_path)?;
                let fingerprint = diagram_fingerprint(diagram);
                let owner = STANDARD.encode(key.as_bytes());
                let header =
                    format!("<!-- portable-mermaid:cache v1 {fingerprint} owner={owner} -->\n");
                let existing = match fs::read_to_string(&svg_path) {
                    Ok(text) => Some(text),
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
                    Err(error) => {
                        return Err(error)
                            .with_context(|| format!("cannot read {}", svg_path.display()));
                    }
                };
                if let Some(text) = &existing {
                    verify_svg_ownership(text, &root, &key, &diagram.id)
                        .with_context(|| format!("cannot replace {}", svg_path.display()))?;
                }
                if existing.as_ref().is_some_and(|text| {
                    text.starts_with(&header)
                        && text.contains("<svg ")
                        && text.trim_end().ends_with("</svg>")
                }) {
                    cached_count += 1;
                } else {
                    let svg = render::render(&diagram.source).with_context(|| {
                        format!(
                            "cannot render diagram {} in {}",
                            diagram.id,
                            display_relative(path, &root)
                        )
                    })?;
                    let bytes = format!("{header}{svg}\n").into_bytes();
                    if existing.as_ref().map(String::as_bytes) != Some(bytes.as_slice()) {
                        svg_changes.push(Change {
                            path: svg_path.clone(),
                            bytes,
                            original: existing.as_ref().map(|text| text.as_bytes().to_vec()),
                        });
                    }
                }
                let relative = pathdiff::diff_paths(
                    &svg_path,
                    path.parent().context("Markdown has no parent")?,
                )
                .context("cannot construct relative SVG link")?;
                links.push(relative.to_string_lossy().replace('\\', "/"));
            }
            document::render_document(&markdown, &diagrams, &links)?
        };
        if updated != markdown {
            markdown_changes.push(Change {
                path: path.clone(),
                bytes: updated.into_bytes(),
                original: Some(markdown.into_bytes()),
            });
        }
    }

    for change in svg_changes.iter().chain(markdown_changes.iter()) {
        verify_unchanged(change)?;
    }
    // Write images first so every new Markdown reference has its SVG.
    for change in svg_changes.iter().chain(markdown_changes.iter()) {
        atomic_write(&change.path, &change.bytes)?;
    }
    println!(
        "Updated {} Markdown file(s), {} SVG(s); {} diagram(s), {} cached.",
        markdown_changes.len(),
        svg_changes.len(),
        diagram_count,
        cached_count
    );
    Ok(())
}

fn is_markdown(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("md") || ext.eq_ignore_ascii_case("markdown"))
}

fn collect_markdown(target: &Path) -> Result<Vec<PathBuf>> {
    if target.is_file() {
        if !is_markdown(target) {
            bail!("selected file must have .md or .markdown extension");
        }
        return Ok(vec![target.to_path_buf()]);
    }
    if !target.is_dir() {
        bail!("select a Markdown file or directory");
    }
    fn visit(dir: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
        for entry in fs::read_dir(dir).with_context(|| format!("cannot scan {}", dir.display()))? {
            let entry = entry?;
            let kind = entry.file_type()?;
            if kind.is_symlink() {
                continue;
            }
            let path = entry.path();
            if kind.is_dir() {
                let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
                if name.starts_with('.')
                    || matches!(
                        name.as_str(),
                        "node_modules"
                            | "target"
                            | "vendor"
                            | "dist"
                            | "build"
                            | "venv"
                            | "__pycache__"
                    )
                {
                    continue;
                }
                if name == "diagrams"
                    && dir
                        .file_name()
                        .is_some_and(|name| name.eq_ignore_ascii_case("docs"))
                {
                    continue;
                }
                visit(&path, files)?;
            } else if kind.is_file() && is_markdown(&path) {
                files.push(path);
            }
        }
        Ok(())
    }
    let mut files = Vec::new();
    visit(target, &mut files)?;
    files.sort();
    Ok(files)
}

fn check_output_directory(root: &Path, dir: &Path) -> Result<()> {
    for component in [root.join("docs"), dir.to_path_buf()] {
        match fs::symlink_metadata(&component) {
            Ok(meta) if meta.file_type().is_symlink() || !meta.is_dir() => {
                bail!(
                    "output directory must be a real directory: {}",
                    component.display()
                );
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

fn check_output_file(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_symlink() || !meta.is_file() => {
            bail!("output must be a real file: {}", path.display())
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn document_key(path: &Path, root: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn display_relative(path: &Path, root: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .display()
        .to_string()
}

fn diagram_fingerprint(diagram: &Diagram) -> String {
    let mut hash = Sha256::new();
    for value in [
        "cache-v1",
        env!("CARGO_PKG_VERSION"),
        render::RENDERER_VERSION,
        "automatic-light-dark",
        &diagram.source,
    ] {
        hash.update((value.len() as u64).to_le_bytes());
        hash.update(value.as_bytes());
    }
    format!("{:x}", hash.finalize())
}

fn verify_svg_ownership(svg: &str, root: &Path, key: &str, id: &str) -> Result<()> {
    let header = svg.lines().next().unwrap_or("");
    if !header.starts_with("<!-- portable-mermaid:cache v1 ") {
        bail!("refusing to overwrite an SVG not created by this tool");
    }
    let encoded = header
        .split_whitespace()
        .find_map(|part| part.strip_prefix("owner="))
        .context("generated SVG has no ownership information; move it aside before regenerating")?;
    let owner = String::from_utf8(
        STANDARD
            .decode(encoded)
            .context("invalid SVG owner metadata")?,
    )?;
    if owner == key {
        return Ok(());
    }
    let owner_relative = Path::new(&owner);
    if owner_relative.is_absolute()
        || owner_relative
            .components()
            .any(|component| !matches!(component, std::path::Component::Normal(_)))
    {
        bail!("invalid SVG owner path");
    }
    let owner_path = root.join(owner_relative);
    match fs::symlink_metadata(&owner_path) {
        Ok(meta) => {
            if !meta.is_file() || meta.file_type().is_symlink() {
                bail!("previous SVG owner is not a regular Markdown file");
            }
            let canonical = fs::canonicalize(&owner_path)?;
            if !canonical.starts_with(root) {
                bail!("previous SVG owner is outside the project");
            }
            // Case-only renames can refer to the same file on case-insensitive disks.
            if owner.to_lowercase() == key.to_lowercase() && same_file(&canonical, &root.join(key))?
            {
                return Ok(());
            }
            let previous = fs::read_to_string(&canonical)?;
            let diagrams = document::discover(&previous, &owner)
                .context("cannot confirm that the previous document released this diagram ID")?;
            if diagrams.iter().any(|diagram| diagram.id == id) {
                bail!(
                    "diagram ID {id} still belongs to {owner}; restore the copied file and remove its ID marker to create a separate diagram"
                );
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    Ok(())
}

fn same_file(left: &Path, right: &Path) -> Result<bool> {
    if fs::canonicalize(left)? == fs::canonicalize(right)? {
        return Ok(true);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let left = fs::metadata(left)?;
        let right = fs::metadata(right)?;
        Ok(left.dev() == right.dev() && left.ino() == right.ino())
    }
    #[cfg(not(unix))]
    Ok(false)
}

fn verify_unchanged(change: &Change) -> Result<()> {
    let current = match fs::read(&change.path) {
        Ok(bytes) => Some(bytes),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(error.into()),
    };
    if current != change.original {
        bail!(
            "{} changed while planning; no files have been written. Run the command again",
            change.path.display()
        );
    }
    Ok(())
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    static SERIAL: AtomicU64 = AtomicU64::new(0);
    let parent = path.parent().context("output has no parent directory")?;
    fs::create_dir_all(parent).with_context(|| format!("cannot create {}", parent.display()))?;
    let serial = SERIAL.fetch_add(1, Ordering::Relaxed);
    let temporary = parent.join(format!(".mer-md-{}-{serial}.tmp", std::process::id()));
    // Open before entering cleanup: never delete a path we failed to create.
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .with_context(|| format!("cannot create temporary output for {}", path.display()))?;
    let result = (|| -> Result<()> {
        file.write_all(bytes)?;
        file.sync_all()?;
        if let Ok(metadata) = fs::metadata(path) {
            fs::set_permissions(&temporary, metadata.permissions())?;
        }
        drop(file);
        fs::rename(&temporary, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result.with_context(|| format!("cannot replace {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recursive_scan_is_case_insensitive_and_skips_dependencies_and_symlinks() {
        let dir = tempfile::tempdir().unwrap();
        for name in [
            "README.md",
            "guide.MARKDOWN",
            "nested/a.md",
            "node_modules/b.md",
            ".hidden/c.md",
            "target/d.md",
            "docs/diagrams/e.md",
            "plain.txt",
        ] {
            let path = dir.path().join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, "text").unwrap();
        }
        assert_eq!(collect_markdown(dir.path()).unwrap().len(), 3);
    }

    #[test]
    fn changed_input_is_detected_before_commit() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("README.md");
        fs::write(&path, "original").unwrap();
        let change = Change {
            path: path.clone(),
            bytes: b"replacement".to_vec(),
            original: Some(b"original".to_vec()),
        };
        assert!(verify_unchanged(&change).is_ok());
        fs::write(&path, "an editor changed this").unwrap();
        assert!(verify_unchanged(&change).is_err());
        assert_eq!(fs::read_to_string(path).unwrap(), "an editor changed this");
    }
}
