//! Local filesystem backend shell.

use super::{
    Backend, BackendAppend, BackendCapabilities, BackendError, BackendErrorKind, BackendMetadata,
    BackendRange, BackendResult, PublishDurability, PublishError, PublishFailureKind, PublishMode,
    PublishOutcome, PublishResult, BASIC_OBJECT_BACKEND_CAPABILITIES,
};
use crate::object::{ObjectName, ObjectPrefix};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
#[cfg(all(test, unix))]
use std::sync::{Arc, Mutex};

const OBJECT_FILE_SUFFIX: &str = ".object@";
static TEMP_OBJECT_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LocalFsPublishStep {
    TemporaryCreate,
    TemporaryWrite,
    TemporarySync,
    FinalPublish,
    ParentSync,
}

impl LocalFsPublishStep {
    const fn name(self) -> &'static str {
        match self {
            Self::TemporaryCreate => "temporary_create",
            Self::TemporaryWrite => "temporary_write",
            Self::TemporarySync => "temporary_sync",
            Self::FinalPublish => "final_publish",
            Self::ParentSync => "parent_sync",
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct LocalFsBackend {
    root: PathBuf,
    #[cfg(all(test, unix))]
    publish_fault: Arc<Mutex<Option<LocalFsPublishStep>>>,
}

impl LocalFsBackend {
    pub(crate) fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            #[cfg(all(test, unix))]
            publish_fault: Arc::new(Mutex::new(None)),
        }
    }

    pub(crate) fn root(&self) -> &Path {
        &self.root
    }

    #[cfg(all(test, unix))]
    fn arm_publish_fault(&self, step: LocalFsPublishStep) -> BackendResult<()> {
        let mut fault = self.publish_fault.lock().map_err(|_| {
            BackendError::new(
                BackendErrorKind::Unknown,
                "local filesystem publish fault state is poisoned",
            )
        })?;
        *fault = Some(step);
        Ok(())
    }

    #[cfg(all(test, unix))]
    pub(crate) fn inject_temporary_write_publish_fault(&self) -> BackendResult<()> {
        self.arm_publish_fault(LocalFsPublishStep::TemporaryWrite)
    }

    #[cfg(all(test, unix))]
    pub(crate) fn inject_temporary_sync_publish_fault(&self) -> BackendResult<()> {
        self.arm_publish_fault(LocalFsPublishStep::TemporarySync)
    }

    #[cfg(all(test, unix))]
    pub(crate) fn inject_parent_sync_publish_fault(&self) -> BackendResult<()> {
        self.arm_publish_fault(LocalFsPublishStep::ParentSync)
    }

    #[cfg(all(test, unix))]
    fn injected_publish_fault(&self, step: LocalFsPublishStep) -> Option<BackendError> {
        let Ok(mut fault) = self.publish_fault.lock() else {
            return Some(BackendError::new(
                BackendErrorKind::Unknown,
                "local filesystem publish fault state is poisoned",
            ));
        };

        if *fault == Some(step) {
            *fault = None;
            Some(BackendError::new(
                BackendErrorKind::Interrupted,
                format!("test fault injected at {}", step.name()),
            ))
        } else {
            None
        }
    }

    #[cfg(not(all(test, unix)))]
    fn injected_publish_fault(&self, _step: LocalFsPublishStep) -> Option<BackendError> {
        let _ = &self.root;
        None
    }

    fn path_for(&self, name: &ObjectName) -> PathBuf {
        // Object bytes live in a suffixed file so `tables/a` and
        // `tables/a/child` can coexist on filesystems where a path cannot be
        // both file and directory.
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

    fn temporary_path_for(path: &Path, sequence: u64) -> BackendResult<PathBuf> {
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| {
                BackendError::new(
                    BackendErrorKind::InvalidObjectName,
                    format!("object path {} has no file name", path.display()),
                )
            })?;

        Ok(path.with_file_name(format!(
            "{file_name}.tmp.{:x}.{sequence:016x}",
            std::process::id()
        )))
    }

