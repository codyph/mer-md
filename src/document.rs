//! Preserve editable Mermaid source while replacing fences with SVG images.

use std::collections::HashSet;
use std::ops::Range;

use anyhow::{Context, Result, bail, ensure};
use pulldown_cmark::{CodeBlockKind, Event, Parser, Tag, TagEnd};
use sha2::{Digest, Sha256};

const BEGIN: &str = "<!-- portable-mermaid:begin id=";
const END: &str = "<!-- portable-mermaid:end -->";
const SOURCE: &str = "<!-- portable-mermaid:source";
const SOURCE_NO_FINAL_NEWLINE: &str = "<!-- portable-mermaid:source no-final-newline";
const ID: &str = "<!-- portable-mermaid:id ";
const RESERVED: &str = "<!-- portable-mermaid:";

#[derive(Clone, Debug)]
pub struct Diagram {
    /// Persistent, filename-safe diagram identity.
    pub id: String,
    /// The source to pass to the renderer, without the Markdown fence.
    pub source: String,
    /// Exact original Markdown fence, including its original line endings.
    pub original_fence: String,
    /// The region owned by this diagram in the current Markdown document.
    pub span: Range<usize>,
    pub managed: bool,
}

#[derive(Debug)]
struct Fence {
    span: Range<usize>,
    source: String,
    mermaid: bool,
    nested: bool,
}

#[derive(Debug)]
struct HtmlSpan {
    span: Range<usize>,
    nested: bool,
}

#[derive(Debug)]
struct Identity {
    id: String,
    span: Range<usize>,
    used: bool,
}

#[derive(Debug)]
struct Managed {
    id: String,
    start: usize,
    content_start: usize,
    source_start: Option<usize>,
    source_end: Option<usize>,
    original_fence: Option<String>,
}

struct SourceComment {
    start: usize,
    body: Range<usize>,
    end: usize,
    original_fence: String,
}

