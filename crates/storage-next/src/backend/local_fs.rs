//! Local filesystem backend shell.

use super::{
    Backend, BackendCapabilities, BackendError, BackendErrorKind, BackendMetadata, BackendRange,
    BackendResult, BASIC_OBJECT_BACKEND_CAPABILITIES,
};
use crate::object::{ObjectName, ObjectPrefix};
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Component, Path, PathBuf};

const OBJECT_FILE_SUFFIX: &str = ".object@";

#[derive(Debug, Clone)]
pub(crate) struct LocalFsBackend {
    root: PathBuf,
}

impl LocalFsBackend {
    pub(crate) fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub(crate) fn root(&self) -> &Path {
        &self.root
    }

    fn path_for(&self, name: &ObjectName) -> PathBuf {
        let mut path = self.root.clone();
        let mut components = name.as_str().split('/').peekable();
        while let Some(component) = components.next() {
            if components.peek().is_some() {
                path.push(component);
            } else {
                path.push(format!("{component}{OBJECT_FILE_SUFFIX}"));
            }
        }
        path
    }

    fn ensure_parent_dirs(&self, parent: &Path, create_missing: bool) -> BackendResult<()> {
        self.ensure_root_dir(create_missing)?;
        let relative = parent.strip_prefix(&self.root).map_err(|_| {
            BackendError::new(
                BackendErrorKind::Corruption,
                format!("path {} escaped backend root", parent.display()),
            )
        })?;

        let mut current = self.root.clone();
        for component in relative.components() {
            let Component::Normal(part) = component else {
                return Err(BackendError::new(
                    BackendErrorKind::Corruption,
                    format!("path {} contains a non-normal component", parent.display()),
                ));
            };
            current.push(part);
            Self::ensure_dir(&current, create_missing)?;
        }

        Ok(())
    }

    fn ensure_root_dir(&self, create_missing: bool) -> BackendResult<()> {
        Self::ensure_dir(&self.root, create_missing)
    }

    fn ensure_dir(path: &Path, create_missing: bool) -> BackendResult<()> {
        match fs::symlink_metadata(path) {
            Ok(metadata) if metadata.file_type().is_symlink() => Err(BackendError::new(
                BackendErrorKind::Corruption,
                format!("directory path {} is a symlink", path.display()),
            )),
            Ok(metadata) if metadata.is_dir() => Ok(()),
            Ok(_) => Err(BackendError::new(
                BackendErrorKind::Corruption,
                format!("directory path {} is not a directory", path.display()),
            )),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound && create_missing => {
                fs::create_dir(path).map_err(|err| map_io_error(&err))?;
                Self::ensure_dir(path, false)
            }
            Err(error) => Err(map_io_error(&error)),
        }
    }

    fn metadata_for_object_path(&self, path: &Path) -> BackendResult<BackendMetadata> {
        if let Some(parent) = path.parent() {
            self.ensure_parent_dirs(parent, false)?;
        }

        let metadata = fs::symlink_metadata(path).map_err(|err| map_io_error(&err))?;
        if metadata.file_type().is_symlink() {
            return Err(BackendError::new(
                BackendErrorKind::Corruption,
                format!("object path {} is a symlink", path.display()),
            ));
        }
        if !metadata.is_file() {
            return Err(BackendError::new(
                BackendErrorKind::Corruption,
                format!("object path {} is not a file", path.display()),
            ));
        }
        Ok(BackendMetadata::new(metadata.len(), None))
    }

    fn name_from_path(&self, path: &Path) -> BackendResult<ObjectName> {
        let relative = path.strip_prefix(&self.root).map_err(|_| {
            BackendError::new(
                BackendErrorKind::Corruption,
                format!("path {} escaped backend root", path.display()),
            )
        })?;

        let mut parts = Vec::new();
        for component in relative.components() {
            let Component::Normal(part) = component else {
                return Err(BackendError::new(
                    BackendErrorKind::Corruption,
                    format!("path {} contains a non-normal component", path.display()),
                ));
            };
            let Some(part) = part.to_str() else {
                return Err(BackendError::new(
                    BackendErrorKind::Corruption,
                    format!("path {} contains non-UTF-8 data", path.display()),
                ));
            };
            parts.push(part.to_owned());
        }

        let Some(last) = parts.last_mut() else {
            return Err(BackendError::new(
                BackendErrorKind::Corruption,
                format!("path {} does not name an object file", path.display()),
            ));
        };
        let Some(stem) = last.strip_suffix(OBJECT_FILE_SUFFIX) else {
            return Err(BackendError::new(
                BackendErrorKind::Corruption,
                format!(
                    "path {} does not use the object-file suffix",
                    path.display()
                ),
            ));
        };
        *last = stem.to_owned();

        ObjectName::new(parts.join("/")).map_err(|err| {
            BackendError::new(
                BackendErrorKind::Corruption,
                format!("path {} is not a valid object name: {err}", path.display()),
            )
        })
    }

