use std::collections::BTreeMap;
use std::fmt::Write as _;

use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::CoreError;

pub const SEMANTIC_ENCODING_VERSION: &str = "canonical-json-v1";

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum CanonicalNode {
    Null,
    Bool(bool),
    Integer(u64),
    String(String),
    Array(Vec<Self>),
    Object(BTreeMap<String, Self>),
}

impl CanonicalNode {
    pub fn to_canonical_bytes(&self) -> Result<Vec<u8>, CoreError> {
        let mut output = Vec::new();
        write_node(self, &mut output)?;
        Ok(output)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SemanticFingerprint([u8; 32]);

impl SemanticFingerprint {
    #[must_use]
    pub fn from_canonical_bytes(bytes: &[u8]) -> Self {
        let digest: [u8; 32] = Sha256::digest(bytes).into();
        Self(digest)
    }

    pub fn from_node(node: &CanonicalNode) -> Result<Self, CoreError> {
        let bytes = node.to_canonical_bytes()?;
        Ok(Self::from_canonical_bytes(&bytes))
    }

    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    #[must_use]
    pub fn to_hex(self) -> String {
        let mut output = String::with_capacity(64);
        for byte in self.0 {
            let _ = write!(output, "{byte:02x}");
        }
        output
    }
}

impl std::fmt::Display for SemanticFingerprint {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.to_hex())
    }
}

impl Serialize for SemanticFingerprint {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_hex())
    }
}

fn write_node(node: &CanonicalNode, output: &mut Vec<u8>) -> Result<(), CoreError> {
    match node {
        CanonicalNode::Null => output.extend_from_slice(b"null"),
        CanonicalNode::Bool(value) => {
            output.extend_from_slice(if *value { b"true" } else { b"false" });
        }
        CanonicalNode::Integer(value) => output.extend_from_slice(value.to_string().as_bytes()),
        CanonicalNode::String(value) => write_string(value, output),
        CanonicalNode::Array(values) => {
            output.push(b'[');
            for (index, value) in values.iter().enumerate() {
                if index != 0 {
                    output.push(b',');
                }
                write_node(value, output)?;
            }
            output.push(b']');
        }
        CanonicalNode::Object(values) => {
            output.push(b'{');
            for (index, (key, value)) in values.iter().enumerate() {
                if index != 0 {
                    output.push(b',');
                }
                write_string(key, output);
                output.push(b':');
                write_node(value, output)?;
            }
            output.push(b'}');
        }
    }
    Ok(())
}

fn write_string(value: &str, output: &mut Vec<u8>) {
    output.push(b'"');
    for character in value.chars() {
        match character {
            '"' => output.extend_from_slice(br#"\""#),
            '\\' => output.extend_from_slice(br"\\"),
            '\u{08}' => output.extend_from_slice(br"\b"),
            '\u{0c}' => output.extend_from_slice(br"\f"),
            '\n' => output.extend_from_slice(br"\n"),
            '\r' => output.extend_from_slice(br"\r"),
            '\t' => output.extend_from_slice(br"\t"),
            control if control <= '\u{1f}' => {
                output.extend_from_slice(format!(r"\u{:04x}", u32::from(control)).as_bytes());
            }
            other => {
                let mut buffer = [0_u8; 4];
                output.extend_from_slice(other.encode_utf8(&mut buffer).as_bytes());
            }
        }
    }
    output.push(b'"');
}

#[must_use]
pub fn object<const N: usize>(entries: [(&str, CanonicalNode); N]) -> CanonicalNode {
    CanonicalNode::Object(
        entries
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value))
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::{CanonicalNode, SemanticFingerprint, object};

    #[test]
    fn fixed_writer_vector() {
        let node = object([
            ("a", CanonicalNode::Integer(1)),
            (
                "b",
                CanonicalNode::Array(vec![
                    CanonicalNode::String("x".to_owned()),
                    CanonicalNode::Integer(2),
                ]),
            ),
            (
                "c",
                object([("y", CanonicalNode::Bool(true)), ("z", CanonicalNode::Null)]),
            ),
        ]);
        let bytes = node.to_canonical_bytes().unwrap();
        assert_eq!(bytes, br#"{"a":1,"b":["x",2],"c":{"y":true,"z":null}}"#);
        assert_eq!(
            SemanticFingerprint::from_canonical_bytes(&bytes).to_hex(),
            "ef7399b9e14e5bc9393892927aff176ede3c1416d3af75cc0e44eaa6312a133d"
        );
    }
}
