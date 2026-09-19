use super::types::*;
use crate::{CancellationToken, NativePath, git::Git};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{Read, Write},
    path::{Component, Path},
    time::Duration,
};
use tempfile::NamedTempFile;
pub(super) const MAX_FILE: u64 = 16 * 1024 * 1024;
pub(super) const MAX_TOTAL: usize = 64 * 1024 * 1024;
pub(super) fn invalid(message: impl Into<String>) -> Error {
    Error::Invalid(message.into())
}
pub(super) fn conflict(message: impl Into<String>) -> Error {
    Error::Conflict(message.into())
}
pub(super) fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
pub(super) fn identity(meta: &fs::Metadata) -> Result<Identity> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        Ok(Identity {
            device: meta.dev(),
            inode: meta.ino(),
        })
    }
    #[cfg(not(unix))]
    {
        let _ = meta;
        Err(invalid(
            "Mutation identity checks currently require Unix; other platforms remain read-only.",
        ))
    }
}
pub(super) fn mode(meta: &fs::Metadata) -> u32 {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        meta.permissions().mode() & 0o777
    }
    #[cfg(not(unix))]
    {
        if meta.permissions().readonly() {
            0o444
        } else {
            0o644
        }
    }
}
pub(super) fn set_mode(path: &Path, mode: u32) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(mode))?;
    }
    #[cfg(not(unix))]
    {
        let _ = (path, mode);
    }
    Ok(())
}
/// Inspect every existing component, including ancestors of a registered root.
pub(super) fn no_links(path: &Path) -> Result<()> {
    if !path.is_absolute() {
        return Err(invalid("Expected an absolute native path"));
    }
    let mut current = std::path::PathBuf::new();
    for part in path.components() {
        if matches!(part, Component::ParentDir | Component::CurDir) {
            return Err(invalid("Unnormalized path"));
        }
        current.push(part);
        match fs::symlink_metadata(&current) {
            Ok(meta) if meta.file_type().is_symlink() => {
                return Err(invalid(format!(
                    "Linked path is read-only: {}",
                    current.display()
                )));
            }
            Ok(meta) if current != path && !meta.is_dir() => {
                return Err(invalid("Ancestor is not a directory"));
            }
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
    }
    Ok(())
}
pub(super) fn relative(path: &Path) -> Result<()> {
    if path.as_os_str().is_empty()
        || path.is_absolute()
        || path.components().any(|p| {
            !matches!(p, Component::Normal(_))
                || p.as_os_str()
                    .to_str()
                    .is_some_and(|s| s.eq_ignore_ascii_case(".git"))
        })
    {
        return Err(invalid(
            "Destination must be a nonempty relative path without '.', '..', or .git",
        ));
    }
    Ok(())
}
pub(super) fn read_node(path: &Path) -> Result<Option<(Node, Vec<u8>)>> {
    no_links(path)?;
    let meta = match fs::symlink_metadata(path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    let id = identity(&meta)?;
    if meta.is_dir() {
        return Ok(Some((
            Node {
                directory: true,
                hash: None,
                size: 0,
                mode: mode(&meta),
                identity: Some(id),
            },
            vec![],
        )));
    }
    if !meta.is_file() {
        return Err(invalid(format!("Not a regular file: {}", path.display())));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if meta.nlink() != 1 {
            return Err(invalid(format!(
                "Hard-linked file is read-only: {}",
                path.display()
            )));
        }
    }
    if meta.len() > MAX_FILE {
        return Err(invalid("File exceeds mutation limit (16 MiB)"));
    }
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let mut file = options.open(path)?;
    let mut bytes = vec![];
    if identity(&file.metadata()?)? != id {
        return Err(conflict("File replaced while opening"));
    }
    (&mut file).take(MAX_FILE + 1).read_to_end(&mut bytes)?;
    let after = fs::symlink_metadata(path)?;
    if identity(&after)? != id
        || after.modified()? != meta.modified()?
        || after.len() != meta.len()
        || bytes.len() as u64 != meta.len()
        || mode(&after) != mode(&meta)
    {
        return Err(conflict("File changed while reading"));
    }
    Ok(Some((
        Node {
            directory: false,
            hash: Some(hash(&bytes)),
            size: bytes.len() as u64,
            mode: mode(&meta),
            identity: Some(id),
        },
        bytes,
    )))
}
pub(super) fn tree(path: &Path) -> Result<BTreeMap<NativePath, Node>> {
    fn walk(path: &Path, nodes: &mut BTreeMap<NativePath, Node>, size: &mut u64) -> Result<()> {
        let (node, _) = read_node(path)?.ok_or_else(|| conflict("Package disappeared"))?;
        *size += node.size;
        if *size > MAX_TOTAL as u64 || nodes.len() >= 20_000 {
            return Err(invalid("Package exceeds mutation limits"));
        }
        let dir = node.directory;
        nodes.insert(path.to_path_buf().into(), node);
        if dir {
            for entry in fs::read_dir(path)? {
                let p = entry?.path();
                if p.file_name()
                    .is_some_and(|s| s.to_str().is_some_and(|s| s.eq_ignore_ascii_case(".git")))
                {
                    return Err(invalid(
                        "Package contains Git metadata or a nested checkout",
                    ));
                }
                walk(&p, nodes, size)?;
            }
        }
        Ok(())
    }
    let mut nodes = BTreeMap::new();
    walk(path, &mut nodes, &mut 0)?;
    Ok(nodes)
}
pub(super) fn checkout(path: &Path) -> Result<CheckoutGuard> {
    let token = CancellationToken::default();
    let git = Git {
        executable: Path::new("git"),
        timeout: Duration::from_secs(5),
        cancellation: &token,
    };
    let meta = git
        .inspect(path)
        .map_err(|e| invalid(format!("Checkout unavailable: {e:?}")))?;
    Ok(CheckoutGuard {
        git_dir: meta.git_dir.into(),
        common_dir: meta.common_dir.into(),
        branch: meta.branch,
        head: meta.head,
    })
}
pub(super) fn sync_dir(path: &Path) -> Result<()> {
    File::open(path)?.sync_all()?;
    Ok(())
}
pub(super) fn atomic(path: &Path, bytes: &[u8], permissions: u32, replace: bool) -> Result<()> {
    no_links(path)?;
    let parent = path.parent().ok_or_else(|| invalid("Missing parent"))?;
    let mut temp = NamedTempFile::new_in(parent)?;
    temp.write_all(bytes)?;
    set_mode(temp.path(), permissions)?;
    temp.as_file().sync_all()?;
    if replace {
        temp.persist(path).map_err(|e| Error::Io(e.error))?;
    } else {
        temp.persist_noclobber(path)
            .map_err(|e| Error::Io(e.error))?;
    }
    sync_dir(parent)
}