    fn create_temporary_file(path: &Path) -> BackendResult<(PathBuf, File)> {
        // Temporary names include process id and a local sequence. Retrying a
        // bounded number of create_new attempts preserves no-clobber behavior
        // without assuming the directory is free of stale temp files.
        for _ in 0..16 {
            let sequence = TEMP_OBJECT_COUNTER.fetch_add(1, Ordering::Relaxed);
            let temp_path = Self::temporary_path_for(path, sequence)?;
            match OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&temp_path)
            {
                Ok(file) => return Ok((temp_path, file)),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(map_io_error(&error)),
            }
        }

        Err(BackendError::new(
            BackendErrorKind::AlreadyExists,
            format!(
                "could not allocate a temporary object path for {}",
                path.display()
            ),
        ))
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

    fn validate_optional_file_path(path: &Path) -> BackendResult<Option<BackendMetadata>> {
        match fs::symlink_metadata(path) {
            Ok(metadata) if metadata.file_type().is_symlink() => Err(BackendError::new(
                BackendErrorKind::Corruption,
                format!("object path {} is a symlink", path.display()),
            )),
            Ok(metadata) if !metadata.is_file() => Err(BackendError::new(
                BackendErrorKind::Corruption,
                format!("object path {} is not a file", path.display()),
            )),
            Ok(metadata) => Ok(Some(BackendMetadata::new(metadata.len(), None))),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(map_io_error(&error)),
        }
    }

    fn publish_error(
        name: &ObjectName,
        kind: PublishFailureKind,
        error: BackendError,
    ) -> PublishError {
        PublishError::new(name.clone(), kind, error)
    }

    fn cleanup_temporary_path(path: &Path) {
        // Best-effort temp removal must not mask the primary publish failure.
        let _ = fs::remove_file(path);
    }

    fn prepare_publish_target(
        &self,
        name: &ObjectName,
        mode: PublishMode,
    ) -> PublishResult<PathBuf> {
        let final_path = self.path_for(name);
        if let Some(parent) = final_path.parent() {
            self.ensure_parent_dirs(parent, true).map_err(|error| {
                Self::publish_error(name, PublishFailureKind::FailedBeforeVisibility, error)
            })?;
        }

        let target_metadata = Self::validate_optional_file_path(&final_path).map_err(|error| {
            Self::publish_error(name, PublishFailureKind::FailedBeforeVisibility, error)
        })?;
        // Create mode must fail before visibility if the target already
        // exists; replace mode is allowed to publish over an existing object.
        if mode == PublishMode::Create && target_metadata.is_some() {
            return Err(PublishError::precondition_failed(
                name,
                format!("object {name} already exists"),
            ));
        }

        Ok(final_path)
    }

    fn write_publish_temporary_object(
        &self,
        name: &ObjectName,
        final_path: &Path,
        bytes: &[u8],
    ) -> PublishResult<PathBuf> {
        if let Some(error) = self.injected_publish_fault(LocalFsPublishStep::TemporaryCreate) {
            return Err(Self::publish_error(
                name,
                PublishFailureKind::FailedBeforeVisibility,
                error,
            ));
        }

        let (temp_path, mut file) = Self::create_temporary_file(final_path).map_err(|error| {
            Self::publish_error(name, PublishFailureKind::FailedBeforeVisibility, error)
        })?;

        // The temporary object is not reachable by object name. Failures before
        // the final install are therefore classified as before-visibility.
        if let Some(error) = self.injected_publish_fault(LocalFsPublishStep::TemporaryWrite) {
            Self::cleanup_temporary_path(&temp_path);
            return Err(Self::publish_error(
                name,
                PublishFailureKind::FailedBeforeVisibility,
                error,
            ));
        }

        if let Err(error) = file.write_all(bytes).map_err(|err| map_io_error(&err)) {
            Self::cleanup_temporary_path(&temp_path);
            return Err(Self::publish_error(
                name,
                PublishFailureKind::FailedBeforeVisibility,
                error,
            ));
        }

        if let Some(error) = self.injected_publish_fault(LocalFsPublishStep::TemporarySync) {
            Self::cleanup_temporary_path(&temp_path);
            return Err(Self::publish_error(
                name,
                PublishFailureKind::FailedBeforeVisibility,
                error,
            ));
        }

        // fsync the temporary file before publishing it. After this point the
        // remaining uncertainty is about visibility or parent-directory
        // durability, not about whether the temp file bytes reached storage.
        if let Err(error) = file.sync_all().map_err(|err| map_io_error(&err)) {
            Self::cleanup_temporary_path(&temp_path);
            return Err(Self::publish_error(
                name,
                PublishFailureKind::FailedBeforeVisibility,
                error,
            ));
        }

        drop(file);
        Ok(temp_path)
    }

