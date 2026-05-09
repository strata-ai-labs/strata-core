use crate::test_path_key::hook_path_key;
use std::collections::{HashMap, HashSet};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::{Duration, Instant};

fn sync_failure_slot() -> &'static Mutex<HashMap<PathBuf, io::ErrorKind>> {
    static SYNC_FAILURE: OnceLock<Mutex<HashMap<PathBuf, io::ErrorKind>>> = OnceLock::new();
    SYNC_FAILURE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn begin_sync_failure_slot() -> &'static Mutex<HashMap<PathBuf, io::ErrorKind>> {
    static BEGIN_SYNC_FAILURE: OnceLock<Mutex<HashMap<PathBuf, io::ErrorKind>>> = OnceLock::new();
    BEGIN_SYNC_FAILURE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn commit_sync_failure_slot() -> &'static Mutex<HashMap<PathBuf, io::ErrorKind>> {
    static COMMIT_SYNC_FAILURE: OnceLock<Mutex<HashMap<PathBuf, io::ErrorKind>>> = OnceLock::new();
    COMMIT_SYNC_FAILURE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn flush_thread_spawn_failure_slot() -> &'static Mutex<HashSet<PathBuf>> {
    static FLUSH_THREAD_SPAWN_FAILURE: OnceLock<Mutex<HashSet<PathBuf>>> = OnceLock::new();
    FLUSH_THREAD_SPAWN_FAILURE.get_or_init(|| Mutex::new(HashSet::new()))
}

fn clear_branch_storage_failure_slot() -> &'static Mutex<HashMap<PathBuf, io::ErrorKind>> {
    static CLEAR_BRANCH_STORAGE_FAILURE: OnceLock<Mutex<HashMap<PathBuf, io::ErrorKind>>> =
        OnceLock::new();
    CLEAR_BRANCH_STORAGE_FAILURE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn clear_default_branch_marker_failure_slot() -> &'static Mutex<HashMap<PathBuf, io::ErrorKind>> {
    static CLEAR_DEFAULT_BRANCH_MARKER_FAILURE: OnceLock<Mutex<HashMap<PathBuf, io::ErrorKind>>> =
        OnceLock::new();
    CLEAR_DEFAULT_BRANCH_MARKER_FAILURE.get_or_init(|| Mutex::new(HashMap::new()))
}

#[derive(Default)]
struct HaltPublishPauseState {
    reached: bool,
    released: bool,
}

type HaltPublishPause = Arc<(Mutex<HaltPublishPauseState>, Condvar)>;

fn halt_publish_pause_slot() -> &'static Mutex<HashMap<PathBuf, HaltPublishPause>> {
    static HALT_PUBLISH_PAUSE: OnceLock<Mutex<HashMap<PathBuf, HaltPublishPause>>> =
        OnceLock::new();
    HALT_PUBLISH_PAUSE.get_or_init(|| Mutex::new(HashMap::new()))
}

pub(super) fn inject_sync_failure(path: &Path, kind: io::ErrorKind) {
    let key = hook_path_key(path);
    sync_failure_slot().lock().unwrap().insert(key, kind);
}

pub(super) fn clear_sync_failure(path: &Path) {
    if sync_failure_slot().lock().unwrap().is_empty() {
        return;
    }
    let key = hook_path_key(path);
    sync_failure_slot().lock().unwrap().remove(&key);
}

pub(super) fn inject_begin_sync_failure(path: &Path, kind: io::ErrorKind) {
    let key = hook_path_key(path);
    begin_sync_failure_slot().lock().unwrap().insert(key, kind);
}

pub(super) fn clear_begin_sync_failure(path: &Path) {
    if begin_sync_failure_slot().lock().unwrap().is_empty() {
        return;
    }
    let key = hook_path_key(path);
    begin_sync_failure_slot().lock().unwrap().remove(&key);
}

pub(super) fn inject_commit_sync_failure(path: &Path, kind: io::ErrorKind) {
    let key = hook_path_key(path);
    commit_sync_failure_slot().lock().unwrap().insert(key, kind);
}

pub(super) fn clear_commit_sync_failure(path: &Path) {
    if commit_sync_failure_slot().lock().unwrap().is_empty() {
        return;
    }
    let key = hook_path_key(path);
    commit_sync_failure_slot().lock().unwrap().remove(&key);
}

pub(super) fn inject_flush_thread_spawn_failure(path: &Path) {
    let key = hook_path_key(path);
    flush_thread_spawn_failure_slot()
        .lock()
        .unwrap()
        .insert(key);
}

