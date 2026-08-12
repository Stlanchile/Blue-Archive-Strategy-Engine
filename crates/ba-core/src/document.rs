use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;

use crate::CoreError;
use crate::fs_secure::{self, PinnedDirectory, is_json_candidate};
use crate::strict_json::{DocumentDispatch, scan_dispatch};

#[derive(Debug)]
pub struct BufferedDocument {
    path: PathBuf,
    bytes: Vec<u8>,
    dispatch: DocumentDispatch,
}

pub fn read_document_directory(
    directory: impl AsRef<Path>,
) -> Result<Vec<BufferedDocument>, CoreError> {
    let pinned = PinnedDirectory::open_ambient(directory.as_ref())?;
    let snapshot = pinned.enumerate_catalog()?;
    let mut documents = Vec::new();
    for candidate in snapshot.iter().filter(|entry| is_json_candidate(entry)) {
        let path = pinned.display_path().join(candidate.name());
        documents.push(BufferedDocument::from_bytes(
            &path,
            pinned.read_candidate(candidate)?,
        )?);
    }
    pinned.verify_unchanged()?;
    if pinned.enumerate_catalog()? != snapshot {
        return Err(CoreError::CatalogGenerationChanged {
            path: pinned.display_path().to_path_buf(),
            message: "document directory snapshot changed during loading".to_owned(),
        });
    }
    pinned.verify_unchanged()?;
    Ok(documents)
}

impl BufferedDocument {
    pub fn read(path: impl AsRef<Path>) -> Result<Self, CoreError> {
        let path = path.as_ref();
        let bytes = fs_secure::read_document_path(path)?;
        Self::from_bytes(path, bytes)
    }

    pub(crate) fn from_bytes(path: impl AsRef<Path>, bytes: Vec<u8>) -> Result<Self, CoreError> {
        let path = path.as_ref();
        let dispatch = scan_dispatch(path, &bytes)?;
        Ok(Self {
            path: path.to_path_buf(),
            bytes,
            dispatch,
        })
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    #[must_use]
    pub const fn dispatch(&self) -> &DocumentDispatch {
        &self.dispatch
    }

    pub fn parse_typed<T: DeserializeOwned>(&self) -> Result<T, CoreError> {
        let mut deserializer = serde_json::Deserializer::from_slice(&self.bytes);
        serde_path_to_error::deserialize(&mut deserializer).map_err(|error| {
            let pointer = serde_path_to_pointer(&error.path().to_string());
            let inner = error.into_inner();
            let line = u64::try_from(inner.line()).ok().filter(|value| *value != 0);
            let column = line.and_then(|_| u64::try_from(inner.column()).ok());
            CoreError::InvalidJson {
                path: self.path.clone(),
                message: format!("strict typed parse failed: {inner}"),
                pointer,
                line,
                column,
            }
        })
    }
}

fn serde_path_to_pointer(path: &str) -> Option<String> {
    if path.is_empty() || path == "." {
        return None;
    }
    let mut pointer = String::new();
    for segment in path.split('.') {
        let mut remaining = segment;
        if let Some((field, indices)) = remaining.split_once('[') {
            if !field.is_empty() {
                pointer.push('/');
                pointer.push_str(&escape_pointer(field));
            }
            remaining = indices;
            while let Some((index, rest)) = remaining.split_once(']') {
                pointer.push('/');
                pointer.push_str(index);
                let Some(next) = rest.strip_prefix('[') else {
                    break;
                };
                remaining = next;
            }
        } else if !remaining.is_empty() {
            pointer.push('/');
            pointer.push_str(&escape_pointer(remaining));
        }
    }
    (!pointer.is_empty()).then_some(pointer)
}

fn escape_pointer(value: &str) -> String {
    value.replace('~', "~0").replace('/', "~1")
}
