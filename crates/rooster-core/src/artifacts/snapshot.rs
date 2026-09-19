use super::{DocumentSnapshot, FileIdentity, SnapshotInfo};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, Metadata, OpenOptions},
    io::{self, Read},
    path::Path,
    time::UNIX_EPOCH,
};

fn identity(metadata: &Metadata) -> FileIdentity {
    #[cfg(unix)]
    let (device, inode) = {
        use std::os::unix::fs::MetadataExt;
        (Some(metadata.dev()), Some(metadata.ino()))
    };
    #[cfg(not(unix))]
    let (device, inode) = (None, None);
    FileIdentity {
        device,
        inode,
        size: metadata.len(),
        modified_nanos: metadata
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .and_then(|d| d.as_nanos().try_into().ok()),
    }
}

pub(super) fn read(path: &Path, limit: u64) -> io::Result<DocumentSnapshot> {
    let before = fs::symlink_metadata(path)?;
    if !before.file_type().is_file() {
        return Err(io::Error::other("not a regular, non-symlink file"));
    }
    if before.len() > limit {
        return Err(io::Error::other("snapshot byte limit exceeded"));
    }
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let mut file = options.open(path)?;
    if identity(&before) != identity(&file.metadata()?) {
        return Err(io::Error::other("file changed before snapshot"));
    }
    let mut bytes = Vec::new();
    (&mut file)
        .take(limit.saturating_add(1))
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(io::Error::other("snapshot byte limit exceeded"));
    }
    let after = fs::symlink_metadata(path)?;
    if identity(&before) != identity(&after)
        || identity(&before) != identity(&file.metadata()?)
        || bytes.len() as u64 != before.len()
    {
        return Err(io::Error::other("file changed while reading snapshot"));
    }
    let text = String::from_utf8(bytes.clone()).ok();
    let crlf = bytes.windows(2).filter(|pair| *pair == b"\r\n").count();
    let lf = bytes.iter().filter(|b| **b == b'\n').count();
    let info = SnapshotInfo {
        sha256: format!("{:x}", Sha256::digest(&bytes)),
        identity: identity(&before),
        utf8: text.is_some(),
        has_bom: bytes.starts_with(&[0xef, 0xbb, 0xbf]),
        newline: if lf == 0 {
            "none"
        } else if crlf == lf {
            "crlf"
        } else if crlf == 0 {
            "lf"
        } else {
            "mixed"
        }
        .into(),
    };
    Ok(DocumentSnapshot { info, bytes, text })
}

pub(super) fn stable_id(prefix: &str, path: &Path) -> String {
    format!(
        "{prefix}-{}",
        crate::paths::checkout_id(path).trim_start_matches("checkout-")
    )
}