/// Find fences and managed diagrams using a project-relative document key.
pub fn discover(markdown: &str, document_key: &str) -> Result<Vec<Diagram>> {
    // Raw `-->` edits must not expose stored fences to the Markdown parser.
    let (masked, sources) = mask_source_comments(markdown)?;
    let (fences, html) = parse_markdown(&masked);
    let mut diagrams = Vec::new();
    let mut identities = Vec::new();
    let mut current: Option<Managed> = None;
    let mut offset = 0;

    for line in markdown.split_inclusive('\n') {
        let line_span = offset..offset + line.len();
        offset += line.len();
        let text = line.trim_end_matches(['\r', '\n']);

        if current
            .as_ref()
            .and_then(|block| block.source_end)
            .is_some_and(|end| line_span.start < end)
        {
            continue;
        }

        let trimmed = text.trim_start_matches(' ');
        if !trimmed.starts_with(RESERVED) {
            continue;
        }
        // A marker in a code example is ordinary code, never a command.
        let content_start = line_span.start + text.len() - trimmed.len();
        let Some(html_span) = html.iter().find(|item| item.span.contains(&content_start)) else {
            continue;
        };
        ensure!(
            !html_span.nested,
            "diagram markers inside lists or blockquotes are not supported"
        );
        ensure!(
            text.len() - trimmed.len() <= 3,
            "diagram markers must be at the document's top level"
        );
        let marker = trimmed.trim_end_matches([' ', '\t']);

        if let Some(id) = marker_id(marker, BEGIN)? {
            ensure!(current.is_none(), "nested managed diagrams are invalid");
            current = Some(Managed {
                id,
                start: line_span.start,
                content_start: line_span.end,
                source_start: None,
                source_end: None,
                original_fence: None,
            });
        } else if matches!(marker, SOURCE | SOURCE_NO_FINAL_NEWLINE) {
            let block = current
                .as_mut()
                .context("source marker outside a managed diagram")?;
            ensure!(
                block.original_fence.is_none(),
                "duplicate source for diagram '{}'",
                block.id
            );
            let comment = sources
                .iter()
                .find(|comment| comment.start == line_span.start)
                .context("invalid diagram source comment")?;
            block.source_start = Some(line_span.start);
            block.original_fence = Some(comment.original_fence.clone());
            block.source_end = Some(comment.end);
        } else if marker == END {
            let block = current
                .take()
                .context("end marker without a diagram begin marker")?;
            let source_start = block
                .source_start
                .context("missing diagram source marker")?;
            let source_end = block
                .source_end
                .context("unterminated diagram source comment")?;
            ensure!(
                is_centered_image(&markdown[block.content_start..source_start])
                    && markdown[source_end..line_span.start].trim().is_empty(),
                "unexpected content inside managed diagram '{}'; refusing to overwrite it",
                block.id
            );
            let original_fence = block
                .original_fence
                .with_context(|| format!("missing source for diagram '{}'", block.id))?;
            let source = source_from_original(&original_fence)?;
            diagrams.push(Diagram {
                id: block.id,
                source,
                original_fence,
                span: block.start..line_span.end,
                managed: true,
            });
        } else if let Some(id) = marker_id(marker, ID)? {
            ensure!(
                current.is_none(),
                "identity marker inside a managed block is invalid"
            );
            identities.push(Identity {
                id,
                span: line_span,
                used: false,
            });
        } else {
            bail!("unrecognized or malformed diagram marker: {marker}");
        }
    }

    if let Some(block) = current {
        bail!("unterminated managed block for diagram '{}'", block.id);
    }

    // Reserve existing IDs before allocating IDs for newly inserted fences.
    let mut reserved = HashSet::new();
    for id in diagrams
        .iter()
        .map(|diagram| &diagram.id)
        .chain(identities.iter().map(|identity| &identity.id))
    {
        ensure!(reserved.insert(id.clone()), "duplicate diagram ID '{id}'");
    }

    for fence in fences.into_iter().filter(|fence| fence.mermaid) {
        if diagrams
            .iter()
            .any(|diagram| contains(&diagram.span, &fence.span))
        {
            continue;
        }
        ensure!(
            !fence.nested,
            "Mermaid fences inside lists or blockquotes are not supported; move the fence to the document's top level"
        );
        ensure!(
            has_closing_fence(&markdown[fence.span.clone()]),
            "Mermaid fence must have a closing Markdown fence"
        );
        let mut start = fence.span.start;
        let identity = identities.iter_mut().rev().find(|identity| {
            !identity.used
                && identity.span.end <= fence.span.start
                && markdown[identity.span.end..fence.span.start]
                    .trim()
                    .is_empty()
        });
        let id = if let Some(identity) = identity {
            identity.used = true;
            start = identity.span.start;
            identity.id.clone()
        } else {
            allocate_id(document_key, fence.span.start, &mut reserved)
        };
        diagrams.push(Diagram {
            id,
            source: fence.source,
            original_fence: markdown[fence.span.clone()].to_owned(),
            span: start..fence.span.end,
            managed: false,
        });
    }

    ensure!(
        identities.iter().all(|identity| identity.used),
        "orphan diagram identity marker; it must immediately precede a Mermaid fence"
    );
    diagrams.sort_by_key(|diagram| diagram.span.start);
    let mut ids = HashSet::new();
    let mut previous_end = 0;
    for diagram in &diagrams {
        ensure!(
            ids.insert(diagram.id.as_str()),
            "duplicate diagram ID '{}'",
            diagram.id
        );
        ensure!(
            diagram.span.start >= previous_end,
            "overlapping managed diagrams"
        );
        previous_end = diagram.span.end;
    }
    Ok(diagrams)
}

/// Replace diagrams with centred images and editable source comments.
/// Image links are relative to the Markdown file.
pub fn render_document(
    markdown: &str,
    diagrams: &[Diagram],
    image_links: &[String],
) -> Result<String> {
    ensure!(
        diagrams.len() == image_links.len(),
        "a diagram image link is missing"
    );
    let newline = newline_for(markdown);
    let mut replacements = Vec::with_capacity(diagrams.len());
    for (diagram, link) in diagrams.iter().zip(image_links) {
        ensure!(
            !link.contains(['\r', '\n']),
            "image paths cannot contain newlines"
        );
        let link = encode_image_link(link);
        let image = format!(
            "<p align=\"center\">{newline}  <img src=\"{link}\" alt=\"Mermaid diagram\" />{newline}</p>"
        );
        let readable = escape_comment_source(&diagram.original_fence);
        let source_marker = if diagram.original_fence.ends_with('\n') {
            SOURCE
        } else {
            SOURCE_NO_FINAL_NEWLINE
        };
        let separator = if diagram.original_fence.ends_with('\n') {
            ""
        } else {
            newline
        };
        let trailing = if diagram.original_fence.ends_with('\n') {
            newline
        } else {
            ""
        };
        let replacement = format!(
            "{BEGIN}{} -->{newline}{newline}{image}{newline}{newline}{source_marker}{newline}{readable}{separator}-->{newline}{END}{trailing}",
            diagram.id,
        );
        replacements.push((diagram.span.clone(), replacement));
    }
    apply_replacements(markdown, replacements)
}

