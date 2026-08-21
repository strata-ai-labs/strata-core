//! Host memory facts for the derived default storage budget (#2905).
//!
//! The storage budget is a ceiling, never a reservation — the block cache and
//! memtables grow with use — so deriving the DEFAULT ceiling from the host at
//! open time lets an un-budgeted open fit a small device without committing
//! memory on a large one. Policy: one quarter of the host's usable memory —
//! the smaller of `MemAvailable` and the cgroup limit, because a container
//! limit must win over host RAM or the OOM merely relocates — clamped to
//! `[1 MiB, 8 GiB]`. Deployments wanting more set an explicit `memory_budget`;
//! `STRATA_HOST_MEMORY_BYTES` overrides the probe for deterministic lanes.
//!
//! Platform scope: the probe reads `/proc/meminfo` and the cgroup v2/v1 limit
//! files with std only (this crate denies `unsafe`). Where none exist (macOS,
//! Windows, wasm) it reports no facts and the open falls back to the fixed
//! default with `FixedDefault` provenance.

use std::path::Path;

/// Divisor applied to usable host memory: the derived default claims 25%.
pub(crate) const DERIVED_BUDGET_DIVISOR: u64 = 4;
/// Upper bound of the derived default. Defaults serve the common case; a
/// 384 GiB host must not derive 96 GiB (#2905) — larger deployments opt in.
pub(crate) const DERIVED_BUDGET_CEILING_BYTES: u64 = 8 * 1024 * 1024 * 1024;
/// Lower bound of the derived default: the minimum supported storage budget.
pub(crate) const DERIVED_BUDGET_FLOOR_BYTES: u64 = 1024 * 1024;
/// Probe override (usable host memory in bytes) for CI, DST, and perf lanes.
pub(crate) const HOST_MEMORY_OVERRIDE_ENV: &str = "STRATA_HOST_MEMORY_BYTES";

const PROC_MEMINFO: &str = "/proc/meminfo";
const CGROUP_V2_LIMIT: &str = "/sys/fs/cgroup/memory.max";
const CGROUP_V1_LIMIT: &str = "/sys/fs/cgroup/memory/memory.limit_in_bytes";

/// What the host reports about memory it can actually give this process.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct HostMemoryFacts {
    /// `MemAvailable` (or the override), in bytes.
    pub(crate) available_bytes: Option<u64>,
    /// The enclosing cgroup's memory limit, in bytes, when one is set.
    pub(crate) cgroup_limit_bytes: Option<u64>,
}

impl HostMemoryFacts {
    /// The memory the process may reasonably use: the smaller of the two facts.
    pub(crate) fn usable_bytes(self) -> Option<u64> {
        match (self.available_bytes, self.cgroup_limit_bytes) {
            (Some(available), Some(limit)) => Some(available.min(limit)),
            (Some(available), None) => Some(available),
            (None, Some(limit)) => Some(limit),
            (None, None) => None,
        }
    }
}

/// A derived default budget together with the host basis it came from.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct DerivedBudget {
    /// 25% of `usable_host_bytes`, clamped to `[FLOOR, CEILING]`.
    pub(crate) total_bytes: u64,
    /// The usable host memory the derivation started from.
    pub(crate) usable_host_bytes: u64,
}

/// The derived default budget: 25% of usable memory, clamped. `None` when the
/// host reports nothing — the caller falls back to the fixed default.
pub(crate) fn derive_default_budget(facts: HostMemoryFacts) -> Option<DerivedBudget> {
    let usable_host_bytes = facts.usable_bytes()?;
    Some(DerivedBudget {
        total_bytes: (usable_host_bytes / DERIVED_BUDGET_DIVISOR)
            .clamp(DERIVED_BUDGET_FLOOR_BYTES, DERIVED_BUDGET_CEILING_BYTES),
        usable_host_bytes,
    })
}

/// The derived total alone — the truth-table surface exercised by the unit
/// tests; production reads the full [`DerivedBudget`] via [`derive_default_budget`].
#[cfg(test)]
pub(crate) fn derive_default_budget_bytes(facts: HostMemoryFacts) -> Option<u64> {
    derive_default_budget(facts).map(|derived| derived.total_bytes)
}

/// Probe the host: the override first, then the platform files.
pub(crate) fn probe() -> HostMemoryFacts {
    let override_value = std::env::var(HOST_MEMORY_OVERRIDE_ENV).ok();
    probe_with_override(override_value.as_deref())
}