pub(super) fn clear_flush_thread_spawn_failure(path: &Path) {
    if flush_thread_spawn_failure_slot().lock().unwrap().is_empty() {
        return;
    }
    let key = hook_path_key(path);
    flush_thread_spawn_failure_slot()
        .lock()
        .unwrap()
        .remove(&key);
}

pub(super) fn inject_clear_branch_storage_failure(path: &Path, kind: io::ErrorKind) {
    let key = hook_path_key(path);
    clear_branch_storage_failure_slot()
        .lock()
        .unwrap()
        .insert(key, kind);
}

pub(super) fn clear_clear_branch_storage_failure(path: &Path) {
    if clear_branch_storage_failure_slot()
        .lock()
        .unwrap()
        .is_empty()
    {
        return;
    }
    let key = hook_path_key(path);
    clear_branch_storage_failure_slot()
        .lock()
        .unwrap()
        .remove(&key);
}

pub(super) fn inject_clear_default_branch_marker_failure(path: &Path, kind: io::ErrorKind) {
    let key = hook_path_key(path);
    clear_default_branch_marker_failure_slot()
        .lock()
        .unwrap()
        .insert(key, kind);
}

pub(super) fn clear_clear_default_branch_marker_failure(path: &Path) {
    if clear_default_branch_marker_failure_slot()
        .lock()
        .unwrap()
        .is_empty()
    {
        return;
    }
    let key = hook_path_key(path);
    clear_default_branch_marker_failure_slot()
        .lock()
        .unwrap()
        .remove(&key);
}

pub(super) fn install_halt_publish_pause(path: &Path) {
    let key = hook_path_key(path);
    halt_publish_pause_slot().lock().unwrap().insert(
        key,
        Arc::new((Mutex::new(HaltPublishPauseState::default()), Condvar::new())),
    );
}

pub(super) fn clear_halt_publish_pause(path: &Path) {
    if halt_publish_pause_slot().lock().unwrap().is_empty() {
        return;
    }
    let key = hook_path_key(path);
    let pause = halt_publish_pause_slot().lock().unwrap().remove(&key);

    if let Some(pause) = pause {
        let (state, cv) = &*pause;
        let mut state = state.lock().unwrap();
        state.released = true;
        cv.notify_all();
    }
}

pub(super) fn maybe_inject_sync_failure(path: &Path) -> Option<io::Error> {
    if sync_failure_slot().lock().unwrap().is_empty() {
        return None;
    }
    let key = hook_path_key(path);
    sync_failure_slot()
        .lock()
        .unwrap()
        .remove(&key)
        .map(|kind| io::Error::new(kind, "injected sync failure"))
}

pub(super) fn maybe_inject_begin_sync_failure(path: &Path) -> Option<io::Error> {
    if begin_sync_failure_slot().lock().unwrap().is_empty() {
        return None;
    }
    let key = hook_path_key(path);
    begin_sync_failure_slot()
        .lock()
        .unwrap()
        .remove(&key)
        .map(|kind| io::Error::new(kind, "injected begin_background_sync failure"))
}

pub(super) fn maybe_inject_commit_sync_failure(path: &Path) -> Option<io::Error> {
    if commit_sync_failure_slot().lock().unwrap().is_empty() {
        return None;
    }
    let key = hook_path_key(path);
    commit_sync_failure_slot()
        .lock()
        .unwrap()
        .remove(&key)
        .map(|kind| io::Error::new(kind, "injected commit_background_sync failure"))
}

pub(super) fn take_flush_thread_spawn_failure(path: &Path) -> bool {
    if flush_thread_spawn_failure_slot().lock().unwrap().is_empty() {
        return false;
    }
    let key = hook_path_key(path);
    flush_thread_spawn_failure_slot()
        .lock()
        .unwrap()
        .remove(&key)
}

pub(super) fn maybe_inject_clear_branch_storage_failure(path: &Path) -> Option<io::Error> {
    if clear_branch_storage_failure_slot()
        .lock()
        .unwrap()
        .is_empty()
    {
        return None;
    }
    let key = hook_path_key(path);
    clear_branch_storage_failure_slot()
        .lock()
        .unwrap()
        .remove(&key)
        .map(|kind| io::Error::new(kind, "injected clear_branch_storage failure"))
}

pub(super) fn maybe_inject_clear_default_branch_marker_failure(path: &Path) -> Option<io::Error> {
    if clear_default_branch_marker_failure_slot()
        .lock()
        .unwrap()
        .is_empty()
    {
        return None;
    }
    let key = hook_path_key(path);
    clear_default_branch_marker_failure_slot()
        .lock()
        .unwrap()
        .remove(&key)
        .map(|kind| io::Error::new(kind, "injected clear_default_branch_marker failure"))
}

