# Storage-next lock-decoupling roadmap — RETIRED

**Retired 2026-06-30. Superseded by the authoritative plan:**
`docs/architecture/implementation-plans/M4P/m4p-l8i-runtime-lock-decoupling-implementation-plan.md`

This draft was written before discovering M4P-L8I and largely duplicated it — less
completely (it missed **Group B: WAL fsync off the commit lock**, L8I's single biggest
foreground lever, and under-specified the crash-consistency / per-branch-publish rigor and
the "fold derived facts into the swapped `Arc`" subtlety). L8I already has the old engine as
its blueprint, a Required-Invariants section, and a paired test plan.

The convoy root cause and this session's findings (the fifth under-lock cost — the O(rows)
flush-watermark coverage scan, now tactically bounded — and the workload-F / crawl-rate
validation signal) are folded into the L8I plan's **Update — 2026-06-30** section.

This roadmap's only non-overlapping idea — **concurrent disjoint-level compaction** — is a
*drain-rate* lever, complementary to and out of scope for the lock decoupling; track it with
the M12C-style compaction work.

Root cause: `docs/design/performance/durable-background-lock-convoy.md`.