/// The probe with the override injected (tests pass it directly; no env mutation).
pub(crate) fn probe_with_override(override_value: Option<&str>) -> HostMemoryFacts {
    if let Some(bytes) = override_value.and_then(|value| value.trim().parse::<u64>().ok()) {
        return HostMemoryFacts {
            available_bytes: Some(bytes),
            cgroup_limit_bytes: None,
        };
    }
    probe_files(
        Path::new(PROC_MEMINFO),
        &[Path::new(CGROUP_V2_LIMIT), Path::new(CGROUP_V1_LIMIT)],
    )
}

/// Read the facts from explicit paths; an absent or unreadable file is simply no fact.
pub(crate) fn probe_files(meminfo: &Path, cgroup_limits: &[&Path]) -> HostMemoryFacts {
    let available_bytes = std::fs::read_to_string(meminfo)
        .ok()
        .and_then(|text| parse_meminfo_available(&text));
    let cgroup_limit_bytes = cgroup_limits.iter().find_map(|path| {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|text| parse_cgroup_limit(&text))
    });
    HostMemoryFacts {
        available_bytes,
        cgroup_limit_bytes,
    }
}

/// `MemAvailable:  8159254 kB` → bytes. Absent or malformed → `None`.
pub(crate) fn parse_meminfo_available(text: &str) -> Option<u64> {
    text.lines().find_map(|line| {
        let rest = line.strip_prefix("MemAvailable:")?;
        let mut parts = rest.split_whitespace();
        let value: u64 = parts.next()?.parse().ok()?;
        match parts.next().unwrap_or("kB") {
            "kB" => value.checked_mul(1024),
            "B" => Some(value),
            _ => None,
        }
    })
}

/// cgroup v1 reports "no limit" as the largest page-aligned `i64`; anything at
/// or above it is unlimited, as is v2's literal `max`.
const CGROUP_V1_UNLIMITED_FLOOR: u64 = (i64::MAX as u64) & !4095;

