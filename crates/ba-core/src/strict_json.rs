use std::collections::BTreeSet;
use std::fmt;
use std::path::Path;

use serde::de::{DeserializeSeed, MapAccess, SeqAccess, Visitor};

use crate::CoreError;
use crate::error::MAX_JSON_DEPTH;
use crate::schema::{
    DocumentKind, REWARD_SCHEDULE_DOCUMENT_TYPE, RULESET_DOCUMENT_TYPE, SCENARIO_DOCUMENT_TYPE,
    SCHEMA_VERSION_V1, SCHEMA_VERSION_V2,
};

pub use crate::document::BufferedDocument;

pub use crate::schema::DocumentDispatch;

pub(crate) fn scan_dispatch(path: &Path, bytes: &[u8]) -> Result<DocumentDispatch, CoreError> {
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    let capture = RootSeed
        .deserialize(&mut deserializer)
        .and_then(|capture| {
            deserializer.end()?;
            Ok(capture)
        })
        .map_err(|error| {
            let line = u64::try_from(error.line()).ok().filter(|value| *value != 0);
            let column = line.and_then(|_| u64::try_from(error.column()).ok());
            CoreError::InvalidJson {
                path: path.to_path_buf(),
                message: format!("duplicate/depth scan failed: {error}"),
                pointer: None,
                line,
                column,
            }
        })?;

    let kind = match capture.document_type.as_deref() {
        Some(RULESET_DOCUMENT_TYPE) => Some(DocumentKind::Ruleset),
        Some(REWARD_SCHEDULE_DOCUMENT_TYPE) => Some(DocumentKind::RewardSchedule),
        Some(SCENARIO_DOCUMENT_TYPE) => Some(DocumentKind::Scenario),
        _ => None,
    };
    match (capture.schema_version, kind) {
        (Some(version @ (SCHEMA_VERSION_V1 | SCHEMA_VERSION_V2)), Some(kind)) => {
            Ok(DocumentDispatch {
                schema_version: version,
                kind,
            })
        }
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
