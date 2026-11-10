//! Put native fallback labels inside their owning nodes and edges.

use anyhow::{Context, Result, bail};
use roxmltree::{Document, Node};
use std::collections::HashMap;
use std::ops::Range;

pub fn expand(svg: &str) -> Result<String> {
    let document = Document::parse(svg).context("invalid SVG during label expansion")?;
    let prefix = format!(
        "{}-foreignobject-label-",
        document.root_element().attribute("id").unwrap_or("mer-md")
    );
    let definitions: HashMap<_, _> = document
        .descendants()
        .filter(|node| node.has_tag_name("g"))
        .filter(|node| {
            node.parent_element()
                .is_some_and(|parent| parent.has_tag_name("defs"))
        })
        .filter_map(|node| {
            let id = node.attribute("id")?;
            let number = id.strip_prefix(&prefix)?;
            (!number.is_empty() && number.bytes().all(|byte| byte.is_ascii_digit()))
                .then_some((id, node))
        })
        .collect();
    let mut edits: Vec<(Range<usize>, String)> = Vec::new();
    let mut expanded = HashMap::new();
    for instance in document
        .descendants()
        .filter(|node| node.has_tag_name("use"))
    {
        if instance.ancestors().any(|node| node.has_tag_name("defs"))
            || !instance.ancestors().any(|node| {
                node.attribute("data-merman-foreignobject") == Some("fallback")
                    || node.attribute("class").is_some_and(|classes| {
                        classes
                            .split_ascii_whitespace()
                            .any(|class| class == "merman-foreignobject-fallback")
                    })
            })
        {
            continue;
        }
        let href = instance
            .attribute("href")
            .or_else(|| instance.attribute(("http://www.w3.org/1999/xlink", "href")));
        let Some((id, target)) = href
            .and_then(|value| value.strip_prefix('#'))
            .and_then(|id| definitions.get(id).map(|target| (id, *target)))
        else {
            continue;
        };
        let x = coordinate(instance, "x")?;
        let y = coordinate(instance, "y")?;
        let mut content = svg[target.range()].to_owned();
        let id_attribute = target.attribute_node("id").unwrap().range();
        content.replace_range(
            id_attribute.start - target.range().start..id_attribute.end - target.range().start,
            "",
        );
        if x != 0.0 || y != 0.0 {
            content = format!("<g transform=\"translate({x} {y})\">{content}</g>");
        }
        let (opening, name) = instance_group(svg, instance)?;
        edits.push((instance.range(), format!("{opening}{content}</{name}>")));
        *expanded.entry(id).or_insert(0usize) += 1;
    }
    for (id, count) in expanded {
        let reference = format!("#{id}");
        let references = document
            .descendants()
            .filter(|node| {
                node.is_element()
                    && node.attributes().any(|attribute| {
                        attribute.name() == "href" && attribute.value() == reference
                    })
            })
            .count();
        let other_reference = document.descendants().any(|node| {
            node.attributes().any(|attribute| {
                attribute.name() != "href" && has_url_reference(attribute.value(), &reference)
            }) || node.has_tag_name("style")
                && node
                    .text()
                    .is_some_and(|text| has_url_reference(text, &reference))
        });
        if references == count && !other_reference {
            let target = definitions[id];
            let parent = target.parent_element().unwrap();
            let remove = if parent.children().filter(Node::is_element).count() == 1
                && parent.children().all(|child| {
                    child.is_element()
                        || child.is_text()
                            && child.text().is_some_and(|text| text.trim().is_empty())
                }) {
                parent.range()
            } else {
                target.range()
            };
            edits.push((remove, String::new()));
        }
    }
    edits.sort_by_key(|(range, _)| std::cmp::Reverse(range.start));
    let mut result = svg.to_owned();
    for (range, replacement) in edits {
        result.replace_range(range, &replacement);
    }
    Ok(result)
}

fn has_url_reference(value: &str, reference: &str) -> bool {
    [
        format!("url({reference})"),
        format!("url('{reference}')"),
        format!("url(\"{reference}\")"),
    ]
    .iter()
    .any(|pattern| value.contains(pattern))
}

fn coordinate(node: Node<'_, '_>, name: &str) -> Result<f64> {
    let value = node.attribute(name).unwrap_or("0");
    let number: f64 = value
        .trim()
        .trim_end_matches("px")
        .parse()
        .with_context(|| format!("invalid native label {name} coordinate: {value}"))?;
    if !number.is_finite() {
        bail!("nonfinite native label {name} coordinate");
    }
    Ok(number)
}