/// cgroup v2 `memory.max` / v1 `memory.limit_in_bytes` → a limit, or `None` when unlimited or malformed.
pub(crate) fn parse_cgroup_limit(text: &str) -> Option<u64> {
    let trimmed = text.trim();
    if trimmed == "max" {
        return None;
    }
    let value: u64 = trimmed.parse().ok()?;
    if value >= CGROUP_V1_UNLIMITED_FLOOR {
        return None;
    }
    Some(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIB: u64 = 1024 * 1024;
    const GIB: u64 = 1024 * MIB;

    fn facts(available: Option<u64>, limit: Option<u64>) -> HostMemoryFacts {
        HostMemoryFacts {
            available_bytes: available,
            cgroup_limit_bytes: limit,
        }
    }

    #[test]
    fn derivation_is_a_quarter_of_usable_memory() {
        assert_eq!(
            derive_default_budget_bytes(facts(Some(16 * GIB), None)),
            Some(4 * GIB)
        );
    }

    #[test]
    fn derivation_takes_the_smaller_of_host_and_cgroup() {
        // A 512 MiB container on a 64 GiB host must derive from the container limit.
        assert_eq!(
            derive_default_budget_bytes(facts(Some(64 * GIB), Some(512 * MIB))),
            Some(128 * MIB)
        );
        // And the host bound wins when the cgroup limit is looser than available memory.
        assert_eq!(
            derive_default_budget_bytes(facts(Some(2 * GIB), Some(64 * GIB))),
            Some(512 * MIB)
        );
        // A cgroup limit alone is enough to derive from.
        assert_eq!(
            derive_default_budget_bytes(facts(None, Some(GIB))),
            Some(256 * MIB)
        );
    }

    #[test]
    fn derivation_clamps_to_the_ceiling_on_large_hosts() {
        // 384 GiB host: 25% would be 96 GiB — the ceiling holds it to 8 GiB.
        assert_eq!(
            derive_default_budget_bytes(facts(Some(384 * GIB), None)),
            Some(DERIVED_BUDGET_CEILING_BYTES)
        );
        assert_eq!(DERIVED_BUDGET_CEILING_BYTES, 8 * GIB);
        // Exactly at the ceiling's source is not clamped.
        assert_eq!(
            derive_default_budget_bytes(facts(Some(32 * GIB), None)),
            Some(8 * GIB)
        );
    }

    #[test]
    fn derivation_clamps_to_the_floor_on_tiny_hosts() {
        // A 2 MiB reading derives below the minimum supported budget; the floor lifts it.
        assert_eq!(
            derive_default_budget_bytes(facts(Some(2 * MIB), None)),
            Some(DERIVED_BUDGET_FLOOR_BYTES)
        );
        assert_eq!(DERIVED_BUDGET_FLOOR_BYTES, MIB);
        // A Pi-Zero-class 350 MiB device opens with ~87 MiB.
        assert_eq!(
            derive_default_budget_bytes(facts(Some(350 * MIB), None)),
            Some(350 * MIB / 4)
        );
    }

    #[test]
    fn derivation_yields_nothing_without_facts() {
        assert_eq!(derive_default_budget_bytes(facts(None, None)), None);
    }

    #[test]
    fn meminfo_parser_reads_mem_available_in_kib() {
        let text = "MemTotal:       16318508 kB\nMemFree:         1234 kB\nMemAvailable:    8159254 kB\nBuffers:          1 kB\n";
        assert_eq!(parse_meminfo_available(text), Some(8_159_254 * 1024));
    }

    #[test]
    fn meminfo_parser_accepts_an_explicit_byte_unit() {
        // The `B` arm: a value already in bytes is taken as-is.
        assert_eq!(
            parse_meminfo_available("MemAvailable: 4096 B\n"),
            Some(4096)
        );
        // kB is scaled by 1024; the two units must not collapse.
        assert_ne!(
            parse_meminfo_available("MemAvailable: 4096 B\n"),
            parse_meminfo_available("MemAvailable: 4096 kB\n")
        );
    }

    #[test]
    fn meminfo_parser_rejects_missing_or_malformed_lines() {
        assert_eq!(parse_meminfo_available("MemTotal: 1 kB\n"), None);
        assert_eq!(parse_meminfo_available("MemAvailable: lots kB\n"), None);
        assert_eq!(parse_meminfo_available("MemAvailable: 5 MB\n"), None);
        assert_eq!(parse_meminfo_available(""), None);
    }

    #[test]
    fn cgroup_parser_handles_v2_max_v1_sentinel_and_numbers() {
        assert_eq!(parse_cgroup_limit("max\n"), None);
        assert_eq!(parse_cgroup_limit("536870912\n"), Some(512 * MIB));
        // cgroup v1 "no limit" is the largest page-aligned i64.
        assert_eq!(parse_cgroup_limit("9223372036854771712\n"), None);
        assert_eq!(parse_cgroup_limit("9223372036854775807"), None);
        assert_eq!(parse_cgroup_limit("garbage"), None);
        assert_eq!(parse_cgroup_limit(""), None);
    }

    #[test]
    fn override_bypasses_the_platform_probe() {
        let facts = probe_with_override(Some("123456789"));
        assert_eq!(facts.available_bytes, Some(123_456_789));
        assert_eq!(facts.cgroup_limit_bytes, None);
        // A malformed override is ignored, not trusted.
        let facts = probe_with_override(Some("not-a-number"));
        assert!(facts.available_bytes.is_none() || facts.available_bytes != Some(0));
    }

    #[test]
    fn probe_honors_the_env_override() {
        // Serialized via a process-global guard: probe() reads a real env var.
        static ENV_GUARD: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _guard = ENV_GUARD
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let prior = std::env::var(HOST_MEMORY_OVERRIDE_ENV).ok();
        std::env::set_var(HOST_MEMORY_OVERRIDE_ENV, "2147483648");
        let facts = probe();
        match prior {
            Some(value) => std::env::set_var(HOST_MEMORY_OVERRIDE_ENV, value),
            None => std::env::remove_var(HOST_MEMORY_OVERRIDE_ENV),
        }
        // A Default::default() probe would ignore the override entirely.
        assert_eq!(facts.available_bytes, Some(2 * GIB));
        assert_eq!(facts.cgroup_limit_bytes, None);
    }

    #[test]
    fn file_probe_reads_both_sources_and_tolerates_absence() {
        let dir = tempfile::tempdir().expect("tempdir");
        let meminfo = dir.path().join("meminfo");
        let limit = dir.path().join("memory.max");
        std::fs::write(&meminfo, "MemAvailable: 1024 kB\n").expect("write meminfo");
        std::fs::write(&limit, "2097152\n").expect("write limit");
        let facts = probe_files(&meminfo, &[&limit]);
        assert_eq!(facts.available_bytes, Some(MIB));
        assert_eq!(facts.cgroup_limit_bytes, Some(2 * MIB));

        let missing = dir.path().join("missing");
        let facts = probe_files(&missing, &[&missing]);
        assert_eq!(facts, HostMemoryFacts::default());
    }
}
