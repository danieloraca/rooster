use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::path::{Path, PathBuf};

/// JSON uses a string for Unicode paths and a lossless platform encoding otherwise.
/// The display form is never used to reconstruct a filesystem path.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct NativePath(pub PathBuf);

#[derive(Serialize, Deserialize)]
#[serde(untagged, deny_unknown_fields)]
enum EncodedPath {
    Text(String),
    Unix { unix_bytes: Vec<u8> },
    Windows { windows_wide: Vec<u16> },
}

impl NativePath {
    pub fn resolve(path: impl AsRef<Path>) -> std::io::Result<Self> {
        absolute_input(path.as_ref()).map(Self)
    }
    pub fn as_path(&self) -> &Path {
        &self.0
    }
}

impl From<PathBuf> for NativePath {
    fn from(path: PathBuf) -> Self {
        Self(path)
    }
}

impl AsRef<Path> for NativePath {
    fn as_ref(&self) -> &Path {
        &self.0
    }
}

impl std::fmt::Display for NativePath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.display().fmt(f)
    }
}

impl Serialize for NativePath {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if let Some(path) = self.0.to_str() {
            return serializer.serialize_str(path);
        }
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStrExt;
            EncodedPath::Unix {
                unix_bytes: self.0.as_os_str().as_bytes().to_vec(),
            }
            .serialize(serializer)
        }
        #[cfg(windows)]
        {
            use std::os::windows::ffi::OsStrExt;
            EncodedPath::Windows {
                windows_wide: self.0.as_os_str().encode_wide().collect(),
            }
            .serialize(serializer)
        }
        #[cfg(not(any(unix, windows)))]
        {
            Err(serde::ser::Error::custom(
                "unsupported native path encoding",
            ))
        }
    }
}

impl<'de> Deserialize<'de> for NativePath {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        match EncodedPath::deserialize(deserializer)? {
            EncodedPath::Text(path) => Ok(PathBuf::from(path).into()),
            #[cfg(unix)]
            EncodedPath::Unix { unix_bytes } => {
                use std::os::unix::ffi::OsStringExt;
                Ok(PathBuf::from(std::ffi::OsString::from_vec(unix_bytes)).into())
            }
            #[cfg(windows)]
            EncodedPath::Windows { windows_wide } => {
                use std::os::windows::ffi::OsStringExt;
                Ok(PathBuf::from(std::ffi::OsString::from_wide(&windows_wide)).into())
            }
            _ => Err(serde::de::Error::custom(
                "path belongs to another operating system",
            )),
        }
    }
}

pub(crate) fn git_path(bytes: &[u8]) -> Result<PathBuf, String> {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStringExt;
        Ok(PathBuf::from(std::ffi::OsString::from_vec(bytes.to_vec())))
    }
    #[cfg(not(unix))]
    {
        String::from_utf8(bytes.to_vec())
            .map(PathBuf::from)
            .map_err(|_| "Git returned an unsupported path encoding".into())
    }
}

pub(crate) fn absolute_input(path: &Path) -> std::io::Result<PathBuf> {
    let expanded = if let Ok(tail) = path.strip_prefix("~") {
        let base = directories::BaseDirs::new().ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "cannot resolve the home directory",
            )
        })?;
        base.home_dir().join(tail)
    } else {
        path.to_path_buf()
    };
    std::path::absolute(expanded)
}

pub(crate) fn checkout_id(path: &Path) -> String {
    use sha2::{Digest, Sha256};
    let mut hash = Sha256::new();
    // as_encoded_bytes is lossless for native paths; IDs are deliberately local.
    hash.update(std::env::consts::OS.as_bytes());
    hash.update([0]);
    hash.update(path.as_os_str().as_encoded_bytes());
    format!("checkout-{:x}", hash.finalize())
}