/// Percent-encode a relative URL without changing its path separators.
fn encode_image_link(link: &str) -> String {
    let mut encoded = String::with_capacity(link.len());
    for byte in link.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~' | b'/') {
            encoded.push(char::from(byte));
        } else {
            use std::fmt::Write as _;
            write!(&mut encoded, "%{byte:02X}").expect("writing to a String cannot fail");
        }
    }
    encoded
}

fn escape_comment_source(source: &str) -> String {
    source
        .replace('&', "&amp;")
        .replace("<!--", "&lt;!--")
        .replace("-->", "--&gt;")
        .replace("--!>", "--!&gt;")
}

fn unescape_comment_source(source: &str) -> String {
    // Decode ampersands last to preserve literal entities.
    source
        .replace("&lt;!--", "<!--")
        .replace("--&gt;", "-->")
        .replace("--!&gt;", "--!>")
        .replace("&amp;", "&")
}

fn mask_source_comments(markdown: &str) -> Result<(String, Vec<SourceComment>)> {
    let mut masked = markdown.to_owned();
    let mut sources: Vec<SourceComment> = Vec::new();
    loop {
        let (_, html) = parse_markdown(&masked);
        let mut offset = 0;
        let mut found = Vec::new();
        for line in masked.split_inclusive('\n') {
            let start = offset;
            offset += line.len();
            let text = line.trim_end_matches(['\r', '\n']);
            let marker = text.trim_start_matches(' ').trim_end_matches([' ', '\t']);
            if !matches!(marker, SOURCE | SOURCE_NO_FINAL_NEWLINE)
                || sources.iter().any(|comment| comment.start == start)
            {
                continue;
            }
            let content_start = start + text.len() - text.trim_start_matches(' ').len();
            if !html.iter().any(|span| span.span.contains(&content_start)) {
                continue;
            }
            if found
                .iter()
                .any(|prior: &SourceComment| prior.body.contains(&start))
            {
                continue;
            }
            // Reparse after masking sources that hide later comments.
            let comment = read_source_comment(markdown, start, offset, marker)?;
            found.push(comment);
        }
        if found.is_empty() {
            break;
        }
        let mut bytes = masked.into_bytes();
        for comment in &found {
            for byte in &mut bytes[comment.body.clone()] {
                if !matches!(byte, b'\r' | b'\n') {
                    *byte = b' ';
                }
            }
        }
        masked = String::from_utf8(bytes).expect("masked UTF-8 replaces whole bodies with ASCII");
        sources.extend(found);
    }
    Ok((masked, sources))
}

fn read_source_comment(
    markdown: &str,
    start: usize,
    body_start: usize,
    marker: &str,
) -> Result<SourceComment> {
    let no_final_newline = marker == SOURCE_NO_FINAL_NEWLINE;
    let mut offset = body_start;
    let mut fence: Option<(u8, usize)> = None;
    let mut closed_fence = false;
    for line in markdown[body_start..].split_inclusive('\n') {
        let line_start = offset;
        offset += line.len();
        let text = line.trim_end_matches(['\r', '\n']);
        if text.trim() == "-->" && closed_fence {
            let stored = &markdown[body_start..line_start];
            let readable = if no_final_newline {
                stored
                    .strip_suffix("\r\n")
                    .or_else(|| stored.strip_suffix('\n'))
                    .context("source comment is missing its separator newline")?
            } else {
                stored
            };
            let original_fence = unescape_comment_source(readable);
            source_from_original(&original_fence).context("invalid stored Mermaid fence")?;
            return Ok(SourceComment {
                start,
                body: body_start..line_start,
                end: offset,
                original_fence,
            });
        }
        ensure!(
            !text.trim_start_matches(' ').starts_with(RESERVED),
            "readable source was interrupted by a managed marker before its comment closed"
        );
        ensure!(
            !closed_fence,
            "source comment must close immediately after its Mermaid fence"
        );
        let text = text.trim_start_matches(' ');
        if let Some((character, width)) = fence {
            let close_width = text.bytes().take_while(|byte| *byte == character).count();
            if close_width >= width
                && text[close_width..]
                    .bytes()
                    .all(|byte| matches!(byte, b' ' | b'\t'))
            {
                closed_fence = true;
            }
        } else {
            let character = text
                .bytes()
                .next()
                .context("readable source must start with a Mermaid fence")?;
            let width = text.bytes().take_while(|byte| *byte == character).count();
            ensure!(
                matches!(character, b'`' | b'~') && width >= 3,
                "readable source must start with a Mermaid fence"
            );
            fence = Some((character, width));
        }
    }
    bail!("unterminated diagram source comment")
}

