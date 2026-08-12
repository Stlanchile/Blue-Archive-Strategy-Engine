use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

use crate::CoreError;
use crate::error::{
    MAX_CATALOG_DIRECTORY_ENTRIES, MAX_CATALOG_ENTRIES, MAX_DOCUMENT_BYTES, ObservedSize,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NodeKind {
    RegularFile,
    Directory,
    Symlink,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MetadataSnapshot {
    device: u64,
    inode: u64,
    mode: u32,
    link_count: u64,
    size: i64,
    modification_seconds: i64,
    modification_nanoseconds: u64,
    change_seconds: i64,
    change_nanoseconds: u64,
    kind: NodeKind,
}

impl MetadataSnapshot {
    #[must_use]
    pub(crate) const fn kind(&self) -> NodeKind {
        self.kind
    }

    #[must_use]
    pub(crate) fn same_identity(&self, other: &Self) -> bool {
        self.device == other.device && self.inode == other.inode && self.kind == other.kind
    }

    #[must_use]
    pub(crate) fn length(&self) -> Option<u64> {
        u64::try_from(self.size).ok()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DirectoryEntrySnapshot {
    name: OsString,
    metadata: MetadataSnapshot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FileReadStage {
    OpenedBeforeRead,
    ReadBeforePostMetadata,
}

impl DirectoryEntrySnapshot {
    #[must_use]
    pub(crate) fn name(&self) -> &OsStr {
        &self.name
    }

    #[must_use]
    pub(crate) const fn metadata(&self) -> &MetadataSnapshot {
        &self.metadata
    }
}

#[cfg(any(target_os = "android", target_os = "linux"))]
mod platform {
    use std::fs::File;
    use std::io::{Read, Take};
    use std::os::fd::{AsFd, OwnedFd};
    use std::os::unix::ffi::{OsStrExt, OsStringExt};

    use rustix::fs::{AtFlags, Dir, FileType, Mode, OFlags, Stat, fstat, open, openat, statat};
    use rustix::io::Errno;

    use super::{
        CoreError, DirectoryEntrySnapshot, MAX_CATALOG_DIRECTORY_ENTRIES, MAX_CATALOG_ENTRIES,
        MAX_DOCUMENT_BYTES, MetadataSnapshot, NodeKind, ObservedSize, OsStr, OsString, Path,
        PathBuf,
    };

    #[derive(Debug)]
    pub(crate) struct PinnedDirectory {
        descriptor: OwnedFd,
        display_path: PathBuf,
        opened_metadata: MetadataSnapshot,
    }

    impl PinnedDirectory {
        pub(crate) fn open_ambient(path: &Path) -> Result<Self, CoreError> {
            let descriptor = open(
                path,
                OFlags::RDONLY | OFlags::CLOEXEC | OFlags::DIRECTORY | OFlags::NONBLOCK,
                Mode::empty(),
            )
            .map_err(|source| io_error(path, source))?;
            let opened_metadata =
                snapshot(&fstat(&descriptor).map_err(|source| io_error(path, source))?);
            if opened_metadata.kind() != NodeKind::Directory {
                return Err(CoreError::PathPolicy {
                    path: path.to_path_buf(),
                    message: "selected ambient root must resolve to a directory".to_owned(),
                });
            }
            Ok(Self {
                descriptor,
                display_path: path.to_path_buf(),
                opened_metadata,
            })
        }

        #[must_use]
        pub(crate) fn display_path(&self) -> &Path {
            &self.display_path
        }

        #[must_use]
        pub(crate) const fn opened_metadata(&self) -> &MetadataSnapshot {
            &self.opened_metadata
        }

        pub(crate) fn inspect(&self, name: &OsStr) -> Result<MetadataSnapshot, CoreError> {
            let display = self.display_path.join(name);
            statat(&self.descriptor, name, AtFlags::SYMLINK_NOFOLLOW)
                .map(|value| snapshot(&value))
                .map_err(|source| io_error(&display, source))
        }

        pub(crate) fn open_child_directory(
            &self,
            name: &OsStr,
            inspected: &MetadataSnapshot,
        ) -> Result<Self, CoreError> {
            let display = self.display_path.join(name);
            if inspected.kind() != NodeKind::Directory {
                return Err(CoreError::PathPolicy {
                    path: display,
                    message: "catalog path must be a non-symlink directory".to_owned(),
                });
            }
            let descriptor = openat(
                &self.descriptor,
                name,
                OFlags::RDONLY
                    | OFlags::CLOEXEC
                    | OFlags::DIRECTORY
                    | OFlags::NOFOLLOW
                    | OFlags::NONBLOCK,
                Mode::empty(),
            )
            .map_err(|source| {
                generation_or_io_error(
                    &display,
                    source,
                    "catalog directory changed between inspection and open",
                )
            })?;
            let opened_metadata =
                snapshot(&fstat(&descriptor).map_err(|source| io_error(&display, source))?);
            if opened_metadata.kind() != NodeKind::Directory
                || !opened_metadata.same_identity(inspected)
            {
                return Err(CoreError::CatalogGenerationChanged {
                    path: display,
                    message: "catalog directory changed identity between inspection and open"
                        .to_owned(),
                });
            }
            Ok(Self {
                descriptor,
                display_path: display,
                opened_metadata,
            })
        }

        pub(crate) fn verify_unchanged(&self) -> Result<(), CoreError> {
            let current = snapshot(
                &fstat(&self.descriptor).map_err(|source| io_error(&self.display_path, source))?,
            );
            if current == self.opened_metadata {
                Ok(())
            } else {
                Err(CoreError::CatalogGenerationChanged {
                    path: self.display_path.clone(),
                    message: "pinned directory metadata changed during catalog assembly".to_owned(),
                })
            }
        }

        pub(crate) fn verify_child_identity(
            &self,
            name: &OsStr,
            pinned: &Self,
        ) -> Result<(), CoreError> {
            let display = self.display_path.join(name);
            let current = statat(&self.descriptor, name, AtFlags::SYMLINK_NOFOLLOW)
                .map(|value| snapshot(&value))
                .map_err(|source| {
                    generation_or_io_error(
                        &display,
                        source,
                        "catalog child changed before final identity verification",
                    )
                })?;
            if current.same_identity(pinned.opened_metadata()) {
                Ok(())
            } else {
                Err(CoreError::CatalogGenerationChanged {
                    path: display,
                    message: "catalog child name no longer identifies the pinned directory"
                        .to_owned(),
                })
            }
        }

        pub(crate) fn enumerate_catalog(&self) -> Result<Vec<DirectoryEntrySnapshot>, CoreError> {
            let mut directory = Dir::read_from(&self.descriptor)
                .map_err(|source| io_error(&self.display_path, source))?;
            let mut entries = Vec::new();
            let mut json_candidates = 0_usize;
            while let Some(item) = directory.read() {
                let item = item.map_err(|source| io_error(&self.display_path, source))?;
                let raw = item.file_name().to_bytes();
                if raw == b"." || raw == b".." {
                    continue;
                }
                let observed = entries.len() + 1;
                if observed > MAX_CATALOG_DIRECTORY_ENTRIES {
                    return Err(CoreError::CatalogDirectoryEntryLimitExceeded {
                        directory: self.display_path.clone(),
                        observed,
                        maximum: MAX_CATALOG_DIRECTORY_ENTRIES,
                    });
                }
                let name = OsString::from_vec(raw.to_vec());
                if has_json_extension(&name) {
                    json_candidates += 1;
                    if json_candidates > MAX_CATALOG_ENTRIES {
                        return Err(CoreError::CatalogEntryLimitExceeded {
                            directory: self.display_path.clone(),
                            observed: json_candidates,
                            maximum: MAX_CATALOG_ENTRIES,
                        });
                    }
                }
                let display = self.display_path.join(&name);
                let metadata = statat(&self.descriptor, &name, AtFlags::SYMLINK_NOFOLLOW)
                    .map(|value| snapshot(&value))
                    .map_err(|source| {
                        generation_or_io_error(
                            &display,
                            source,
                            "catalog entry changed during directory enumeration",
                        )
                    })?;
                entries.push(DirectoryEntrySnapshot { name, metadata });
            }
            entries.sort_by(|left, right| left.name.as_bytes().cmp(right.name.as_bytes()));
            Ok(entries)
        }

        pub(crate) fn read_candidate(
            &self,
            candidate: &DirectoryEntrySnapshot,
        ) -> Result<Vec<u8>, CoreError> {
            let display = self.display_path.join(candidate.name());
            if candidate.metadata().kind() != NodeKind::RegularFile {
                return Err(CoreError::PathPolicy {
                    path: display,
                    message: "a .json catalog entry must be a non-symlink regular file".to_owned(),
                });
            }
            self.read_relative_file(candidate.name(), Some(candidate.metadata()), &display)
        }

        pub(crate) fn read_relative_document(
            &self,
            name: &OsStr,
            display: &Path,
        ) -> Result<Vec<u8>, CoreError> {
            let inspected = self.inspect(name)?;
            if inspected.kind() != NodeKind::RegularFile {
                return Err(CoreError::PathPolicy {
                    path: display.to_path_buf(),
                    message: "JSON documents must be non-symlink regular files".to_owned(),
                });
            }
            self.read_relative_file(name, Some(&inspected), display)
        }

        fn read_relative_file(
            &self,
            name: &OsStr,
            inspected: Option<&MetadataSnapshot>,
            display: &Path,
        ) -> Result<Vec<u8>, CoreError> {
            self.read_relative_file_observed(name, inspected, display, |_| {})
        }

        fn read_relative_file_observed(
            &self,
            name: &OsStr,
            inspected: Option<&MetadataSnapshot>,
            display: &Path,
            mut observer: impl FnMut(super::FileReadStage),
        ) -> Result<Vec<u8>, CoreError> {
            if inspected
                .and_then(MetadataSnapshot::length)
                .is_some_and(|length| length > MAX_DOCUMENT_BYTES)
            {
                return Err(CoreError::DocumentSizeLimitExceeded {
                    path: display.to_path_buf(),
                    observed: ObservedSize::Exact(
                        inspected
                            .and_then(MetadataSnapshot::length)
                            .unwrap_or_default(),
                    ),
                    maximum: MAX_DOCUMENT_BYTES,
                });
            }

            let descriptor = openat(
                &self.descriptor,
                name,
                OFlags::RDONLY | OFlags::CLOEXEC | OFlags::NOFOLLOW | OFlags::NONBLOCK,
                Mode::empty(),
            )
            .map_err(|source| {
                generation_or_io_error(
                    display,
                    source,
                    "JSON source changed between inspection and open",
                )
            })?;
            let before = snapshot(&fstat(&descriptor).map_err(|source| io_error(display, source))?);
            if before.kind() != NodeKind::RegularFile {
                return Err(CoreError::PathPolicy {
                    path: display.to_path_buf(),
                    message: "opened JSON source is not a regular file".to_owned(),
                });
            }
            if inspected.is_some_and(|expected| !before.same_identity(expected)) {
                return Err(CoreError::CatalogGenerationChanged {
                    path: display.to_path_buf(),
                    message: "JSON source changed identity between inspection and open".to_owned(),
                });
            }
            let length = before.length().ok_or_else(|| CoreError::PathPolicy {
                path: display.to_path_buf(),
                message: "opened JSON source reported an invalid negative size".to_owned(),
            })?;
            if length > MAX_DOCUMENT_BYTES {
                return Err(CoreError::DocumentSizeLimitExceeded {
                    path: display.to_path_buf(),
                    observed: ObservedSize::Exact(length),
                    maximum: MAX_DOCUMENT_BYTES,
                });
            }

            observer(super::FileReadStage::OpenedBeforeRead);
            let file = File::from(descriptor);
            let mut limited: Take<File> = file.take(MAX_DOCUMENT_BYTES + 1);
            let mut bytes = Vec::with_capacity(
                usize::try_from(length).unwrap_or(usize::try_from(MAX_DOCUMENT_BYTES).unwrap_or(0)),
            );
            limited
                .read_to_end(&mut bytes)
                .map_err(|source| CoreError::Io {
                    path: display.to_path_buf(),
                    source,
                })?;
            let file = limited.into_inner();
            observer(super::FileReadStage::ReadBeforePostMetadata);
            let after = snapshot(&fstat(file.as_fd()).map_err(|source| io_error(display, source))?);
            if before != after {
                return Err(CoreError::CatalogGenerationChanged {
                    path: display.to_path_buf(),
                    message: "JSON source metadata changed while it was being read".to_owned(),
                });
            }
            let observed =
                u64::try_from(bytes.len()).map_err(|_| CoreError::ArithmeticOverflow {
                    context: "converting buffered document length",
                })?;
            if observed > MAX_DOCUMENT_BYTES {
                return Err(CoreError::DocumentSizeLimitExceeded {
                    path: display.to_path_buf(),
                    observed: ObservedSize::GreaterThan(MAX_DOCUMENT_BYTES),
                    maximum: MAX_DOCUMENT_BYTES,
                });
            }
            Ok(bytes)
        }

        #[cfg(test)]
        pub(crate) fn read_candidate_observed(
            &self,
            candidate: &DirectoryEntrySnapshot,
            observer: impl FnMut(super::FileReadStage),
        ) -> Result<Vec<u8>, CoreError> {
            let display = self.display_path.join(candidate.name());
            self.read_relative_file_observed(
                candidate.name(),
                Some(candidate.metadata()),
                &display,
                observer,
            )
        }
    }

    pub(crate) fn read_document_path(path: &Path) -> Result<Vec<u8>, CoreError> {
        let file_name = path.file_name().ok_or_else(|| CoreError::PathPolicy {
            path: path.to_path_buf(),
            message: "JSON document path must end in a file name".to_owned(),
        })?;
        let parent = path
            .parent()
            .filter(|value| !value.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let pinned_parent = PinnedDirectory::open_ambient(parent)?;
        pinned_parent.read_relative_document(file_name, path)
    }

    pub(crate) fn is_json_candidate(entry: &DirectoryEntrySnapshot) -> bool {
        has_json_extension(entry.name())
    }

    fn has_json_extension(name: &OsStr) -> bool {
        Path::new(name).extension() == Some(OsStr::new("json"))
    }

    fn io_error(path: &Path, source: rustix::io::Errno) -> CoreError {
        CoreError::Io {
            path: path.to_path_buf(),
            source: std::io::Error::from_raw_os_error(source.raw_os_error()),
        }
    }

    fn generation_or_io_error(path: &Path, source: Errno, message: &str) -> CoreError {
        if matches!(source, Errno::LOOP | Errno::NOENT | Errno::NOTDIR) {
            CoreError::CatalogGenerationChanged {
                path: path.to_path_buf(),
                message: message.to_owned(),
            }
        } else {
            io_error(path, source)
        }
    }

    #[allow(clippy::unnecessary_cast)] // rustix Stat field widths vary across supported Unix ABIs.
    fn snapshot(value: &Stat) -> MetadataSnapshot {
        MetadataSnapshot {
            device: value.st_dev as u64,
            inode: value.st_ino as u64,
            mode: value.st_mode as u32,
            link_count: value.st_nlink as u64,
            size: value.st_size as i64,
            modification_seconds: value.st_mtime as i64,
            modification_nanoseconds: value.st_mtime_nsec as u64,
            change_seconds: value.st_ctime as i64,
            change_nanoseconds: value.st_ctime_nsec as u64,
            kind: match FileType::from_raw_mode(value.st_mode) {
                FileType::RegularFile => NodeKind::RegularFile,
                FileType::Directory => NodeKind::Directory,
                FileType::Symlink => NodeKind::Symlink,
                _ => NodeKind::Other,
            },
        }
    }
}

#[cfg(any(target_os = "android", target_os = "linux"))]
pub(crate) use platform::{PinnedDirectory, is_json_candidate, read_document_path};

#[cfg(not(any(target_os = "android", target_os = "linux")))]
mod platform_stub {
    use super::{CoreError, DirectoryEntrySnapshot, MetadataSnapshot, OsStr, Path, PathBuf};

    #[derive(Debug)]
    pub(crate) struct PinnedDirectory {
        display_path: PathBuf,
    }

    impl PinnedDirectory {
        pub(crate) fn open_ambient(path: &Path) -> Result<Self, CoreError> {
            Err(unsupported(path))
        }

        #[must_use]
        pub(crate) fn display_path(&self) -> &Path {
            &self.display_path
        }

        #[must_use]
        pub(crate) fn opened_metadata(&self) -> &MetadataSnapshot {
            unreachable!("unsupported secure filesystem target")
        }

        pub(crate) fn inspect(&self, _name: &OsStr) -> Result<MetadataSnapshot, CoreError> {
            Err(unsupported(&self.display_path))
        }

        pub(crate) fn open_child_directory(
            &self,
            _name: &OsStr,
            _inspected: &MetadataSnapshot,
        ) -> Result<Self, CoreError> {
            Err(unsupported(&self.display_path))
        }

        pub(crate) fn verify_unchanged(&self) -> Result<(), CoreError> {
            Err(unsupported(&self.display_path))
        }

        pub(crate) fn verify_child_identity(
            &self,
            _name: &OsStr,
            _pinned: &Self,
        ) -> Result<(), CoreError> {
            Err(unsupported(&self.display_path))
        }

        pub(crate) fn enumerate_catalog(&self) -> Result<Vec<DirectoryEntrySnapshot>, CoreError> {
            Err(unsupported(&self.display_path))
        }

        pub(crate) fn read_candidate(
            &self,
            _candidate: &DirectoryEntrySnapshot,
        ) -> Result<Vec<u8>, CoreError> {
            Err(unsupported(&self.display_path))
        }
    }

    pub(crate) fn read_document_path(path: &Path) -> Result<Vec<u8>, CoreError> {
        Err(unsupported(path))
    }

    pub(crate) fn is_json_candidate(_entry: &DirectoryEntrySnapshot) -> bool {
        false
    }

    fn unsupported(path: &Path) -> CoreError {
        CoreError::PathPolicy {
            path: path.to_path_buf(),
            message: "secure descriptor-relative JSON loading is unsupported on this target"
                .to_owned(),
        }
    }
}

#[cfg(not(any(target_os = "android", target_os = "linux")))]
pub(crate) use platform_stub::{PinnedDirectory, is_json_candidate, read_document_path};

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::sync::mpsc::sync_channel;
    use std::thread;

    use tempfile::TempDir;

    use super::{FileReadStage, PinnedDirectory, is_json_candidate};
    use crate::CoreError;

    #[test]
    fn observable_in_place_change_during_read_is_rejected() {
        let temp = TempDir::new().expect("tempdir");
        let source = temp.path().join("candidate.json");
        fs::write(&source, b"{}").expect("source");
        let directory = PinnedDirectory::open_ambient(temp.path()).expect("directory");
        let entries = directory.enumerate_catalog().expect("entries");
        let candidate = entries
            .iter()
            .find(|entry| is_json_candidate(entry))
            .expect("candidate");
        let mutation_path: PathBuf = source;
        let (trigger, wait) = sync_channel::<()>(0);
        let (finished, done) = sync_channel::<()>(0);
        let result = thread::scope(|scope| {
            scope.spawn(move || {
                wait.recv().expect("trigger");
                fs::write(&mutation_path, b"{\"changed\":true}").expect("mutate in place");
                finished.send(()).expect("finished");
            });
            directory.read_candidate_observed(candidate, |stage| {
                if stage == FileReadStage::ReadBeforePostMetadata {
                    trigger.send(()).expect("start mutation");
                    done.recv().expect("mutation done");
                }
            })
        });
        assert!(matches!(
            result,
            Err(CoreError::CatalogGenerationChanged { .. })
        ));
    }
}