    fn install_publish_temporary_object(
        &self,
        name: &ObjectName,
        mode: PublishMode,
        temp_path: &Path,
        final_path: &Path,
    ) -> PublishResult<()> {
        if let Some(error) = self.injected_publish_fault(LocalFsPublishStep::FinalPublish) {
            Self::cleanup_temporary_path(temp_path);
            let kind = if mode == PublishMode::Replace {
                PublishFailureKind::VisibilityUnknown
            } else {
                PublishFailureKind::FailedBeforeVisibility
            };
            return Err(Self::publish_error(name, kind, error));
        }

        if mode == PublishMode::Create {
            // A hard link gives create-only no-clobber semantics: success makes
            // the temp bytes visible at the final path; AlreadyExists remains a
            // precondition failure.
            if let Err(error) = fs::hard_link(temp_path, final_path) {
                Self::cleanup_temporary_path(temp_path);
                let error = map_io_error(&error);
                if error.kind() == BackendErrorKind::AlreadyExists {
                    return Err(PublishError::precondition_failed(
                        name,
                        format!("object {name} already exists"),
                    ));
                }
                return Err(Self::publish_error(
                    name,
                    PublishFailureKind::FailedBeforeVisibility,
                    error,
                ));
            }
            Self::cleanup_temporary_path(temp_path);
            Ok(())
        } else if let Err(error) = fs::rename(temp_path, final_path) {
            // rename failure for replace is ambiguous across platforms and
            // filesystems: the caller must not assume either old or new bytes.
            Self::cleanup_temporary_path(temp_path);
            Err(Self::publish_error(
                name,
                PublishFailureKind::VisibilityUnknown,
                map_io_error(&error),
            ))
        } else {
            Ok(())
        }
    }

