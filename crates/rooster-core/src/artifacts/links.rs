use super::{Reference, ReferenceStatus};
use pulldown_cmark::{Event, Parser, Tag};
use std::{
    fs,
    path::{Component, Path, PathBuf},
};

pub(crate) fn lexical(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for part in path.components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            _ => out.push(part.as_os_str()),
        }
    }
    out
}

/// Resolve only paths inside registered boundaries; ordinary references never follow links.
pub(crate) fn bounded_path(
    path: &Path,
    roots: &[PathBuf],
    excluded: &[String],
) -> Result<PathBuf, ReferenceStatus> {
    // Normalization is only for the final containment check. Inspect the original
    // components below so `link/..` cannot conceal a symlink or missing directory.
    if !roots.iter().any(|root| lexical(path).starts_with(root)) {
        return Err(ReferenceStatus::OutsideRoots);
    }
    if path.components().any(|part| part.as_os_str() == ".git") {
        return Err(ReferenceStatus::Excluded);
    }
    let root = roots
        .iter()
        .filter(|r| path.starts_with(r))
        .max_by_key(|r| r.components().count())
        .ok_or(ReferenceStatus::OutsideRoots)?;
    let mut current = root.clone();
    let parts: Vec<_> = path.strip_prefix(root).unwrap().components().collect();
    for (index, part) in parts.iter().enumerate() {
        if part.as_os_str() == ".git"
            || (index + 1 < parts.len()
                && excluded
                    .iter()
                    .any(|name| part.as_os_str() == name.as_str()))
        {
            return Err(ReferenceStatus::Excluded);
        }
        if *part == Component::ParentDir {
            current.pop();
        } else {
            current.push(part.as_os_str());
        }
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(ReferenceStatus::UnsupportedLink);
            }
            Ok(metadata) if index + 1 < parts.len() && !metadata.is_dir() => {
                return Err(ReferenceStatus::Missing);
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Err(ReferenceStatus::Missing);
            }
            Err(_) => return Err(ReferenceStatus::UnsupportedLink),
        }
    }
    let physical = current
        .canonicalize()
        .map_err(|_| ReferenceStatus::Missing)?;
    if !roots.iter().any(|r| physical.starts_with(r)) {
        return Err(ReferenceStatus::OutsideRoots);
    }
    Ok(physical)
}

pub(crate) fn extract(text: &str) -> Vec<(String, usize)> {
    Parser::new(text)
        .into_offset_iter()
        .filter_map(|(event, range)| match event {
            Event::Start(Tag::Link { dest_url, .. } | Tag::Image { dest_url, .. }) => Some((
                dest_url.into_string(),
                text[..range.start].bytes().filter(|b| *b == b'\n').count() + 1,
            )),
            _ => None,
        })
        .collect()
}

pub(crate) fn resolve(
    destination: &str,
    line: usize,
    parent: &Path,
    roots: &[PathBuf],
    excluded: &[String],
) -> Reference {
    let mut reference = Reference {
        destination: destination.into(),
        line,
        target: None,
        target_id: None,
        status: ReferenceStatus::FragmentOnly,
        fragment_checked: false,
    };
    if destination.is_empty() || destination.starts_with('#') {
        return reference;
    }
    if destination.starts_with("//")
        || ["http:", "https:", "mailto:"]
            .iter()
            .any(|scheme| destination.to_ascii_lowercase().starts_with(scheme))
    {
        reference.status = ReferenceStatus::Remote;
        reference.destination = redact_remote(destination);
        return reference;
    }
    if destination
        .split(['/', '\\', '#', '?'])
        .next()
        .is_some_and(|part| part.contains(':'))
    {
        reference.status = ReferenceStatus::UnsupportedScheme;
        return reference;
    }
    let path_part = destination.split(['#', '?']).next().unwrap_or("");
    let bytes: Vec<_> = percent_encoding::percent_decode_str(path_part).collect();
    if bytes.contains(&0) {
        reference.status = ReferenceStatus::UnsupportedScheme;
        return reference;
    }
    let Ok(path) = crate::paths::git_path(&bytes) else {
        reference.status = ReferenceStatus::UnsupportedScheme;
        return reference;
    };
    let target = if path.is_absolute() {
        path
    } else {
        parent.join(path)
    };
    reference.target = Some(target.clone().into());
    match bounded_path(&target, roots, excluded) {
        Ok(physical) => {
            reference.target = Some(physical.into());
            reference.status = ReferenceStatus::Resolved;
        }
        Err(status) => reference.status = status,
    }
    reference
}

fn redact_remote(destination: &str) -> String {
    let mut display = destination.to_string();
    if let Some(scheme_end) = display
        .find("://")
        .map(|i| i + 3)
        .or_else(|| display.starts_with("//").then_some(2))
    {
        let authority_end = display[scheme_end..]
            .find(['/', '?', '#'])
            .map(|i| scheme_end + i)
            .unwrap_or(display.len());
        if let Some(at) = display[scheme_end..authority_end].rfind('@') {
            display.replace_range(scheme_end..scheme_end + at + 1, "[redacted]@");
        }
    }
    if let Some(query) = display.find('?') {
        display.truncate(query);
        display.push_str("?[redacted]");
    }
    display
}

/// Claude @imports are literal filesystem paths, without URL decoding or fragments.
pub(crate) fn resolve_import(
    destination: &str,
    line: usize,
    parent: &Path,
    roots: &[PathBuf],
    excluded: &[String],
) -> Reference {
    let path = Path::new(destination);
    let target = if path.is_absolute() {
        path.to_path_buf()
    } else {
        parent.join(path)
    };
    let mut reference = Reference {
        destination: format!("@{destination}"),
        line,
        target: Some(target.clone().into()),
        target_id: None,
        status: ReferenceStatus::Missing,
        fragment_checked: false,
    };
    match bounded_path(&target, roots, excluded) {
        Ok(physical) => {
            reference.target = Some(physical.into());
            reference.status = ReferenceStatus::Resolved;
        }
        Err(status) => reference.status = status,
    }
    reference
}