pub(super) fn maybe_pause_after_halt_health_publish(path: &Path) {
    if halt_publish_pause_slot().lock().unwrap().is_empty() {
        return;
    }
    let key = hook_path_key(path);
    let pause = halt_publish_pause_slot().lock().unwrap().get(&key).cloned();

    if let Some(pause) = pause {
        let (state, cv) = &*pause;
        let mut state = state.lock().unwrap();
        state.reached = true;
        cv.notify_all();
        while !state.released {
            state = cv.wait(state).unwrap();
        }
    }
}

pub(super) fn wait_for_halt_publish_pause(path: &Path, timeout: Duration) -> bool {
    if halt_publish_pause_slot().lock().unwrap().is_empty() {
        return false;
    }
    let key = hook_path_key(path);
    let Some(pause) = halt_publish_pause_slot().lock().unwrap().get(&key).cloned() else {
        return false;
    };

    let start = Instant::now();
    let (state, cv) = &*pause;
    let mut state = state.lock().unwrap();
    while !state.reached {
        let Some(remaining) = timeout.checked_sub(start.elapsed()) else {
            return false;
        };
        let (next_state, wait_result) = cv.wait_timeout(state, remaining).unwrap();
        state = next_state;
        if wait_result.timed_out() && !state.reached {
            return false;
        }
    }
    true
}

