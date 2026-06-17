# M4P Slice Plans

This directory holds detailed implementation and test plans for the M4P
storage-next parity-restoration program.

Parent plan:
`docs/architecture/implementation-plans/m4p-storage-next-parity-restoration-implementation-plan.md`

Test methodology:
`docs/architecture/implementation-plans/m4p-storage-next-parity-restoration-test-plan.md`

Required audit sources:

1. `docs/architecture/perf-tuning/storage-next-mechanics-parity-audit.md`
2. `docs/architecture/perf-tuning/storage-next-serving-path-parity-plan.md`

Supporting perf evidence:

1. `docs/architecture/perf-tuning/perf-p*.md`
2. `docs/architecture/perf-tuning/perf-i*.md`

The mechanics audit is the source of required parity findings. Slice plans must
cite the relevant L1-L9 layer audit section and, when applicable, the
Restoration Source Map, Audit Matrix, and Final Parity Matrix sections. The
serving-path proof plan and perf reports must be cited for point-read, load,
scan, compaction, and source-fanout work.

## Naming

Use the existing M4 slice naming style:

1. implementation plan:
   `m4p-l6b-nonzero-level-point-pruning-implementation-plan.md`
2. test plan:
   `m4p-l6b-nonzero-level-point-pruning-test-plan.md`

Keep slice labels in planning documents only. Do not put `M4P`, `L6B`, or other
roadmap labels in production Rust identifiers, comments, fixture bytes, panic
messages, or user-visible text.

## Required Slice Contents

Each slice pair must record:

1. objective;
2. audit finding references by file and section heading;
3. old-source map and storage-next target map;
4. predecessors and exact lower-layer dependencies;
5. implementation scope and non-goals;
6. correctness, crash/fault, source-guard, fuzz/generated, and benchmark gates;
7. expected mechanical counter movement for performance-sensitive work;
8. a stop condition if the measured counter movement does not appear.

If a slice defers an audit finding, the slice must record the owner layer,
reason, and replacement proof or later slice that will close it.

## Written Slice Plans

1. `m4p-l1-backend-io-parity-implementation-plan.md`
2. `m4p-l1-backend-io-parity-test-plan.md`
3. `m4p-l2-object-layout-parity-implementation-plan.md`
4. `m4p-l2-object-layout-parity-test-plan.md`
5. `m4p-l3-durable-format-parity-implementation-plan.md`
6. `m4p-l3-durable-format-parity-test-plan.md`
7. `m4p-l4-durable-service-parity-implementation-plan.md`
8. `m4p-l4-durable-service-parity-test-plan.md`
9. `m4p-l5-table-runtime-parity-implementation-plan.md`
10. `m4p-l5-table-runtime-parity-test-plan.md`
11. `m4p-l6-branch-lsm-runtime-parity-implementation-plan.md`
12. `m4p-l6-branch-lsm-runtime-parity-test-plan.md`
13. `m4p-l6j-l0-l7-compaction-closure-implementation-plan.md`
14. `m4p-l6j-l0-l7-compaction-closure-test-plan.md`
15. `m4p-l6k-table-compaction-hot-path-implementation-and-test-plan.md`
16. `m4p-l6l-branch-read-hot-path-implementation-plan.md`
17. `m4p-l6l-branch-read-hot-path-test-plan.md`
18. `m4p-l7-commit-runtime-parity-implementation-plan.md`
19. `m4p-l7-commit-runtime-parity-test-plan.md`
20. `m4p-l8-automatic-maintenance-scheduling-followup.md`
21. `m4p-l8-lifecycle-maintenance-parity-implementation-plan.md`
22. `m4p-l8-lifecycle-maintenance-parity-test-plan.md`
23. `m4p-l8b-lifecycle-maintenance-followup-implementation-plan.md`
24. `m4p-l8b-lifecycle-maintenance-followup-test-plan.md`
25. `m4p-l8e-background-maintenance-executor-implementation-plan.md`
26. `m4p-l8e-background-maintenance-executor-test-plan.md`
27. `m4p-l8c-lifecycle-recovery-close-parity-implementation-plan.md`
28. `m4p-l8c-lifecycle-recovery-close-parity-test-plan.md`
29. `m4p-l8d-durable-table-manifest-row-pruning-parity-implementation-plan.md`
30. `m4p-l8d-durable-table-manifest-row-pruning-parity-test-plan.md`
31. `m4p-l8f-load-performance-stabilization-implementation-plan.md`
32. `m4p-l8f-load-performance-stabilization-test-plan.md`
33. `m4p-l8g-cache-mode-lifecycle-policy-implementation-plan.md`
34. `m4p-l8g-cache-mode-lifecycle-policy-test-plan.md`

## Initial Slice Queue

The parent plan defines the first executable queue:

1. `M4P-L1A`: IO boundary guard and durable-delete decision scope.
2. `M4P-L2A`: object/table classifier helpers.
3. `M4P-L3A`: format-impact decision.
4. `M4P-L4A`: touched publication/recovery windows.
5. `M4P-L5A`: table/source counters and facts.
6. `M4P-L6A`: branch source-layout counters.
7. `M4P-L6B`: nonzero-level point-read pruning.
8. `M4P-L6C`: lazy nonzero-level scan planning.
9. `M4P-L6D`: history, timestamp, and hot-facts boundedness.
10. `M4P-L8A`: automatic maintenance scheduling.
11. `M4P-L8B`: score-based compaction drain.
12. `M4P-L8C`: write-admission and pressure policy.
13. `M4P-L9A`: diagnostics boundary.
14. `M4P-L9B`: storage-shaped read-set facts.

The L8A-L8C queue items are planned together in
`m4p-l8-lifecycle-maintenance-parity-implementation-plan.md` and
`m4p-l8-lifecycle-maintenance-parity-test-plan.md` because post-commit
scheduling, compaction chaining, and admission pressure share one source-shape
exit gate.
