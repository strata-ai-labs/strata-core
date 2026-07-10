# Storage Testing Hardening (STH): closing the charter gaps

Status: draft program index
Charter: `docs/architecture/v1-storage-testing-taxonomy-and-gaps.md`
Code: **STH** — a storage-testing program, distinct from the ST2–ST7 architecture-cleanup slices.

This is a sequenced series of implementation plans that drives every storage-next
bug class to its charter exit bar. It is the execution arm of the testing
charter: the charter says *what world-class means per bug class*; these plans say
*how we get there, in what order, reusing what*.

## The bar we are holding (read this first)

**Success is measured as bug-class coverage at the charter exit bar — never as
lines of test code.** The 0.8:1 test-to-source ratio is a symptom worth noting,
not a target to chase; padding line count closes no bug class. Each plan's exit
gate is a *technique-coverage* statement ("every durable op is verified against
the prefix-of-acknowledged-history oracle under random kills"), and every plan
builds reusable machinery the next plan composes — so the suite gets *deeper*,
not just *bigger*. A plan that adds 2,000 lines and closes no class has failed;
a plan that adds 300 lines and moves a class from ❌ to ✅ has succeeded.

## The series

Ordered by dependency and leverage. Each plan is self-contained and can be run
via `/epic-implement`; each converges its own test work (these *are* test plans —
the harness is the implementation).

| Plan | Closes (charter class) | Status → exit | Depends on | The story it tells |
|---|---|---|---|---|
| [STH-1](sth-1-recovery-oracle-implementation-plan.md) | 4 Recovery oracle | ❌ → ✅ | — | "We don't check that it reopened; we check we got the *right data* back." |
| [STH-2](sth-2-fault-injection-sweeps-implementation-plan.md) | 5 Error-path sweeps | 🟡 → ✅ | STH-1 | "We fail *every* I/O operation, not a few we guessed." |
| [STH-3](sth-3-durability-realism-implementation-plan.md) | 3 Crash, 10 FS-assumptions | 🟡/❌ → ✅ | STH-1 | "We model the disk that lies — torn writes, reordering, non-atomic rename." |
| [STH-4](sth-4-deterministic-simulation-implementation-plan.md) | 9 Deterministic simulation | 🟡 → ✅ | STH-1, STH-2 | "Every failure replays from a seed; we sweep the interleavings nothing else reaches." |
| [STH-5](sth-5-failure-during-failure-implementation-plan.md) | 6 Failure-during-failure | ❌ → ✅ | STH-1/2/3 | "We break recovery *while it is recovering* and it still holds." |
| [STH-6](sth-6-differential-and-liveness-implementation-plan.md) | 2 Differential, 8 Liveness | 🟡/✅ → ✅ | — | "Same workload, every config, identical results — and it never falls behind." |
| [STH-7](sth-7-test-process-gates-implementation-plan.md) | 11 Coverage/mutation, 12 Memory safety, 7 (deepen), anti-drift | ❌/🟡 → ✅ | suites exist | "The discipline is enforced by CI, and the map can't lie." |

Classes 1 (contract) and 7 (hostile input) are already at exit bar; STH-7 only
*deepens* 7 (continuous + structure-aware fuzz) and adds the anti-drift guard.

## Sequencing

```
STH-1 (oracle) ──┬─→ STH-2 (fault sweeps) ──┐
                 ├─→ STH-3 (durability realism) ──┼─→ STH-5 (failure-during-failure)
                 └─→ STH-4 (DST driver) ──────────┘
STH-6 (differential + liveness) ── independent, run anytime
STH-7 (process gates) ── Miri/sanitizer cheap now; coverage/mutation after suites exist
```

STH-1 is foundational: the recovery oracle it builds is the post-condition check
that STH-2, STH-3, STH-4, and STH-5 all reuse. Build it first and well. STH-6 and
the cheap half of STH-7 (Miri/sanitizer CI) can run in parallel from day one.

## The blog arc ("How we test StrataDB")

The plans are written so their execution *is* the blog outline. The narrative
escalates with the taxonomy: contract → silent-wrong-result → crash → silent
data loss → error paths → failure-during-failure → hostile input → liveness →
deterministic simulation → filesystem lies → the discipline that enforces it.
The throughline is the charter's thesis — *coverage in one class says nothing
about another* — and the punchline is the deterministic-simulation driver
(STH-4): the moment a database can replay any failure from a seed is the moment
its testing story becomes credible. Each plan's "Why this matters" section is a
draft of its blog beat.

## Execution discipline (applies to every plan)

- **Definition of done = the class exit bar**, demonstrated by coverage, not LOC.
- **Reuse, don't duplicate**: STH-2/3/4/5 assert through the STH-1 oracle.
- **Determinism**: every new harness is seeded; failures print the seed and replay.
- **Behavioral names only**: no `STH`/class codes in test identifiers (codes live
  in PR titles and these plans). Tests assert error *class and code*, not text.
- **Regression protocol**: any bug a plan finds becomes a permanent test + a
  corpus/seed entry before the fix lands.
