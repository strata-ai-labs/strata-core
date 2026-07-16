//! Intentional test-fixture leaks that stay visible to leak checkers.

use std::sync::Mutex;

/// Addresses of intentionally leaked fixtures. Holding each address in
/// a process global keeps the allocation reachable, so `LeakSanitizer`
/// attributes it to this registry instead of reporting it — real leaks
/// elsewhere still fail the sanitizer lane.
static LEAKED_FIXTURES: Mutex<Vec<usize>> = Mutex::new(Vec::new());

/// Leaks `value` for the remainder of the process and returns the
/// `&'static` reference fixtures need.
///
/// Test code must use this instead of a bare `Box::leak` so the nightly
/// ASAN lane can run with leak detection enabled: bare fixture leaks
/// drown the report in known-benign noise.
pub fn leak_static<T>(value: T) -> &'static T {
    let leaked: &'static T = Box::leak(Box::new(value));
    LEAKED_FIXTURES
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .push(std::ptr::from_ref(leaked) as usize);
    leaked
}

/// Skips `value`'s destructor exactly like `std::mem::forget`, while
/// registering the heap allocation so leak checkers attribute it here.
///
/// For crash-simulation fixtures that borrow stack locals and therefore
/// cannot satisfy [`leak_static`]'s `'static` bound.
pub fn forget_registered<T>(value: T) {
    let address = Box::into_raw(Box::new(value)) as usize;
    LEAKED_FIXTURES
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .push(address);
}