/// Restore fences while retaining their IDs and surrounding prose.
pub fn restore_document(markdown: &str, diagrams: &[Diagram]) -> Result<String> {
    let newline = newline_for(markdown);
    let replacements = diagrams
        .iter()
        .filter(|diagram| diagram.managed)
        .map(|diagram| {
            (
                diagram.span.clone(),
                format!("{ID}{} -->{newline}{}", diagram.id, diagram.original_fence),
            )
        })
        .collect();
    apply_replacements(markdown, replacements)
}

fn marker_id(marker: &str, prefix: &str) -> Result<Option<String>> {
    let Some(rest) = marker.strip_prefix(prefix) else {
        return Ok(None);
    };
    let id = rest
        .strip_suffix(" -->")
        .context("malformed diagram identity marker")?;
    ensure!(
        !id.is_empty()
            && id.len() <= 64
            && id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_')),
        "diagram IDs must contain 1–64 letters, digits, hyphens or underscores"
    );
    Ok(Some(id.to_owned()))
}

fn new_id(document_key: &str, offset: usize) -> String {
    let mut hash = Sha256::new();
    hash.update(document_key.as_bytes());
    hash.update(b"\0");
    hash.update(offset.to_string().as_bytes());
    let bytes = hash.finalize();
    let mut id = String::from("d-");
    for byte in &bytes[..6] {
        use std::fmt::Write as _;
        write!(&mut id, "{byte:02x}").expect("writing to a String cannot fail");
    }
    id
}

fn allocate_id(document_key: &str, offset: usize, reserved: &mut HashSet<String>) -> String {
    let mut attempt = 0_u64;
    loop {
        let id = if attempt == 0 {
            new_id(document_key, offset)
        } else {
            new_id(&format!("{document_key}\0collision-{attempt}"), offset)
        };
        if reserved.insert(id.clone()) {
            return id;
        }
        attempt += 1;
    }
}

fn source_from_original(original: &str) -> Result<String> {
    let (mut fences, _) = parse_markdown(original);
    ensure!(
        fences.len() == 1,
        "stored source must contain exactly one Mermaid fence"
    );
    let fence = fences.remove(0);
    ensure!(
        fence.mermaid && !fence.nested,
        "stored source is not a top-level Mermaid fence"
    );
    ensure!(
        has_closing_fence(original),
        "stored Mermaid fence is missing its closing fence"
    );
    ensure!(
        original[..fence.span.start].trim().is_empty()
            && original[fence.span.end..].trim().is_empty(),
        "stored source contains content outside its Mermaid fence"
    );
    Ok(fence.source)
}

fn has_closing_fence(original: &str) -> bool {
    let mut lines = original.lines();
    let Some(opening) = lines.next() else {
        return false;
    };
    let opening = opening.trim_start_matches(' ');
    let Some(character @ (b'`' | b'~')) = opening.bytes().next() else {
        return false;
    };
    let width = opening
        .bytes()
        .take_while(|byte| *byte == character)
        .count();
    let Some(closing) = lines.last() else {
        return false;
    };
    let closing = closing.trim_start_matches(' ');
    let close_width = closing
        .bytes()
        .take_while(|byte| *byte == character)
        .count();
    close_width >= width && closing[close_width..].bytes().all(|byte| byte == b' ')
}

