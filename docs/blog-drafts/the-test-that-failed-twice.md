# The Test That Failed Twice Before It Passed

*How a 40-line oracle test caught two shipping bugs in a "safe" storage-engine refactor — and what it taught me about testing recovery paths.*

---

Some refactors feel safe. This one had every credential: it was a pure cache, the old code path stayed alive as the source of truth, and the design guaranteed — by construction — that the worst possible failure was yesterday's performance, never a wrong answer.

The test I wrote to prove it safe failed twice. Both failures were real bugs. Both would have shipped. And the second one revealed that the code path I had carefully wired up doesn't actually run in production at all.

## The setup

Strata is an embedded database (think SQLite's deployment model) with time-travel reads: you can ask for a key *as of* any timestamp, and the engine resolves that timestamp to a commit version and reads the world as it existed then.

The resolution step needed an index. The engine records one `(commit_version, commit_timestamp)` fact per commit, and the original implementation answered every `as_of` query by scanning all of those facts and rebuilding a lookup structure from scratch — a placeholder the code itself confessed to, in a comment, with the classic "we should fix this before it becomes a hot path."

The fix is obvious: keep a retained, append-only index of those facts per branch. Append on every commit, binary-search on every query. O(log n) instead of O(everything).

The interesting part isn't the index. It's the two questions every derived structure has to answer:

1. **How do you know the index is *complete*** — that it covers all history, not just the commits it happened to see?
2. **What happens after a restart**, when the in-memory index is gone but the history isn't?

## Design: make wrong answers impossible, then test anyway

The index carries an explicit *exactness contract*: every lookup must either prove it's equivalent to the old scan, or refuse to answer — in which case the caller falls back to the scan, which remains the source of truth.

Concretely, the index keeps two flags. `complete` says the entries cover all retained history (set when the index is seeded from a full scan, or when the branch was born empty in-process). A monotonicity flag guards the binary search's soundness. Any inconsistency — a version that goes backwards, a timestamp that disagrees with a recorded entry — doesn't try to repair anything. It *poisons* the index: `complete` flips false, forever, and every future lookup falls back to the scan.

This is a design pattern worth naming: **poison-to-fallback**. When your fast path can't prove it's right, the failure mode should be "use the slow path," not "return the fast answer and hope." It means the correctness argument for the whole feature reduces to one claim: *the fast path only fires when equivalence is provable.*

For restarts, the index gets persisted at checkpoint time and restored at recovery, with the write-ahead log's replay filling in whatever came after the checkpoint.

At this point I had: a sound design, a differential test (index answers ≡ scan answers over randomized histories), unit tests on every poisoning rule, and a green full test suite. The kind of position where you ship.

## The oracle

Instead I wrote one more test, and it's the reason this post exists. The test asserts the *mechanism*, not just the answers:

```text
1. Open a database, commit at timestamps 10 and 30.
2. Run one as_of read      → seeds the index from a scan.
3. Checkpoint. Close.
4. Reopen.
5. ASSERT: the index is already complete — BEFORE any read runs.
6. Read as_of(20) and as_of(30) → answers must match history.
```

Step 5 is the whole point. Without it, the test passes even if persistence is completely broken — because the first read would just quietly fall back to the scan and re-seed. The answers would be right, the feature would be a no-op, and no test would ever know. When your design degrades gracefully, *graceful degradation is exactly what your tests will hide.*

## Failure #1: recovery poisoned its own restoration

The oracle failed. The index arrived at reopen complete... and then wasn't.

The culprit was an interaction between two individually-correct behaviors. Write-ahead log replay is *idempotent by design*: after a crash or restart, it re-applies commits, including some the checkpoint already covers. That's normal and safe — row application tolerates it.

But my index's corruption detection treated any observation of a version at-or-below its current tip as an inconsistency. So recovery would restore a pristine, complete index from the checkpoint — and then WAL replay would re-observe commit #1, the poison rule would fire, and the index would demote itself back to fallback-forever. The restoration and the replay were each doing their job. Together they guaranteed the feature never worked after any restart.

The fix distinguishes *re-observation* from *inconsistency*: an observation that exactly matches an existing entry (same version, same timestamp) is a no-op; a mismatch or a gap still poisons. One binary search, one honest question — "have I seen exactly this fact before?"

Bugs like this live in the seams between correct components. No unit test of the index or of replay would find it, because each side is right. Only a test that runs the *actual sequence* — checkpoint, close, reopen, replay — walks through the seam.

## Failure #2: the production path was not the path I wired

Fixed the poison rule. Oracle still red. The index wasn't complete at reopen — as if the checkpoint had never written it.

Here's where I stopped reasoning and started instrumenting. Two `eprintln!` lines: one where the checkpoint gathers the index for persistence, one where recovery restores it. Ran the test.

**Neither line printed.**

Not "printed the wrong thing." *Never executed.* I had wired the persistence into the checkpoint function that reads as the canonical entry point — the one whose name and signature say "this is where checkpoints happen." It turns out that path exists, compiles, has callers, and is essentially never what production uses. The real checkpoint writer is an off-lock background build: a maintenance task pre-collects everything it needs *before* taking any locks, then a later phase publishes it — an artifact of earlier work that moved every byte of I/O out of lock scope. That builder gathered rows, watermarks, boundaries... and passed an empty list where my timeline data should have been, because I'd added the parameter with a harmless-looking default at the one call site I hadn't traced.

There's a compounding subtlety: the test's `close()` didn't trigger a checkpoint either, because the close-time checkpoint is policy-gated and skips tiny write-ahead-log tails. So even the test had to learn to request a checkpoint explicitly — which is itself the kind of thing you only discover by watching what actually runs.

The lesson I keep re-learning, and that this session finally tattooed on me: **when behavior depends on which of several plausible paths executes, stop reading code and make the code tell you.** Two print statements settled in ninety seconds what code-reading had gotten confidently wrong. In a mature codebase, "the function that looks canonical" and "the function production calls" drift apart — deliberately, for good reasons (lock discipline, in this case), and nothing forces your mental model to drift with them.

## What survived contact

Three things I'd defend as general practice now:

**Test the mechanism, not just the answers.** If your feature has a fallback, answer-checking tests validate the fallback. Assert the state that proves the fast path is live — "complete before any read" — even if it needs a test-only accessor. The 40-line oracle found two bugs that a 3,000-test green suite, a differential test, and a sound correctness argument all missed.

**Design poison-to-fallback, then trust it.** Both bugs became *performance* bugs instead of correctness bugs because the design refuses to serve unproven answers. That's what bought the freedom to refactor recovery machinery at all. The failure hierarchy you want: impossible > loud > slow > wrong. Spend design effort moving failures up that ladder.

**Instrument before you theorize.** My prior for "the checkpoint function named checkpoint is the checkpoint path" was high, and it was wrong. Print statements, counters, or a debugger — anything that reports what executed beats any amount of inference about what should execute.

And one bonus lesson from the same afternoon: our test suite includes *vocabulary guards* — tests that grep the source and reject forbidden words on certain API surfaces. My function named `restore_...` was rejected because the branch API surface bans product verbs like "restore" (reserved for user-facing operations that don't exist yet). Five minutes of renaming, and future readers will never confuse an internal cache-seeding step with a user-facing restore. Guards that police *language* look pedantic until the day they prevent a concept collision.

The refactor shipped. The rows it will eventually replace are still being written — they're the oracle for the next slice, which deletes them and collects a 3× reduction in per-commit write volume. That test will get to fail on some bugs of its own first. I'm counting on it.

---

*Strata is an embedded database with branches, time travel, and built-in retrieval, currently in a ground-up V1 rewrite. This post describes work on its storage engine.*
