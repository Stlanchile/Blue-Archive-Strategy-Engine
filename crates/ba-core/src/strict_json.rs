use std::collections::BTreeSet;
use std::fmt;
#[cfg(any(target_os = "android", target_os = "linux"))]
use std::fs::OpenOptions;
use std::fs::{File, Metadata};
use std::io::{Read, Take};
use std::path::{Path, PathBuf};

#[cfg(any(target_os = "android", target_os = "linux"))]
use std::os::unix::fs::MetadataExt;
#[cfg(any(target_os = "android", target_os = "linux"))]
use std::os::unix::fs::OpenOptionsExt;

use serde::de::{DeserializeOwned, DeserializeSeed, MapAccess, SeqAccess, Visitor};

use crate::CoreError;
use crate::error::{MAX_DOCUMENT_BYTES, MAX_JSON_DEPTH, ObservedSize};
use crate::schema::{
    DocumentKind, REWARD_SCHEDULE_DOCUMENT_TYPE, RULESET_DOCUMENT_TYPE, SCENARIO_DOCUMENT_TYPE,
    SCHEMA_VERSION_V1,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentDispatch {
    pub schema_version: u64,
    pub kind: DocumentKind,
}

#[derive(Debug)]
pub struct BufferedDocument {
    path: PathBuf,
    bytes: Vec<u8>,
    dispatch: DocumentDispatch,
}

impl BufferedDocument {
    pub fn read(path: impl AsRef<Path>) -> Result<Self, CoreError> {
        let path = path.as_ref();
        let bytes = read_complete_bounded(path)?;
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
    pub fn dispatch(&self) -> &DocumentDispatch {
        &self.dispatch
    }

    pub fn parse_typed<T: DeserializeOwned>(&self) -> Result<T, CoreError> {
        serde_json::from_slice(&self.bytes).map_err(|error| CoreError::InvalidJson {
            path: self.path.clone(),
            message: format!("strict typed parse failed: {error}"),
        })
    }
}

fn read_complete_bounded(path: &Path) -> Result<Vec<u8>, CoreError> {
    let link_metadata = std::fs::symlink_metadata(path).map_err(|source| CoreError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    if link_metadata.file_type().is_symlink() || !link_metadata.file_type().is_file() {
        return Err(CoreError::PathPolicy {
            path: path.to_path_buf(),
            message: "JSON documents must be non-symlink regular files".to_owned(),
        });
    }
    if link_metadata.len() > MAX_DOCUMENT_BYTES {
        return Err(CoreError::DocumentSizeLimitExceeded {
            path: path.to_path_buf(),
            observed: ObservedSize::Exact(link_metadata.len()),
            maximum: MAX_DOCUMENT_BYTES,
        });
    }

    let file = open_verified_regular_file(path, &link_metadata)?;
    let opened_metadata = file.metadata().map_err(|source| CoreError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    if opened_metadata.len() > MAX_DOCUMENT_BYTES {
        return Err(CoreError::DocumentSizeLimitExceeded {
            path: path.to_path_buf(),
            observed: ObservedSize::Exact(opened_metadata.len()),
            maximum: MAX_DOCUMENT_BYTES,
        });
    }
    let mut limited: Take<File> = file.take(MAX_DOCUMENT_BYTES + 1);
    let mut bytes = Vec::with_capacity(
        usize::try_from(opened_metadata.len())
            .unwrap_or(usize::try_from(MAX_DOCUMENT_BYTES).unwrap_or(0)),
    );
    limited
        .read_to_end(&mut bytes)
        .map_err(|source| CoreError::Io {
            path: path.to_path_buf(),
            source,
        })?;
    let observed = u64::try_from(bytes.len()).map_err(|_| CoreError::ArithmeticOverflow {
        context: "converting buffered document length",
    })?;
    if observed > MAX_DOCUMENT_BYTES {
        return Err(CoreError::DocumentSizeLimitExceeded {
            path: path.to_path_buf(),
            observed: ObservedSize::GreaterThan(MAX_DOCUMENT_BYTES),
            maximum: MAX_DOCUMENT_BYTES,
        });
    }
    Ok(bytes)
}

#[cfg(any(target_os = "android", target_os = "linux"))]
fn open_verified_regular_file(path: &Path, expected: &Metadata) -> Result<File, CoreError> {
    let mut options = OpenOptions::new();
    options.read(true);
    options.custom_flags(0o00_400_000 | 0o00_004_000);

    let file = options.open(path).map_err(|source| CoreError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let opened = file.metadata().map_err(|source| CoreError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    if !opened.file_type().is_file() {
        return Err(CoreError::PathPolicy {
            path: path.to_path_buf(),
            message: "opened JSON source is not a regular file".to_owned(),
        });
    }
    if !same_file_identity(expected, &opened) {
        return Err(CoreError::PathPolicy {
            path: path.to_path_buf(),
            message: "JSON source changed identity between inspection and open".to_owned(),
        });
    }
    Ok(file)
}

#[cfg(not(any(target_os = "android", target_os = "linux")))]
fn open_verified_regular_file(path: &Path, _expected: &Metadata) -> Result<File, CoreError> {
    Err(CoreError::PathPolicy {
        path: path.to_path_buf(),
        message: "secure no-follow, nonblocking JSON opens are unsupported on this v0.1 target"
            .to_owned(),
    })
}

#[cfg(any(target_os = "android", target_os = "linux"))]
fn same_file_identity(expected: &Metadata, opened: &Metadata) -> bool {
    expected.dev() == opened.dev() && expected.ino() == opened.ino()
}

fn scan_dispatch(path: &Path, bytes: &[u8]) -> Result<DocumentDispatch, CoreError> {
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    let capture = RootSeed
        .deserialize(&mut deserializer)
        .and_then(|capture| {
            deserializer.end()?;
            Ok(capture)
        })
        .map_err(|error| CoreError::InvalidJson {
            path: path.to_path_buf(),
            message: format!("duplicate/depth scan failed: {error}"),
        })?;

    let kind = match capture.document_type.as_deref() {
        Some(RULESET_DOCUMENT_TYPE) => Some(DocumentKind::Ruleset),
        Some(REWARD_SCHEDULE_DOCUMENT_TYPE) => Some(DocumentKind::RewardSchedule),
        Some(SCENARIO_DOCUMENT_TYPE) => Some(DocumentKind::Scenario),
        _ => None,
    };
    match (capture.schema_version, kind) {
        (Some(SCHEMA_VERSION_V1), Some(kind)) => Ok(DocumentDispatch {
            schema_version: SCHEMA_VERSION_V1,
            kind,
        }),
        _ => Err(CoreError::UnsupportedDocument {
            path: path.to_path_buf(),
            schema_version: capture.schema_version,
            document_type: capture.document_type,
        }),
    }
}

#[derive(Debug)]
struct DispatchCapture {
    schema_version: Option<u64>,
    document_type: Option<String>,
}

struct RootSeed;

impl<'de> DeserializeSeed<'de> for RootSeed {
    type Value = DispatchCapture;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_map(RootVisitor)
    }
}

struct RootVisitor;

impl<'de> Visitor<'de> for RootVisitor {
    type Value = DispatchCapture;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("one top-level JSON object")
    }

    fn visit_map<M>(self, mut map: M) -> Result<Self::Value, M::Error>
    where
        M: MapAccess<'de>,
    {
        let mut keys = BTreeSet::new();
        let mut schema_version = None;
        let mut document_type = None;
        while let Some(key) = map.next_key::<String>()? {
            if !keys.insert(key.clone()) {
                return Err(serde::de::Error::custom(format!(
                    "duplicate object key {key:?}"
                )));
            }
            match key.as_str() {
                "schema_version" => schema_version = Some(map.next_value::<u64>()?),
                "document_type" => document_type = Some(map.next_value::<String>()?),
                _ => map.next_value_seed(ScanSeed { depth: 2 })?,
            }
        }
        Ok(DispatchCapture {
            schema_version,
            document_type,
        })
    }
}

#[derive(Clone, Copy)]
struct ScanSeed {
    depth: usize,
}

impl<'de> DeserializeSeed<'de> for ScanSeed {
    type Value = ();

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_any(ScanVisitor { depth: self.depth })
    }
}

struct ScanVisitor {
    depth: usize,
}

impl ScanVisitor {
    fn ensure_depth<E: serde::de::Error>(&self) -> Result<(), E> {
        if self.depth > MAX_JSON_DEPTH {
            Err(E::custom(format!(
                "JSON nesting depth {} exceeds maximum {MAX_JSON_DEPTH}",
                self.depth
            )))
        } else {
            Ok(())
        }
    }
}

impl<'de> Visitor<'de> for ScanVisitor {
    type Value = ();

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a JSON value")
    }

    fn visit_bool<E>(self, _value: bool) -> Result<(), E> {
        Ok(())
    }

    fn visit_i64<E>(self, _value: i64) -> Result<(), E> {
        Ok(())
    }

    fn visit_u64<E>(self, _value: u64) -> Result<(), E> {
        Ok(())
    }

    fn visit_f64<E>(self, _value: f64) -> Result<(), E> {
        Ok(())
    }

    fn visit_str<E>(self, _value: &str) -> Result<(), E> {
        Ok(())
    }

    fn visit_string<E>(self, _value: String) -> Result<(), E> {
        Ok(())
    }

    fn visit_none<E>(self) -> Result<(), E> {
        Ok(())
    }

    fn visit_unit<E>(self) -> Result<(), E> {
        Ok(())
    }

    fn visit_some<D>(self, deserializer: D) -> Result<(), D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        ScanSeed { depth: self.depth }.deserialize(deserializer)
    }

    fn visit_seq<A>(self, mut sequence: A) -> Result<(), A::Error>
    where
        A: SeqAccess<'de>,
    {
        self.ensure_depth()?;
        while sequence
            .next_element_seed(ScanSeed {
                depth: self.depth + 1,
            })?
            .is_some()
        {}
        Ok(())
    }

    fn visit_map<M>(self, mut map: M) -> Result<(), M::Error>
    where
        M: MapAccess<'de>,
    {
        self.ensure_depth()?;
        let mut keys = BTreeSet::new();
        while let Some(key) = map.next_key::<String>()? {
            if !keys.insert(key.clone()) {
                return Err(serde::de::Error::custom(format!(
                    "duplicate object key {key:?}"
                )));
            }
            map.next_value_seed(ScanSeed {
                depth: self.depth + 1,
            })?;
        }
        Ok(())
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use std::fs;
    use std::os::unix::fs::symlink;
    use std::process::Command;

    use tempfile::TempDir;

    use super::open_verified_regular_file;

    #[test]
    fn replacement_symlink_cannot_pass_the_verified_open() {
        let temp = TempDir::new().expect("tempdir");
        let source = temp.path().join("source.json");
        let moved = temp.path().join("moved.json");
        let other = temp.path().join("other.json");
        fs::write(&source, b"{}").expect("source");
        fs::write(&other, b"{}").expect("other");
        let inspected = fs::symlink_metadata(&source).expect("metadata");
        fs::rename(&source, &moved).expect("move inspected file");
        symlink(&other, &source).expect("replacement symlink");
        assert!(open_verified_regular_file(&source, &inspected).is_err());
    }

    #[test]
    fn replacement_fifo_is_opened_nonblocking_and_rejected() {
        let temp = TempDir::new().expect("tempdir");
        let source = temp.path().join("source.json");
        let moved = temp.path().join("moved.json");
        fs::write(&source, b"{}").expect("source");
        let inspected = fs::symlink_metadata(&source).expect("metadata");
        fs::rename(&source, &moved).expect("move inspected file");
        let status = Command::new("mkfifo")
            .arg(&source)
            .status()
            .expect("mkfifo executes");
        assert!(status.success());
        assert!(open_verified_regular_file(&source, &inspected).is_err());
    }
}