pub(super) fn release_halt_publish_pause(path: &Path) {
    if halt_publish_pause_slot().lock().unwrap().is_empty() {
        return;
    }
    let key = hook_path_key(path);
    let pause = halt_publish_pause_slot().lock().unwrap().get(&key).cloned();

    if let Some(pause) = pause {
        let (state, cv) = &*pause;
        let mut state = state.lock().unwrap();
        state.released = true;
        cv.notify_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sync_failure_hook_consumes_one_failure() {
        let path = Path::new("/tmp/test-sync-failure");
        let other_path = Path::new("/tmp/test-sync-failure-other");
        clear_sync_failure(path);
        clear_sync_failure(other_path);
        assert!(maybe_inject_sync_failure(path).is_none());
        assert!(maybe_inject_sync_failure(other_path).is_none());

        inject_sync_failure(path, io::ErrorKind::Other);
        assert!(
            maybe_inject_sync_failure(other_path).is_none(),
            "hook must be scoped by path"
        );
        let err = maybe_inject_sync_failure(path).expect("expected injected failure");
        assert_eq!(err.kind(), io::ErrorKind::Other);
        assert!(maybe_inject_sync_failure(path).is_none());
    }

    #[test]
    fn test_flush_thread_spawn_hook_consumes_one_failure() {
        let path = Path::new("/tmp/test-spawn-failure");
        let other_path = Path::new("/tmp/test-spawn-failure-other");
        clear_flush_thread_spawn_failure(path);
        clear_flush_thread_spawn_failure(other_path);
        assert!(!take_flush_thread_spawn_failure(path));
        assert!(!take_flush_thread_spawn_failure(other_path));

        inject_flush_thread_spawn_failure(path);
        assert!(
            !take_flush_thread_spawn_failure(other_path),
            "hook must be scoped by path"
        );
        assert!(take_flush_thread_spawn_failure(path));
        assert!(!take_flush_thread_spawn_failure(path));
    }

    #[test]
    fn test_clear_branch_storage_failure_hook_consumes_one_failure() {
        let path = Path::new("/tmp/test-clear-branch-storage-failure");
        let other_path = Path::new("/tmp/test-clear-branch-storage-failure-other");
        clear_clear_branch_storage_failure(path);
        clear_clear_branch_storage_failure(other_path);
        assert!(maybe_inject_clear_branch_storage_failure(path).is_none());
        assert!(maybe_inject_clear_branch_storage_failure(other_path).is_none());

        inject_clear_branch_storage_failure(path, io::ErrorKind::Other);
        assert!(
            maybe_inject_clear_branch_storage_failure(other_path).is_none(),
            "hook must be scoped by path"
        );
        let err =
            maybe_inject_clear_branch_storage_failure(path).expect("expected injected failure");
        assert_eq!(err.kind(), io::ErrorKind::Other);
        assert!(maybe_inject_clear_branch_storage_failure(path).is_none());
    }

    #[test]
    fn test_clear_default_branch_marker_failure_hook_consumes_one_failure() {
        let path = Path::new("/tmp/test-clear-default-branch-marker-failure");
        let other_path = Path::new("/tmp/test-clear-default-branch-marker-failure-other");
        clear_clear_default_branch_marker_failure(path);
        clear_clear_default_branch_marker_failure(other_path);
        assert!(maybe_inject_clear_default_branch_marker_failure(path).is_none());
        assert!(maybe_inject_clear_default_branch_marker_failure(other_path).is_none());

        inject_clear_default_branch_marker_failure(path, io::ErrorKind::Other);
        assert!(
            maybe_inject_clear_default_branch_marker_failure(other_path).is_none(),
            "hook must be scoped by path"
        );
        let err = maybe_inject_clear_default_branch_marker_failure(path)
            .expect("expected injected failure");
        assert_eq!(err.kind(), io::ErrorKind::Other);
        assert!(maybe_inject_clear_default_branch_marker_failure(path).is_none());
    }

    #[test]
    fn test_begin_sync_failure_hook_consumes_one_failure() {
        let path = Path::new("/tmp/test-begin-sync-failure");
        let other_path = Path::new("/tmp/test-begin-sync-failure-other");
        clear_begin_sync_failure(path);
        clear_begin_sync_failure(other_path);
        assert!(maybe_inject_begin_sync_failure(path).is_none());
        assert!(maybe_inject_begin_sync_failure(other_path).is_none());

        inject_begin_sync_failure(path, io::ErrorKind::Other);
        assert!(
            maybe_inject_begin_sync_failure(other_path).is_none(),
            "hook must be scoped by path"
        );
        let err = maybe_inject_begin_sync_failure(path).expect("expected injected failure");
        assert_eq!(err.kind(), io::ErrorKind::Other);
        assert!(maybe_inject_begin_sync_failure(path).is_none());
    }

    #[test]
    fn test_commit_sync_failure_hook_consumes_one_failure() {
        let path = Path::new("/tmp/test-commit-sync-failure");
        let other_path = Path::new("/tmp/test-commit-sync-failure-other");
        clear_commit_sync_failure(path);
        clear_commit_sync_failure(other_path);
        assert!(maybe_inject_commit_sync_failure(path).is_none());
        assert!(maybe_inject_commit_sync_failure(other_path).is_none());

        inject_commit_sync_failure(path, io::ErrorKind::Other);
        assert!(
            maybe_inject_commit_sync_failure(other_path).is_none(),
            "hook must be scoped by path"
        );
        let err = maybe_inject_commit_sync_failure(path).expect("expected injected failure");
        assert_eq!(err.kind(), io::ErrorKind::Other);
        assert!(maybe_inject_commit_sync_failure(path).is_none());
    }

    #[test]
    fn path_scoped_hooks_reach_canonical_aliases_after_normalization() {
        let temp_dir = tempfile::TempDir::new().expect("temp dir");
        let db_dir = temp_dir.path().join("db");
        std::fs::create_dir_all(&db_dir).expect("create db dir");
        let raw_alias = db_dir.join("..").join("db");
        let canonical = raw_alias.canonicalize().expect("canonical db dir");
        assert_eq!(canonical, db_dir.canonicalize().expect("canonical db dir"));
        assert_ne!(
            raw_alias, canonical,
            "test requires a lexical path alias distinct from canonical path"
        );

        clear_sync_failure(&raw_alias);
        clear_sync_failure(&canonical);
        inject_sync_failure(&raw_alias, io::ErrorKind::Other);
        assert!(
            maybe_inject_sync_failure(&canonical).is_some(),
            "sync failure hooks should reach canonical aliases"
        );
        assert!(maybe_inject_sync_failure(&raw_alias).is_none());

        inject_sync_failure(&canonical, io::ErrorKind::Other);
        clear_sync_failure(&raw_alias);
        assert!(
            maybe_inject_sync_failure(&canonical).is_none(),
            "sync failure clear should remove canonical alias keys"
        );

        clear_begin_sync_failure(&raw_alias);
        clear_begin_sync_failure(&canonical);
        inject_begin_sync_failure(&raw_alias, io::ErrorKind::Other);
        assert!(
            maybe_inject_begin_sync_failure(&canonical).is_some(),
            "begin sync failure hooks should reach canonical aliases"
        );
        assert!(maybe_inject_begin_sync_failure(&raw_alias).is_none());

        inject_begin_sync_failure(&canonical, io::ErrorKind::Other);
        clear_begin_sync_failure(&raw_alias);
        assert!(
            maybe_inject_begin_sync_failure(&canonical).is_none(),
            "begin sync failure clear should remove canonical alias keys"
        );

        clear_commit_sync_failure(&raw_alias);
        clear_commit_sync_failure(&canonical);
        inject_commit_sync_failure(&raw_alias, io::ErrorKind::Other);
        assert!(
            maybe_inject_commit_sync_failure(&canonical).is_some(),
            "commit sync failure hooks should reach canonical aliases"
        );
        assert!(maybe_inject_commit_sync_failure(&raw_alias).is_none());

        inject_commit_sync_failure(&canonical, io::ErrorKind::Other);
        clear_commit_sync_failure(&raw_alias);
        assert!(
            maybe_inject_commit_sync_failure(&canonical).is_none(),
            "commit sync failure clear should remove canonical alias keys"
        );

        clear_flush_thread_spawn_failure(&raw_alias);
        clear_flush_thread_spawn_failure(&canonical);
        inject_flush_thread_spawn_failure(&raw_alias);
        assert!(
            take_flush_thread_spawn_failure(&canonical),
            "flush thread spawn hooks should reach canonical aliases"
        );
        assert!(!take_flush_thread_spawn_failure(&raw_alias));

        inject_flush_thread_spawn_failure(&canonical);
        clear_flush_thread_spawn_failure(&raw_alias);
        assert!(
            !take_flush_thread_spawn_failure(&canonical),
            "flush thread spawn clear should remove canonical alias keys"
        );

        clear_clear_branch_storage_failure(&raw_alias);
        clear_clear_branch_storage_failure(&canonical);
        inject_clear_branch_storage_failure(&raw_alias, io::ErrorKind::Other);
        assert!(
            maybe_inject_clear_branch_storage_failure(&canonical).is_some(),
            "clear branch storage hooks should reach canonical aliases"
        );
        assert!(maybe_inject_clear_branch_storage_failure(&raw_alias).is_none());

        inject_clear_branch_storage_failure(&canonical, io::ErrorKind::Other);
        clear_clear_branch_storage_failure(&raw_alias);
        assert!(
            maybe_inject_clear_branch_storage_failure(&canonical).is_none(),
            "clear branch storage clear should remove canonical alias keys"
        );

        clear_clear_default_branch_marker_failure(&raw_alias);
        clear_clear_default_branch_marker_failure(&canonical);
        inject_clear_default_branch_marker_failure(&raw_alias, io::ErrorKind::Other);
        assert!(
            maybe_inject_clear_default_branch_marker_failure(&canonical).is_some(),
            "clear default branch marker hooks should reach canonical aliases"
        );
        assert!(maybe_inject_clear_default_branch_marker_failure(&raw_alias).is_none());

        inject_clear_default_branch_marker_failure(&canonical, io::ErrorKind::Other);
        clear_clear_default_branch_marker_failure(&raw_alias);
        assert!(
            maybe_inject_clear_default_branch_marker_failure(&canonical).is_none(),
            "clear default branch marker clear should remove canonical alias keys"
        );

        clear_halt_publish_pause(&raw_alias);
        clear_halt_publish_pause(&canonical);
        install_halt_publish_pause(&raw_alias);

        let producer_path = canonical.clone();
        let producer = std::thread::spawn(move || {
            maybe_pause_after_halt_health_publish(&producer_path);
        });
        assert!(
            wait_for_halt_publish_pause(&canonical, Duration::from_secs(1)),
            "canonical halt producer should reach raw alias pause installation"
        );
        release_halt_publish_pause(&raw_alias);
        producer.join().unwrap();
        clear_halt_publish_pause(&canonical);

        install_halt_publish_pause(&canonical);
        clear_halt_publish_pause(&raw_alias);
        let producer_path = canonical.clone();
        let producer = std::thread::spawn(move || {
            maybe_pause_after_halt_health_publish(&producer_path);
        });
        assert!(
            !wait_for_halt_publish_pause(&canonical, Duration::from_millis(1)),
            "halt publication clear should remove canonical alias keys"
        );
        producer.join().unwrap();
        clear_halt_publish_pause(&raw_alias);
        clear_halt_publish_pause(&canonical);
    }

    #[test]
    fn test_halt_publish_pause_hook_reaches_and_releases() {
        let path = Path::new("/tmp/test-halt-publish-pause");
        clear_halt_publish_pause(path);
        install_halt_publish_pause(path);

        let thread_path = path.to_path_buf();
        let handle = std::thread::spawn(move || {
            maybe_pause_after_halt_health_publish(&thread_path);
        });

        assert!(
            wait_for_halt_publish_pause(path, Duration::from_secs(1)),
            "pause hook should report when it is reached"
        );
        release_halt_publish_pause(path);
        handle.join().unwrap();
        clear_halt_publish_pause(path);
    }
}