    fn collect_files(&self, dir: &Path, files: &mut Vec<ObjectName>) -> BackendResult<()> {
        match fs::read_dir(dir) {
            Ok(entries) => {
                for entry in entries {
                    let entry = entry.map_err(|err| map_io_error(&err))?;
                    let path = entry.path();
                    let file_type = entry.file_type().map_err(|err| map_io_error(&err))?;
                    if file_type.is_symlink() {
                        return Err(BackendError::new(
                            BackendErrorKind::Corruption,
                            format!("path {} is a symlink", path.display()),
                        ));
                    } else if file_type.is_dir() {
                        self.collect_files(&path, files)?;
                    } else if file_type.is_file() && is_object_file_path(&path) {
                        files.push(self.name_from_path(&path)?);
                    }
                }
                Ok(())
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(err) => Err(map_io_error(&err)),
        }
    }
}

impl Backend for LocalFsBackend {
    fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities::from_slice(BASIC_OBJECT_BACKEND_CAPABILITIES)
    }

    fn read_object(&self, name: &ObjectName) -> BackendResult<Vec<u8>> {
        let path = self.path_for(name);
        self.metadata_for_object_path(&path)?;
        fs::read(path).map_err(|err| map_io_error(&err))
    }

    fn read_range(&self, name: &ObjectName, range: BackendRange) -> BackendResult<Vec<u8>> {
        let Some(end_offset) = range.end_offset() else {
            return Err(BackendError::new(
                BackendErrorKind::InvalidRange,
                format!("range {}.. overflows for object {name}", range.offset()),
            ));
        };

        let path = self.path_for(name);
        self.metadata_for_object_path(&path)?;
        let mut file = File::open(path).map_err(|err| map_io_error(&err))?;
        file.seek(SeekFrom::Start(range.offset()))
            .map_err(|err| map_io_error(&err))?;
        let mut bytes = Vec::new();
        file.take(end_offset.saturating_sub(range.offset()))
            .read_to_end(&mut bytes)
            .map_err(|err| map_io_error(&err))?;
        Ok(bytes)
    }

    fn write_object(&self, name: &ObjectName, bytes: &[u8]) -> BackendResult<BackendMetadata> {
        let path = self.path_for(name);
        if let Some(parent) = path.parent() {
            self.ensure_parent_dirs(parent, true)?;
        }
        match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(BackendError::new(
                    BackendErrorKind::Corruption,
                    format!("object path {} is a symlink", path.display()),
                ));
            }
            Ok(metadata) if !metadata.is_file() => {
                return Err(BackendError::new(
                    BackendErrorKind::Corruption,
                    format!("object path {} is not a file", path.display()),
                ));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(map_io_error(&error)),
        }
        fs::write(&path, bytes).map_err(|err| map_io_error(&err))?;
        Ok(BackendMetadata::new(bytes.len() as u64, None))
    }

    fn delete_object(&self, name: &ObjectName) -> BackendResult<()> {
        let path = self.path_for(name);
        self.metadata_for_object_path(&path)?;
        fs::remove_file(path).map_err(|err| map_io_error(&err))
    }

    fn list_prefix(&self, prefix: &ObjectPrefix) -> BackendResult<Vec<ObjectName>> {
        match fs::symlink_metadata(&self.root) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(BackendError::new(
                    BackendErrorKind::Corruption,
                    format!("directory path {} is a symlink", self.root.display()),
                ));
            }
            Ok(metadata) if !metadata.is_dir() => {
                return Err(BackendError::new(
                    BackendErrorKind::Corruption,
                    format!("directory path {} is not a directory", self.root.display()),
                ));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(map_io_error(&error)),
        }

        let mut names = Vec::new();
        self.collect_files(&self.root, &mut names)?;
        names.retain(|name| name.as_str().starts_with(prefix.as_str()));
        names.sort();
        Ok(names)
    }

    fn object_metadata(&self, name: &ObjectName) -> BackendResult<BackendMetadata> {
        self.metadata_for_object_path(&self.path_for(name))
    }
}

