//! #2524: the in-flight build-output registry.
//!
//! Off-lock builds (flush, compaction, materialization) publish table
//! objects BEFORE the manifest fold that makes them reachable. To the
//! table-object mark those objects are indistinguishable from garbage, so
//! the sweep/retention path historically deferred WHOLESALE while any build
//! was active — which starved reclaim entirely under sustained load
//! (measured: zero retention runs across a whole seed; ~9-17x transient
//! space debt).
//!
//! This registry replaces the wholesale interlock with precise pins: a
//! build reserves each output's object name BEFORE publishing its bytes,
//! and the mark unions the reserved names into its pinned set. Reservations
//! are dropped when the build's prepared value is consumed or abandoned —
//! by then the objects are either manifest-reachable and covered by the
//! in-memory pins (install happens under the same lock hold that re-pins
//! them), or orphaned and correctly sweepable.
//!
//! The registry is in-memory by design: after a crash, published-but-
//! uninstalled outputs ARE garbage, and the fresh-open mark (no builds
//! active) reclaims them.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use crate::object::ObjectName;

/// Shared registry handle. Cloning shares the underlying set.
///
/// Names are reference-counted: idempotent publish retries and cloned
/// prepared values may reserve the same name from concurrent guards, and a
/// pin must survive until the LAST holder drops.
#[derive(Clone, Debug, Default)]
pub(crate) struct InFlightTableOutputs {
    names: Arc<Mutex<BTreeMap<ObjectName, usize>>>,
}

impl InFlightTableOutputs {
    /// Open a guard for one build. Reservations made through the guard are
    /// released together when the guard (and every clone of the prepared
    /// value holding it) drops.
    pub(crate) fn guard(&self) -> InFlightOutputsGuard {
        InFlightOutputsGuard {
            registry: self.clone(),
            reserved: Mutex::new(Vec::new()),
        }
    }

    /// The currently reserved names, for the mark's pinned-object union.
    pub(crate) fn snapshot(&self) -> Vec<ObjectName> {
        self.names
            .lock()
            .map(|names| names.keys().cloned().collect())
            .unwrap_or_default()
    }

    fn reserve(&self, name: &ObjectName) {
        if let Ok(mut names) = self.names.lock() {
            *names.entry(name.clone()).or_insert(0) += 1;
        }
    }

    fn release(&self, name: &ObjectName) {
        if let Ok(mut names) = self.names.lock() {
            if let Some(count) = names.get_mut(name) {
                *count = count.saturating_sub(1);
                if *count == 0 {
                    names.remove(name);
                }
            }
        }
    }
}

/// One build's reservations. `reserve` is `&self` (internally locked) so the
/// per-output publishing sink — including parallel subcompactions — can pin
/// as it goes without exclusive access.
#[derive(Debug)]
pub(crate) struct InFlightOutputsGuard {
    registry: InFlightTableOutputs,
    reserved: Mutex<Vec<ObjectName>>,
}

impl InFlightOutputsGuard {
    /// Pin an output name. MUST be called before the object's bytes are
    /// published — reserving after the publish leaves a window where the
    /// mark sees an unreachable, unpinned object and the sweep deletes it
    /// out from under the build.
    pub(crate) fn reserve(&self, name: ObjectName) {
        self.registry.reserve(&name);
        if let Ok(mut reserved) = self.reserved.lock() {
            reserved.push(name);
        }
    }
}

impl Drop for InFlightOutputsGuard {
    fn drop(&mut self) {
        if let Ok(reserved) = self.reserved.get_mut() {
            for name in reserved.iter() {
                self.registry.release(name);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn name(suffix: &str) -> ObjectName {
        crate::layout::ObjectLayout::table_object("b", 0, suffix).expect("valid object name")
    }

    #[test]
    fn reservations_pin_until_the_last_guard_drops() {
        let registry = InFlightTableOutputs::default();
        let first = registry.guard();
        let second = registry.guard();
        first.reserve(name("t1"));
        second.reserve(name("t1"));
        second.reserve(name("t2"));
        assert_eq!(registry.snapshot().len(), 2);
        drop(second);
        // t1 survives: the first guard still holds it.
        assert_eq!(registry.snapshot(), vec![name("t1")]);
        drop(first);
        assert!(registry.snapshot().is_empty());
    }
}
