use std::ffi::OsString;
use std::path::{Component, Path, PathBuf};

// Test hooks are often injected with raw temp paths while runtime lookups use
// Database::data_dir(), which is canonicalized during open.
pub(crate) fn hook_path_key(path: &Path) -> PathBuf {
    if let Ok(canonical) = path.canonicalize() {
        return canonical;
    }

    let lexical = normalize_lexically(path);
    canonicalize_existing_prefix(path)
        .or_else(|| canonicalize_existing_prefix(&lexical))
        .unwrap_or(lexical)
}

fn normalize_lexically(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();

    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => match normalized.components().last() {
                Some(Component::RootDir | Component::Prefix(_)) => {}
                Some(Component::Normal(_)) => {
                    normalized.pop();
                }
                Some(Component::ParentDir) | Some(Component::CurDir) | None => {
                    normalized.push(component.as_os_str());
                }
            },
            Component::Prefix(_) | Component::RootDir | Component::Normal(_) => {
                normalized.push(component.as_os_str());
            }
        }
    }

    normalized
}

// Preserve useful canonicalization for hooks installed before the database
// directory exists by resolving the deepest existing parent.
fn canonicalize_existing_prefix(path: &Path) -> Option<PathBuf> {
    let mut suffix = Vec::<OsString>::new();
    let mut current = path;

    loop {
        if let Ok(mut canonical) = current.canonicalize() {
            for component in suffix.iter().rev() {
                canonical.push(component);
            }
            return Some(normalize_lexically(&canonical));
        }

        if current.as_os_str().is_empty() && !path.is_absolute() {
            let mut canonical = std::env::current_dir().ok()?.canonicalize().ok()?;
            for component in suffix.iter().rev() {
                canonical.push(component);
            }
            return Some(normalize_lexically(&canonical));
        }

        suffix.push(current.file_name()?.to_os_string());
        current = current.parent()?;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hook_path_key_uses_canonical_existing_path() {
        let temp_dir = tempfile::TempDir::new().expect("temp dir");
        let db_dir = temp_dir.path().join("db");
        std::fs::create_dir_all(&db_dir).expect("create db dir");
        let raw_alias = db_dir.join("..").join("db");

        assert_eq!(hook_path_key(&raw_alias), db_dir.canonicalize().unwrap());
    }

    #[test]
    fn hook_path_key_keeps_distinct_paths_distinct() {
        let temp_dir = tempfile::TempDir::new().expect("temp dir");
        let db_a = temp_dir.path().join("db-a");
        let db_b = temp_dir.path().join("db-b");
        std::fs::create_dir_all(&db_a).expect("create db-a");
        std::fs::create_dir_all(&db_b).expect("create db-b");

        assert_ne!(hook_path_key(&db_a), hook_path_key(&db_b));
    }

    #[test]
    fn hook_path_key_normalizes_missing_absolute_path_under_existing_parent() {
        let temp_dir = tempfile::TempDir::new().expect("temp dir");
        let missing = temp_dir.path().join("db").join("..").join("db");
        let expected = temp_dir.path().canonicalize().unwrap().join("db");

        assert_eq!(hook_path_key(&missing), expected);
    }

    #[test]
    fn hook_path_key_normalizes_missing_relative_path_under_existing_parent() {
        let missing_name = format!("__strata_missing_hook_key_{}__", std::process::id());
        let missing = Path::new(".").join(&missing_name);
        let expected = std::env::current_dir()
            .unwrap()
            .canonicalize()
            .unwrap()
            .join(&missing_name);

        assert_eq!(hook_path_key(&missing), expected);
    }

    #[cfg(unix)]
    #[test]
    fn hook_path_key_canonicalizes_before_lexical_parent_cleanup() {
        let temp_dir = tempfile::TempDir::new().expect("temp dir");
        let actual = temp_dir.path().join("actual");
        let nested = actual.join("nested");
        let expected = actual.join("db");
        std::fs::create_dir_all(&nested).expect("create nested target");
        std::fs::create_dir_all(&expected).expect("create expected db");

        let symlink = temp_dir.path().join("link");
        std::os::unix::fs::symlink(&nested, &symlink).expect("create symlink");
        let alias = symlink.join("..").join("db");

        assert_eq!(hook_path_key(&alias), expected.canonicalize().unwrap());
    }
}