fn is_object_file_path(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.ends_with(OBJECT_FILE_SUFFIX))
}

fn map_io_error(error: &std::io::Error) -> BackendError {
    let kind = match error.kind() {
        std::io::ErrorKind::NotFound => BackendErrorKind::NotFound,
        std::io::ErrorKind::AlreadyExists => BackendErrorKind::AlreadyExists,
        std::io::ErrorKind::PermissionDenied => BackendErrorKind::PermissionDenied,
        std::io::ErrorKind::Interrupted => BackendErrorKind::Interrupted,
        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut => {
            BackendErrorKind::Unavailable
        }
        std::io::ErrorKind::InvalidData | std::io::ErrorKind::UnexpectedEof => {
            BackendErrorKind::Corruption
        }
        std::io::ErrorKind::InvalidInput => BackendErrorKind::InvalidObjectName,
        _ => BackendErrorKind::Unknown,
    };
    BackendError::new(kind, error.to_string())
}

#[cfg(test)]
mod tests {
    use super::LocalFsBackend;
    use crate::backend::{
        Backend, BackendCapability, BackendErrorKind, BackendRange,
        BASIC_OBJECT_BACKEND_CAPABILITIES, CACHE_MODE_REQUIREMENTS,
    };
    use crate::object::{ObjectName, ObjectPrefix};

    #[test]
    fn localfs_backend_reports_basic_object_capabilities_only() {
        let dir = tempfile::tempdir().expect("tempdir");
        let backend = LocalFsBackend::new(dir.path());
        let capabilities = backend.capabilities();

        assert_eq!(backend.root(), dir.path());
        assert!(capabilities.supports(CACHE_MODE_REQUIREMENTS));
        assert!(capabilities.supports(BASIC_OBJECT_BACKEND_CAPABILITIES));
        assert!(!capabilities.contains(BackendCapability::DurablePublish));
        assert!(!capabilities.contains(BackendCapability::DurableSync));
        assert!(!capabilities.contains(BackendCapability::SingleWriterLock));
    }

    #[test]
    fn localfs_backend_round_trips_object_bytes_and_metadata() {
        let dir = tempfile::tempdir().expect("tempdir");
        let backend = LocalFsBackend::new(dir.path());
        let name = ObjectName::new("tables/main/object").expect("valid object name");

        let metadata = backend
            .write_object(&name, b"abcdef")
            .expect("write should succeed");

        assert_eq!(metadata.size_bytes(), 6);
        assert_eq!(backend.read_object(&name).expect("read object"), b"abcdef");
        assert_eq!(
            backend
                .read_range(&name, BackendRange::new(2, 3))
                .expect("read range"),
            b"cde"
        );
        assert_eq!(
            backend
                .object_metadata(&name)
                .expect("metadata")
                .size_bytes(),
            6
        );
    }

    #[test]
    fn localfs_backend_range_reads_are_bounded() {
        let dir = tempfile::tempdir().expect("tempdir");
        let backend = LocalFsBackend::new(dir.path());
        let name = ObjectName::new("tables/main/object").expect("valid object name");
        backend.write_object(&name, b"abc").expect("write");

        assert_eq!(
            backend
                .read_range(&name, BackendRange::new(2, 20))
                .expect("range truncates"),
            b"c"
        );
        assert_eq!(
            backend
                .read_range(&name, BackendRange::new(3, 1))
                .expect("range at end"),
            b""
        );
        assert_eq!(
            backend
                .read_range(&name, BackendRange::new(u64::MAX, 1))
                .expect_err("overflow rejected")
                .kind(),
            BackendErrorKind::InvalidRange
        );
    }