fn instance_group(svg: &str, instance: Node<'_, '_>) -> Result<(String, String)> {
    let start = instance.range().start;
    let mut quote = None;
    let end = svg.as_bytes()[start..]
        .iter()
        .enumerate()
        .find_map(|(offset, byte)| {
            match (quote, byte) {
                (Some(current), byte) if current == *byte => quote = None,
                (None, b'\'' | b'"') => quote = Some(*byte),
                (None, b'>') => return Some(start + offset + 1),
                _ => {}
            }
            None
        })
        .context("unterminated native label reference")?;
    let mut opening = svg[start..end].to_owned();
    let name_end = opening[1..]
        .find(|character: char| character.is_ascii_whitespace() || matches!(character, '/' | '>'))
        .unwrap()
        + 1;
    let old_name = &opening[1..name_end];
    let name = old_name
        .rsplit_once(':')
        .map_or_else(|| "g".to_owned(), |(prefix, _)| format!("{prefix}:g"));
    let mut edits = vec![(1..name_end, name.clone())];
    for attribute in instance
        .attributes()
        .filter(|attribute| matches!(attribute.name(), "href" | "x" | "y"))
    {
        let range = attribute.range();
        edits.push((range.start - start..range.end - start, String::new()));
    }
    if opening.as_bytes().get(opening.len() - 2) == Some(&b'/') {
        edits.push((opening.len() - 2..opening.len() - 1, String::new()));
    }
    edits.sort_by_key(|(range, _)| std::cmp::Reverse(range.start));
    for (range, replacement) in edits {
        opening.replace_range(range, &replacement);
    }
    Ok((opening, name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_generated_labels_become_independent_visible_text() {
        let svg = r##"<svg id="diagram"><defs><g id="diagram-foreignobject-label-0" style="font-size:16px"><svg x="0" y="0" width="80" height="24" viewBox="0 0 80 24"><text fill="#123456">A &amp; B</text></svg></g></defs><g class="node" id="first" style="color:red"><g data-merman-foreignobject="fallback"><use href="#diagram-foreignobject-label-0"/></g></g><g class="node" id="second" style="color:blue"><g data-merman-foreignobject="fallback"><use href="#diagram-foreignobject-label-0"/></g></g></svg>"##;
        let result = expand(svg).unwrap();
        let document = Document::parse(&result).unwrap();
        assert_eq!(
            document
                .descendants()
                .filter(|node| node.has_tag_name("text"))
                .count(),
            2
        );
        for owner in ["first", "second"] {
            let node = document
                .descendants()
                .find(|node| node.attribute("id") == Some(owner))
                .unwrap();
            let text = node
                .descendants()
                .find(|node| node.has_tag_name("text"))
                .unwrap();
            assert_eq!(text.text(), Some("A & B"));
            assert_eq!(text.attribute("fill"), Some("#123456"));
            assert!(
                text.ancestors()
                    .any(|node| node.attribute("style") == Some("font-size:16px"))
            );
        }
        assert!(!result.contains("diagram-foreignobject-label-0"));
    }

    #[test]
    fn retains_instance_transforms_styles_and_nested_references() {
        let svg = r##"<svg id="diagram" xmlns:xlink="http://www.w3.org/1999/xlink"><defs><g id="diagram-foreignobject-label-0"><defs><path id="glyph" d="M0 0L2 2"/></defs><use href="#glyph"/><text>Label</text></g></defs><g class="merman-foreignobject-fallback"><use x="5" y="-2" xlink:href="#diagram-foreignobject-label-0" transform="scale(2)" style="opacity:0.5"/></g><use href="#glyph"/></svg>"##;
        let result = expand(svg).unwrap();
        let document = Document::parse(&result).unwrap();
        let text = document
            .descendants()
            .find(|node| node.has_tag_name("text"))
            .unwrap();
        assert!(
            text.ancestors()
                .any(|node| node.attribute("transform") == Some("translate(5 -2)"))
        );
        assert!(
            text.ancestors()
                .any(|node| node.attribute("transform") == Some("scale(2)")
                    && node.attribute("style") == Some("opacity:0.5"))
        );
        assert!(
            document
                .descendants()
                .any(|node| node.attribute("id") == Some("glyph"))
        );
        assert_eq!(
            document
                .descendants()
                .filter(|node| node.has_tag_name("use") && node.attribute("href") == Some("#glyph"))
                .count(),
            2
        );
    }

    #[test]
    fn keeps_definitions_needed_by_unexpanded_author_references() {
        let svg = r##"<svg id="diagram"><defs><g id="diagram-foreignobject-label-0"><text>Label</text></g><path id="icon" d="M0 0L1 1"/></defs><g data-merman-foreignobject="fallback"><use href="#diagram-foreignobject-label-0"/></g><use href="#diagram-foreignobject-label-0"/><use href="#icon"/></svg>"##;
        let result = expand(svg).unwrap();
        let document = Document::parse(&result).unwrap();
        assert!(
            document
                .descendants()
                .any(|node| node.attribute("id") == Some("diagram-foreignobject-label-0"))
        );
        assert_eq!(
            document
                .descendants()
                .filter(|node| node.has_tag_name("use"))
                .count(),
            2
        );
        assert!(result.contains(r##"<use href="#icon"/>"##));
    }

    #[test]
    fn unrelated_svg_references_are_unchanged() {
        let svg = r##"<svg id="diagram"><defs><g id="author-label"><text>Label</text></g></defs><g class="merman-foreignobject-fallback"><use href="#author-label"/></g></svg>"##;
        assert_eq!(expand(svg).unwrap(), svg);
    }
}
