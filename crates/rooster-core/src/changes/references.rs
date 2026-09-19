use super::{files::*, types::*};
use pulldown_cmark::{Event, LinkType, Parser, Tag};
use std::path::{Component, Path, PathBuf};

/// Only unambiguous inline Markdown destinations are rewritten; prose/definitions remain diagnostic.
pub(super) fn repair(
    text: &str,
    source: &Path,
    destination: &Path,
    old: &Path,
    new: &Path,
) -> Result<(String, Vec<String>)> {
    let mut replacements = vec![];
    let mut warnings = vec![];
    for (event, range) in Parser::new(text).into_offset_iter() {
        let Event::Start(
            Tag::Link {
                link_type,
                dest_url,
                ..
            }
            | Tag::Image {
                link_type,
                dest_url,
                ..
            },
        ) = event
        else {
            continue;
        };
        let url = dest_url.as_ref();
        if url.is_empty() || url.starts_with('#') || url.starts_with('/') || url.contains(':') {
            continue;
        }
        let part = url.split(['#', '?']).next().unwrap_or("");
        let decoded = percent_encoding::percent_decode_str(part)
            .decode_utf8()
            .map_err(|_| invalid("Non-UTF-8 URL needs manual repair"))?;
        let target =
            crate::artifacts::links::lexical(&source.parent().unwrap().join(decoded.as_ref()));
        let target = if target.starts_with(old) {
            new.join(target.strip_prefix(old).unwrap())
        } else {
            target
        };
        let new_url = format!(
            "{}{}",
            relative_url(destination.parent().unwrap(), &target)?,
            &url[part.len()..]
        );
        if new_url == url {
            continue;
        }
        let span = &text[range.clone()];
        if link_type != LinkType::Inline || url.contains('\\') {
            warnings.push(format!(
                "Manual link repair required in {}: non-inline or escaped destination",
                source.display()
            ));
            continue;
        }
        // Locate the literal destination after ](, preserving title and surrounding bytes.
        let Some(open) = label_end(span) else {
            warnings.push("Manual repair required for complex link syntax".into());
            continue;
        };
        let tail = &span[open + 2..];
        let leading = tail.len() - tail.trim_start().len();
        let angle = tail[leading..].starts_with('<');
        let start = range.start + open + 2 + leading + usize::from(angle);
        if text.get(start..start + url.len()) != Some(url) {
            warnings.push("Manual repair required for an encoded or ambiguous link".into());
            continue;
        }
        replacements.push((start, start + url.len(), new_url));
    }
    replacements.sort_by_key(|r| r.0);
    let mut last = 0;
    for (start, end, _) in &replacements {
        if *start < last {
            return Err(invalid("Overlapping Markdown link spans"));
        }
        last = *end;
    }
    let mut result = text.to_string();
    for (start, end, url) in replacements.into_iter().rev() {
        result.replace_range(start..end, &url);
    }
    Ok((result, warnings))
}
// Find the end of the outer label, never a delimiter in its destination/title.
// Nested inline images and code/HTML labels need a richer source map; leave those
// outer links diagnostic rather than guessing at a literal occurrence.
fn label_end(span: &str) -> Option<usize> {
    let bytes = span.as_bytes();
    let start = usize::from(bytes.first() == Some(&b'!'));
    if bytes.get(start) != Some(&b'[') {
        return None;
    }
    let mut depth = 1;
    let mut i = start + 1;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 1,
            b'`' | b'<' => return None,
            b'[' => depth += 1,
            b']' => {
                depth -= 1;
                if depth == 0 {
                    return (bytes.get(i + 1) == Some(&b'(')).then_some(i);
                }
                if bytes.get(i + 1) == Some(&b'(') {
                    return None;
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}
fn relative_url(base: &Path, target: &Path) -> Result<String> {
    let a: Vec<_> = base.components().collect();
    let b: Vec<_> = target.components().collect();
    let common = a.iter().zip(&b).take_while(|(x, y)| x == y).count();
    if common == 0 {
        return Err(invalid("Cross-volume references need manual repair"));
    }
    let mut relative = PathBuf::new();
    for _ in common..a.len() {
        relative.push("..");
    }
    for c in &b[common..] {
        relative.push(c.as_os_str());
    }
    let mut parts = vec![];
    for part in relative.components() {
        match part {
            Component::ParentDir => parts.push("..".into()),
            Component::Normal(p) => parts.push(
                percent_encoding::utf8_percent_encode(
                    p.to_str()
                        .ok_or_else(|| invalid("Non-UTF-8 destination needs manual repair"))?,
                    percent_encoding::NON_ALPHANUMERIC,
                )
                .to_string(),
            ),
            _ => return Err(invalid("Unsupported relative URL")),
        }
    }
    Ok(parts.join("/"))
}