    #[test]
    fn localfs_backend_lists_prefixes_in_order() {
        let dir = tempfile::tempdir().expect("tempdir");
        let backend = LocalFsBackend::new(dir.path());
        let names = [
            ObjectName::new("tables/a/002").expect("name"),
            ObjectName::new("tables/a/001").expect("name"),
            ObjectName::new("tables/b/001").expect("name"),
        ];
        for name in &names {
            backend
                .write_object(name, name.as_str().as_bytes())
                .expect("write");
        }

        let prefix = ObjectPrefix::new("tables/a/").expect("prefix");
        let listed = backend.list_prefix(&prefix).expect("list prefix");
        let listed: Vec<_> = listed.iter().map(ObjectName::as_str).collect();

        assert_eq!(listed, vec!["tables/a/001", "tables/a/002"]);
    }

    #[test]
    fn localfs_backend_can_store_object_and_child_prefix() {
        let dir = tempfile::tempdir().expect("tempdir");
        let backend = LocalFsBackend::new(dir.path());
        let parent = ObjectName::new("tables/a").expect("parent name");
        let child = ObjectName::new("tables/a/child").expect("child name");

        backend
            .write_object(&parent, b"parent")
            .expect("parent write");
        backend.write_object(&child, b"child").expect("child write");

        assert_eq!(
            backend.read_object(&parent).expect("parent read"),
            b"parent"
        );
        assert_eq!(backend.read_object(&child).expect("child read"), b"child");

        let all = backend
            .list_prefix(&ObjectPrefix::new("").expect("all prefix"))
            .expect("list all");
        let all: Vec<_> = all.iter().map(ObjectName::as_str).collect();

        assert_eq!(all, vec!["tables/a", "tables/a/child"]);
    }

    #[test]
    fn localfs_backend_missing_paths_are_classified() {
        let dir = tempfile::tempdir().expect("tempdir");
        let backend = LocalFsBackend::new(dir.path());
        let name = ObjectName::new("manifest/current").expect("name");

        assert_eq!(
            backend.read_object(&name).expect_err("missing read").kind(),
            BackendErrorKind::NotFound
        );
        assert_eq!(
            backend
                .delete_object(&name)
                .expect_err("missing delete")
                .kind(),
            BackendErrorKind::NotFound
        );
        assert!(backend
            .list_prefix(&ObjectPrefix::new("manifest/").expect("prefix"))
            .expect("list")
            .is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn localfs_backend_rejects_symlink_object_paths() {
        use std::os::unix::fs::symlink;

        let dir = tempfile::tempdir().expect("tempdir");
        let outside = tempfile::NamedTempFile::new().expect("outside file");
        std::fs::write(outside.path(), b"outside").expect("outside write");
        let backend = LocalFsBackend::new(dir.path());
        let name = ObjectName::new("escape").expect("name");

        symlink(outside.path(), backend.path_for(&name)).expect("symlink");

        assert_eq!(
            backend.read_object(&name).expect_err("read symlink").kind(),
            BackendErrorKind::Corruption
        );
        assert_eq!(
            backend
                .write_object(&name, b"should not escape")
                .expect_err("write symlink")
                .kind(),
            BackendErrorKind::Corruption
        );
        assert_eq!(
            std::fs::read(outside.path()).expect("outside read"),
            b"outside"
        );
    }

    #[cfg(unix)]
    #[test]
    fn localfs_backend_rejects_symlink_parent_paths() {
        use std::os::unix::fs::symlink;

        let dir = tempfile::tempdir().expect("tempdir");
        let outside = tempfile::tempdir().expect("outside dir");
        let backend = LocalFsBackend::new(dir.path());
        let name = ObjectName::new("tables/object").expect("name");
        symlink(outside.path(), dir.path().join("tables")).expect("symlink dir");

        assert_eq!(
            backend
                .write_object(&name, b"bytes")
                .expect_err("write")
                .kind(),
            BackendErrorKind::Corruption
        );
        assert_eq!(
            backend.read_object(&name).expect_err("read").kind(),
            BackendErrorKind::Corruption
        );
        assert_eq!(
            backend
                .list_prefix(&ObjectPrefix::new("tables/").expect("prefix"))
                .expect_err("list")
                .kind(),
            BackendErrorKind::Corruption
        );
    }
}