    fn sync_publish_parent(&self, name: &ObjectName, final_path: &Path) -> PublishResult<()> {
        let Some(parent) = final_path.parent() else {
            return Ok(());
        };

        // The object is visible before the parent directory is synced. A
        // failure here leaves visibility known but durability unconfirmed.
        if let Some(error) = self.injected_publish_fault(LocalFsPublishStep::ParentSync) {
            return Err(Self::publish_error(
                name,
                PublishFailureKind::VisibleDurabilityUnconfirmed,
                error,
            ));
        }

        File::open(parent)
            .and_then(|dir| dir.sync_all())
            .map_err(|err| {
                Self::publish_error(
                    name,
                    PublishFailureKind::VisibleDurabilityUnconfirmed,
                    map_io_error(&err),
                )
            })
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
        // Only files with the object suffix map back to ObjectName values; this
        // prevents directory-only prefixes and temporary files from leaking
        // into backend listings.
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
                        // Unknown files are ignored; malformed object-suffixed
                        // files are corruption because they could shadow a
                        // backend object.
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
        let mut capabilities = BackendCapabilities::from_slice(BASIC_OBJECT_BACKEND_CAPABILITIES);
        capabilities.insert(super::BackendCapability::AppendObject);
        if cfg!(unix) {
            capabilities.insert(super::BackendCapability::DurablePublish);
            capabilities.insert(super::BackendCapability::DurableSync);
        }
        capabilities
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

    fn append_object(&self, name: &ObjectName, bytes: &[u8]) -> BackendResult<BackendAppend> {
        let path = self.path_for(name);
        let before = self.metadata_for_object_path(&path)?.size_bytes();
        let mut file = OpenOptions::new()
            .append(true)
            .open(&path)
            .map_err(|err| map_io_error(&err))?;
        file.write_all(bytes).map_err(|err| map_io_error(&err))?;
        let after = self.metadata_for_object_path(&path)?;
        Ok(BackendAppend::new(before, bytes.len() as u64, after))
    }

    fn sync_object(&self, name: &ObjectName) -> BackendResult<()> {
        let path = self.path_for(name);
        self.metadata_for_object_path(&path)?;
        File::open(path)
            .and_then(|file| file.sync_all())
            .map_err(|err| map_io_error(&err))
    }

    fn publish_object(
        &self,
        name: &ObjectName,
        bytes: &[u8],
        mode: PublishMode,
    ) -> PublishResult<PublishOutcome> {
        if mode == PublishMode::NonDurableReplace {
            return Err(PublishError::unsupported(
                name,
                BackendError::unsupported(super::BackendCapability::DurablePublish),
            ));
        }
        if !cfg!(unix) {
            return Err(PublishError::unsupported(
                name,
                BackendError::unsupported(super::BackendCapability::DurablePublish),
            ));
        }

        let final_path = self.prepare_publish_target(name, mode)?;
        let temp_path = self.write_publish_temporary_object(name, &final_path, bytes)?;
        self.install_publish_temporary_object(name, mode, &temp_path, &final_path)?;
        self.sync_publish_parent(name, &final_path)?;

        Ok(PublishOutcome::new(
            name.clone(),
            BackendMetadata::new(bytes.len() as u64, None),
            PublishDurability::Durable,
        ))
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
    #[cfg(unix)]
    use super::LocalFsPublishStep;
    #[cfg(unix)]
    use crate::backend::PublishDurability;
    use crate::backend::{
        Backend, BackendCapability, BackendErrorKind, BackendRange, PublishFailureKind,
        PublishMode, BASIC_OBJECT_BACKEND_CAPABILITIES, CACHE_MODE_REQUIREMENTS,
    };
    use crate::object::{ObjectName, ObjectPrefix};
    #[cfg(unix)]
    use std::path::PathBuf;

    #[cfg(unix)]
    fn temporary_entries(backend: &LocalFsBackend, name: &ObjectName) -> Vec<PathBuf> {
        let final_path = backend.path_for(name);
        let parent = final_path.parent().expect("object parent");
        match std::fs::read_dir(parent) {
            Ok(entries) => entries
                .filter_map(Result::ok)
                .map(|entry| entry.path())
                .filter(|path| {
                    path.file_name()
                        .and_then(|name| name.to_str())
                        .is_some_and(|name| name.contains(".tmp."))
                })
                .collect(),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(error) => panic!("could not read object parent {}: {error}", parent.display()),
        }
    }

    #[cfg(unix)]
    fn assert_no_temporary_entries(backend: &LocalFsBackend, name: &ObjectName) {
        let entries = temporary_entries(backend, name);
        assert!(
            entries.is_empty(),
            "publish left temporary entries: {entries:?}"
        );
    }

    #[test]
    fn localfs_backend_reports_object_and_durable_unix_capabilities() {
        let dir = tempfile::tempdir().expect("tempdir");
        let backend = LocalFsBackend::new(dir.path());
        let capabilities = backend.capabilities();

        assert_eq!(backend.root(), dir.path());
        assert!(capabilities.supports(CACHE_MODE_REQUIREMENTS));
        assert!(capabilities.supports(BASIC_OBJECT_BACKEND_CAPABILITIES));
        assert_eq!(
            capabilities.contains(BackendCapability::DurablePublish),
            cfg!(unix)
        );
        assert!(capabilities.contains(BackendCapability::AppendObject));
        assert_eq!(
            capabilities.contains(BackendCapability::DurableSync),
            cfg!(unix)
        );
        assert!(!capabilities.contains(BackendCapability::SingleWriterLock));
    }

    #[cfg(unix)]
    #[test]
    fn localfs_backend_publishes_object_durably() {
        let dir = tempfile::tempdir().expect("tempdir");
        let backend = LocalFsBackend::new(dir.path());
        let name = ObjectName::new("manifest/current").expect("name");

        let outcome = backend
            .publish_object(&name, b"manifest", PublishMode::Replace)
            .expect("publish");

        assert_eq!(outcome.object(), &name);
        assert_eq!(outcome.metadata().size_bytes(), 8);
        assert_eq!(outcome.durability(), PublishDurability::Durable);
        assert_eq!(backend.read_object(&name).expect("read"), b"manifest");
        let final_path = backend.path_for(&name);
        let parent = final_path.parent().expect("parent");
        let temp_entries: Vec<_> = std::fs::read_dir(parent)
            .expect("read parent")
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().contains(".tmp."))
            .collect();
        assert!(
            temp_entries.is_empty(),
            "publish should not leave temporary files"
        );
    }

    #[cfg(unix)]
    #[test]
    fn localfs_backend_create_publish_preserves_existing_object() {
        let dir = tempfile::tempdir().expect("tempdir");
        let backend = LocalFsBackend::new(dir.path());
        let name = ObjectName::new("manifest/current").expect("name");

        backend.write_object(&name, b"old").expect("seed");
        let error = backend
            .publish_object(&name, b"new", PublishMode::Create)
            .expect_err("create should reject existing object");

        assert_eq!(error.kind(), PublishFailureKind::PreconditionFailed);
        assert_eq!(error.source_error().kind(), BackendErrorKind::AlreadyExists);
        assert_eq!(backend.read_object(&name).expect("old preserved"), b"old");
    }

    #[cfg(unix)]
    #[test]
    fn localfs_backend_replace_publish_overwrites_existing_object() {
        let dir = tempfile::tempdir().expect("tempdir");
        let backend = LocalFsBackend::new(dir.path());
        let name = ObjectName::new("manifest/current").expect("name");

        backend.write_object(&name, b"old").expect("seed");
        let outcome = backend
            .publish_object(&name, b"new manifest", PublishMode::Replace)
            .expect("replace publish");

        assert_eq!(outcome.object(), &name);
        assert_eq!(outcome.metadata().size_bytes(), 12);
        assert_eq!(backend.read_object(&name).expect("read"), b"new manifest");
    }

    #[cfg(unix)]
    #[test]
    fn localfs_backend_publish_temp_create_fault_leaves_no_visible_object() {
        let dir = tempfile::tempdir().expect("tempdir");
        let backend = LocalFsBackend::new(dir.path());
        let name = ObjectName::new("manifest/current").expect("name");
        backend
            .arm_publish_fault(LocalFsPublishStep::TemporaryCreate)
            .expect("arm fault");

        let error = backend
            .publish_object(&name, b"manifest", PublishMode::Replace)
            .expect_err("publish fault");

        assert_eq!(error.kind(), PublishFailureKind::FailedBeforeVisibility);
        assert_eq!(error.source_error().kind(), BackendErrorKind::Interrupted);
        assert_eq!(
            backend
                .read_object(&name)
                .expect_err("final object absent")
                .kind(),
            BackendErrorKind::NotFound
        );
        assert_no_temporary_entries(&backend, &name);
    }

    #[cfg(unix)]
    #[test]
    fn localfs_backend_publish_temp_write_fault_cleans_temp_and_preserves_existing_object() {
        let dir = tempfile::tempdir().expect("tempdir");
        let backend = LocalFsBackend::new(dir.path());
        let name = ObjectName::new("manifest/current").expect("name");
        backend.write_object(&name, b"old").expect("seed");
        backend
            .arm_publish_fault(LocalFsPublishStep::TemporaryWrite)
            .expect("arm fault");

        let error = backend
            .publish_object(&name, b"new", PublishMode::Replace)
            .expect_err("publish fault");

        assert_eq!(error.kind(), PublishFailureKind::FailedBeforeVisibility);
        assert_eq!(error.source_error().kind(), BackendErrorKind::Interrupted);
        assert_eq!(backend.read_object(&name).expect("old preserved"), b"old");
        assert_no_temporary_entries(&backend, &name);
    }

    #[cfg(unix)]
    #[test]
    fn localfs_backend_publish_temp_sync_fault_cleans_temp_and_preserves_existing_object() {
        let dir = tempfile::tempdir().expect("tempdir");
        let backend = LocalFsBackend::new(dir.path());
        let name = ObjectName::new("manifest/current").expect("name");
        backend.write_object(&name, b"old").expect("seed");
        backend
            .arm_publish_fault(LocalFsPublishStep::TemporarySync)
            .expect("arm fault");

        let error = backend
            .publish_object(&name, b"new", PublishMode::Replace)
            .expect_err("publish fault");

        assert_eq!(error.kind(), PublishFailureKind::FailedBeforeVisibility);
        assert_eq!(error.source_error().kind(), BackendErrorKind::Interrupted);
        assert_eq!(backend.read_object(&name).expect("old preserved"), b"old");
        assert_no_temporary_entries(&backend, &name);
    }

    #[cfg(unix)]
    #[test]
    fn localfs_backend_publish_final_publish_fault_cleans_temp_and_preserves_existing_object() {
        let dir = tempfile::tempdir().expect("tempdir");
        let backend = LocalFsBackend::new(dir.path());
        let name = ObjectName::new("manifest/current").expect("name");
        backend.write_object(&name, b"old").expect("seed");
        backend
            .arm_publish_fault(LocalFsPublishStep::FinalPublish)
            .expect("arm fault");

        let error = backend
            .publish_object(&name, b"new", PublishMode::Replace)
            .expect_err("publish fault");

        assert_eq!(error.kind(), PublishFailureKind::VisibilityUnknown);
        assert_eq!(error.source_error().kind(), BackendErrorKind::Interrupted);
        assert_eq!(backend.read_object(&name).expect("old preserved"), b"old");
        assert_no_temporary_entries(&backend, &name);
    }

    #[cfg(unix)]
    #[test]
    fn localfs_backend_publish_create_final_publish_fault_leaves_no_visible_object() {
        let dir = tempfile::tempdir().expect("tempdir");
        let backend = LocalFsBackend::new(dir.path());
        let name = ObjectName::new("manifest/current").expect("name");
        backend
            .arm_publish_fault(LocalFsPublishStep::FinalPublish)
            .expect("arm fault");

        let error = backend
            .publish_object(&name, b"new", PublishMode::Create)
            .expect_err("publish fault");

        assert_eq!(error.kind(), PublishFailureKind::FailedBeforeVisibility);
        assert_eq!(error.source_error().kind(), BackendErrorKind::Interrupted);
        assert_eq!(
            backend
                .read_object(&name)
                .expect_err("final object absent")
                .kind(),
            BackendErrorKind::NotFound
        );
        assert_no_temporary_entries(&backend, &name);
    }

    #[cfg(unix)]
    #[test]
    fn localfs_backend_publish_parent_sync_fault_leaves_new_object_visible() {
        let dir = tempfile::tempdir().expect("tempdir");
        let backend = LocalFsBackend::new(dir.path());
        let name = ObjectName::new("manifest/current").expect("name");
        backend.write_object(&name, b"old").expect("seed");
        backend
            .arm_publish_fault(LocalFsPublishStep::ParentSync)
            .expect("arm fault");

        let error = backend
            .publish_object(&name, b"new", PublishMode::Replace)
            .expect_err("publish fault");

        assert_eq!(
            error.kind(),
            PublishFailureKind::VisibleDurabilityUnconfirmed
        );
        assert_eq!(error.source_error().kind(), BackendErrorKind::Interrupted);
        assert_eq!(backend.read_object(&name).expect("new visible"), b"new");
        assert_no_temporary_entries(&backend, &name);
    }

    #[cfg(not(unix))]
    #[test]
    fn localfs_backend_rejects_durable_publish_without_unix_directory_sync() {
        let dir = tempfile::tempdir().expect("tempdir");
        let backend = LocalFsBackend::new(dir.path());
        let name = ObjectName::new("manifest/current").expect("name");

        for mode in [PublishMode::Create, PublishMode::Replace] {
            let error = backend
                .publish_object(&name, b"bytes", mode)
                .expect_err("durable publish should require unix directory sync");

            assert_eq!(error.kind(), PublishFailureKind::Unsupported);
            assert_eq!(
                error.source_error().kind(),
                BackendErrorKind::UnsupportedOperation
            );
        }
    }

    #[test]
    fn localfs_backend_rejects_non_durable_publish_mode() {
        let dir = tempfile::tempdir().expect("tempdir");
        let backend = LocalFsBackend::new(dir.path());
        let name = ObjectName::new("manifest/current").expect("name");

        let error = backend
            .publish_object(&name, b"bytes", PublishMode::NonDurableReplace)
            .expect_err("local durable backend should not claim cache publishing");

        assert_eq!(error.kind(), PublishFailureKind::Unsupported);
        assert_eq!(
            error.source_error().kind(),
            BackendErrorKind::UnsupportedOperation
        );
    }

    #[cfg(unix)]
    #[test]
    fn localfs_backend_publish_rejects_symlink_object_paths() {
        use std::os::unix::fs::symlink;

        let dir = tempfile::tempdir().expect("tempdir");
        let outside = tempfile::NamedTempFile::new().expect("outside file");
        std::fs::write(outside.path(), b"outside").expect("outside write");
        let backend = LocalFsBackend::new(dir.path());
        let name = ObjectName::new("escape").expect("name");

        symlink(outside.path(), backend.path_for(&name)).expect("symlink");
        let error = backend
            .publish_object(&name, b"should not escape", PublishMode::Replace)
            .expect_err("publish symlink");

        assert_eq!(error.kind(), PublishFailureKind::FailedBeforeVisibility);
        assert_eq!(error.source_error().kind(), BackendErrorKind::Corruption);
        assert_eq!(
            std::fs::read(outside.path()).expect("outside read"),
            b"outside"
        );
    }

    #[cfg(unix)]
    #[test]
    fn localfs_backend_publish_rejects_symlink_parent_paths() {
        use std::os::unix::fs::symlink;

        let dir = tempfile::tempdir().expect("tempdir");
        let outside = tempfile::tempdir().expect("outside dir");
        let backend = LocalFsBackend::new(dir.path());
        let name = ObjectName::new("tables/object").expect("name");
        symlink(outside.path(), dir.path().join("tables")).expect("symlink dir");

        let error = backend
            .publish_object(&name, b"bytes", PublishMode::Replace)
            .expect_err("publish through symlink parent");

        assert_eq!(error.kind(), PublishFailureKind::FailedBeforeVisibility);
        assert_eq!(error.source_error().kind(), BackendErrorKind::Corruption);
        assert!(std::fs::read_dir(outside.path())
            .expect("outside dir")
            .next()
            .is_none());
    }

    #[cfg(unix)]
    #[test]
    fn localfs_backend_publish_ignores_stale_temporary_files() {
        let dir = tempfile::tempdir().expect("tempdir");
        let backend = LocalFsBackend::new(dir.path());
        let name = ObjectName::new("manifest/current").expect("name");
        let final_path = backend.path_for(&name);
        let parent = final_path.parent().expect("parent");
        std::fs::create_dir_all(parent).expect("parent dirs");
        let final_file_name = final_path
            .file_name()
            .and_then(|name| name.to_str())
            .expect("file name");
        let stale_temp = parent.join(format!("{final_file_name}.tmp.stale"));
        std::fs::write(&stale_temp, b"stale").expect("stale temp");

        backend
            .publish_object(&name, b"manifest", PublishMode::Replace)
            .expect("publish");

        assert_eq!(backend.read_object(&name).expect("read"), b"manifest");
        assert_eq!(
            std::fs::read(&stale_temp).expect("stale temp preserved"),
            b"stale"
        );
        let listed = backend
            .list_prefix(&ObjectPrefix::new("manifest/").expect("prefix"))
            .expect("list");
        assert_eq!(listed, vec![name]);
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
    fn localfs_backend_appends_to_existing_object() {
        let dir = tempfile::tempdir().expect("tempdir");
        let backend = LocalFsBackend::new(dir.path());
        let name = ObjectName::new("wal/0000000000000001").expect("valid object name");
        backend.write_object(&name, b"abc").expect("seed");

        let append = backend.append_object(&name, b"def").expect("append");

        assert_eq!(append.start_offset(), 3);
        assert_eq!(append.bytes_written(), 3);
        assert_eq!(append.metadata().size_bytes(), 6);
        assert_eq!(backend.read_object(&name).expect("read"), b"abcdef");
    }

    #[test]
    fn localfs_backend_append_requires_existing_object() {
        let dir = tempfile::tempdir().expect("tempdir");
        let backend = LocalFsBackend::new(dir.path());
        let name = ObjectName::new("wal/0000000000000001").expect("valid object name");

        assert_eq!(
            backend
                .append_object(&name, b"def")
                .expect_err("missing object")
                .kind(),
            BackendErrorKind::NotFound
        );
    }

    #[test]
    fn localfs_backend_sync_requires_existing_object() {
        let dir = tempfile::tempdir().expect("tempdir");
        let backend = LocalFsBackend::new(dir.path());
        let name = ObjectName::new("wal/0000000000000001").expect("valid object name");

        assert_eq!(
            backend
                .sync_object(&name)
                .expect_err("missing object")
                .kind(),
            BackendErrorKind::NotFound
        );
        backend.write_object(&name, b"abc").expect("write");
        backend.sync_object(&name).expect("sync existing object");
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