fn parse_markdown(markdown: &str) -> (Vec<Fence>, Vec<HtmlSpan>) {
    let mut fences = Vec::new();
    let mut html = Vec::new();
    let mut nested = 0_usize;
    let mut pending: Option<Fence> = None;
    for (event, range) in Parser::new(markdown).into_offset_iter() {
        match event {
            Event::Start(Tag::BlockQuote(_) | Tag::List(_) | Tag::Item) => nested += 1,
            Event::End(TagEnd::BlockQuote(_) | TagEnd::List(_) | TagEnd::Item) => {
                nested = nested.saturating_sub(1)
            }
            Event::Start(Tag::CodeBlock(kind)) => {
                let mermaid = match kind {
                    CodeBlockKind::Fenced(info) => info
                        .split_whitespace()
                        .next()
                        .is_some_and(|language| language.eq_ignore_ascii_case("mermaid")),
                    CodeBlockKind::Indented => false,
                };
                pending = Some(Fence {
                    span: normalize_fence_span(markdown, range),
                    source: String::new(),
                    mermaid,
                    nested: nested != 0,
                });
            }
            Event::Text(text) if pending.is_some() => {
                pending
                    .as_mut()
                    .expect("checked above")
                    .source
                    .push_str(&text);
            }
            Event::End(TagEnd::CodeBlock) => {
                if let Some(mut fence) = pending.take() {
                    fence.span.end = fence
                        .span
                        .end
                        .max(normalize_fence_span(markdown, range).end);
                    fences.push(fence);
                }
            }
            Event::Html(_) | Event::InlineHtml(_) => html.push(HtmlSpan {
                span: range,
                nested: nested != 0,
            }),
            _ => {}
        }
    }
    (fences, html)
}

fn normalize_fence_span(markdown: &str, range: Range<usize>) -> Range<usize> {
    let line_start = markdown[..range.start]
        .rfind('\n')
        .map_or(0, |index| index + 1);
    let prefix = &markdown[line_start..range.start];
    let start = if prefix.len() <= 3 && prefix.bytes().all(|byte| byte == b' ') {
        line_start
    } else {
        range.start
    };
    // Include the closing line's whitespace, but not the following blank line.
    let mut end = range.end;
    if !markdown[..end].ends_with('\n') {
        let suffix = &markdown[end..];
        let spaces = suffix
            .bytes()
            .take_while(|byte| matches!(byte, b' ' | b'\t'))
            .count();
        let rest = &suffix[spaces..];
        if rest.starts_with("\r\n") {
            end += spaces + 2;
        } else if rest.starts_with('\n') {
            end += spaces + 1;
        } else if rest.is_empty() {
            end += spaces;
        }
    }
    start..end
}

/// Reject added content or attributes rather than discard them on rerender.
fn is_centered_image(markdown: &str) -> bool {
    let markup = markdown.trim();
    let Ok(document) = roxmltree::Document::parse(markup) else {
        return false;
    };
    let paragraph = document.root_element();
    if paragraph.range() != (0..markup.len())
        || paragraph.tag_name().name() != "p"
        || paragraph.namespaces().len() != 0
        || paragraph.attributes().len() != 1
        || paragraph.attribute("align") != Some("center")
    {
        return false;
    }
    let mut children = paragraph.children().filter(|child| {
        !(child.is_text() && child.text().is_some_and(|text| text.trim().is_empty()))
    });
    let Some(image) = children.next() else {
        return false;
    };
    children.next().is_none()
        && image.is_element()
        && image.tag_name().name() == "img"
        && image.namespaces().len() == 0
        && image.attributes().len() == 2
        && image.attribute("src").is_some_and(|src| !src.is_empty())
        && image.attribute("alt") == Some("Mermaid diagram")
        && image.children().next().is_none()
}

fn newline_for(markdown: &str) -> &str {
    if markdown
        .find('\n')
        .is_some_and(|index| index > 0 && markdown.as_bytes()[index - 1] == b'\r')
    {
        "\r\n"
    } else {
        "\n"
    }
}

fn contains(outer: &Range<usize>, inner: &Range<usize>) -> bool {
    outer.start <= inner.start && inner.end <= outer.end
}

fn apply_replacements(markdown: &str, replacements: Vec<(Range<usize>, String)>) -> Result<String> {
    let mut result = String::with_capacity(markdown.len());
    let mut cursor = 0;
    for (range, replacement) in replacements {
        ensure!(
            range.start >= cursor
                && range.end >= range.start
                && range.end <= markdown.len()
                && markdown.is_char_boundary(range.start)
                && markdown.is_char_boundary(range.end),
            "invalid or overlapping diagram replacement ranges"
        );
        result.push_str(&markdown[cursor..range.start]);
        result.push_str(&replacement);
        cursor = range.end;
    }
    result.push_str(&markdown[cursor..]);
    Ok(result)
}

#[cfg(test)]
mod tests;
