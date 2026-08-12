use std::path::PathBuf;

use thiserror::Error;

pub const MAX_CATALOG_ENTRIES: usize = 256;
pub const MAX_CATALOG_DIRECTORY_ENTRIES: usize = 512;
pub const MAX_DOCUMENT_BYTES: u64 = 1_048_576;
pub const MAX_JSON_DEPTH: usize = 64;

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("I/O failure for {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("path policy rejected {path}: {message}")]
    PathPolicy { path: PathBuf, message: String },

    #[error("catalog entry limit exceeded in {directory}: observed {observed}, maximum {maximum}")]
    CatalogEntryLimitExceeded {
        directory: PathBuf,
        observed: usize,
        maximum: usize,
    },

    #[error(
        "catalog directory entry limit exceeded in {directory}: observed at least {observed}, maximum {maximum}"
    )]
    CatalogDirectoryEntryLimitExceeded {
        directory: PathBuf,
        observed: usize,
        maximum: usize,
    },

    #[error("catalog generation changed while loading {path}: {message}")]
    CatalogGenerationChanged { path: PathBuf, message: String },

    #[error(
        "document size limit exceeded for {path}: observed {observed}, maximum {maximum} bytes"
    )]
    DocumentSizeLimitExceeded {
        path: PathBuf,
        observed: ObservedSize,
        maximum: u64,
    },

    #[error("invalid JSON document {path}: {message}")]
    InvalidJson {
        path: PathBuf,
        message: String,
        pointer: Option<String>,
        line: Option<u64>,
        column: Option<u64>,
    },

    #[error(
        "unsupported document dispatch in {path}: schema_version={schema_version:?}, document_type={document_type:?}"
    )]
    UnsupportedDocument {
        path: PathBuf,
        schema_version: Option<u64>,
        document_type: Option<String>,
    },

    #[error(
        "scenario schema version {scenario_schema_version} cannot reference {referenced_kind} {referenced_id} with schema version {referenced_schema_version}"
    )]
    IncompatibleSchemaReference {
        scenario_schema_version: u64,
        referenced_kind: &'static str,
        referenced_id: String,
        referenced_schema_version: u64,
        pointer: &'static str,
    },

    #[error("validation failed{path_suffix}: {message}")]
    Validation {
        path_suffix: String,
        message: String,
    },

    #[error("arithmetic overflow while {context}")]
    ArithmeticOverflow { context: &'static str },

    #[error("invalid action: {message}")]
    InvalidAction { message: String },

    #[error("invalid transition: {message}")]
    InvalidTransition { message: String },

    #[error("internal invariant violation: {message}")]
    InternalInvariant { message: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoreErrorClass {
    Validation,
    CatalogIo,
    Engine,
    Internal,
}

impl CoreError {
    #[must_use]
    pub fn validation(path: Option<&std::path::Path>, message: impl Into<String>) -> Self {
        let path_suffix = path.map_or_else(String::new, |value| {
            format!(" for {}", value.to_string_lossy())
        });
        Self::Validation {
            path_suffix,
            message: message.into(),
        }
    }

    #[must_use]
    pub const fn class(&self) -> CoreErrorClass {
        match self {
            Self::Io { .. }
            | Self::PathPolicy { .. }
            | Self::CatalogEntryLimitExceeded { .. }
            | Self::CatalogDirectoryEntryLimitExceeded { .. }
            | Self::CatalogGenerationChanged { .. }
            | Self::DocumentSizeLimitExceeded { .. } => CoreErrorClass::CatalogIo,
            Self::InvalidJson { .. }
            | Self::UnsupportedDocument { .. }
            | Self::IncompatibleSchemaReference { .. }
            | Self::Validation { .. } => CoreErrorClass::Validation,
            Self::ArithmeticOverflow { .. }
            | Self::InvalidAction { .. }
            | Self::InvalidTransition { .. } => CoreErrorClass::Engine,
            Self::InternalInvariant { .. } => CoreErrorClass::Internal,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObservedSize {
    Exact(u64),
    GreaterThan(u64),
}

impl std::fmt::Display for ObservedSize {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Exact(value) => write!(formatter, "{value} bytes"),
            Self::GreaterThan(value) => write!(formatter, "more than {value} bytes"),
        }
    }
}
