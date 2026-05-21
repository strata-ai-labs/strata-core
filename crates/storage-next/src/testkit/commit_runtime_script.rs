//! Bounded generated commit-runtime scripts for L7M assurance.

use strata_core_next::BranchId;

pub(crate) const COMMIT_RUNTIME_SCRIPT_MAX_BRANCHES: usize = 8;
pub(crate) const COMMIT_RUNTIME_SCRIPT_MAX_OPS: usize = 64;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CommitRuntimeScript {
    branches: [BranchId; COMMIT_RUNTIME_SCRIPT_MAX_BRANCHES],
    operations: [CommitScriptOperation; COMMIT_RUNTIME_SCRIPT_MAX_OPS],
    operation_count: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CommitScriptOperation {
    CachePut {
        branch: u8,
        key: u8,
        value: u8,
    },
    CacheDelete {
        branch: u8,
        key: u8,
    },
    DurablePut {
        branch: u8,
        key: u8,
        value: u8,
        fault: CommitScriptDurableFault,
    },
    DurableDelete {
        branch: u8,
        key: u8,
        fault: CommitScriptDurableFault,
    },
    ConflictPut {
        branch: u8,
        key: u8,
        value: u8,
    },
    ReadOnlyDiagnostic {
        branch: u8,
    },
    BeginQuiesce,
    ReleaseQuiesce,
    AcquireBranchGuard {
        branch: u8,
    },
    ReleaseBranchGuard {
        branch: u8,
    },
    ReplayUnresolved,
    TimelineCheck {
        branch: u8,
    },
    MarkDeleting {
        branch: u8,
    },
    MarkDeleted {
        branch: u8,
    },
    RecreateBranch {
        branch: u8,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CommitScriptDurableFault {
    None,
    CleanWalFailure,
    UncertainWalFailure,
    WriterHalted,
    SegmentIdOverflow,
    ApplyFailureAfterWal,
    VisibleFailureAfterApply,
}

impl CommitRuntimeScript {
    pub(crate) fn decode(data: &[u8]) -> Self {
        let branches = [
            branch_id(0x10),
            branch_id(0x11),
            branch_id(0x12),
            branch_id(0x13),
            branch_id(0x14),
            branch_id(0x15),
            branch_id(0x16),
            branch_id(0x17),
        ];
        let operations =
            [CommitScriptOperation::TimelineCheck { branch: 0 }; COMMIT_RUNTIME_SCRIPT_MAX_OPS];
        let mut script = Self {
            branches,
            operations,
            operation_count: 0,
        };
        script.push_canonical_coverage();
        let mut cursor = 0;
        while cursor + 3 < data.len() && script.operation_count < COMMIT_RUNTIME_SCRIPT_MAX_OPS {
            let selector = data[cursor];
            let branch = bounded_branch(data[cursor + 1]);
            let key = data[cursor + 2];
            let value = data[cursor + 3];
            let operation = match selector % 17 {
                0 => CommitScriptOperation::CachePut { branch, key, value },
                1 => CommitScriptOperation::CacheDelete { branch, key },
                2 => CommitScriptOperation::DurablePut {
                    branch,
                    key,
                    value,
                    fault: CommitScriptDurableFault::None,
                },
                3 => CommitScriptOperation::DurablePut {
                    branch,
                    key,
                    value,
                    fault: durable_fault(value),
                },
                4 => CommitScriptOperation::DurableDelete {
                    branch,
                    key,
                    fault: durable_fault(value),
                },
                5 => CommitScriptOperation::ConflictPut { branch, key, value },
                6 => CommitScriptOperation::ReadOnlyDiagnostic { branch },
                7 => CommitScriptOperation::BeginQuiesce,
                8 => CommitScriptOperation::ReleaseQuiesce,
                9 => CommitScriptOperation::AcquireBranchGuard { branch },
                10 => CommitScriptOperation::ReleaseBranchGuard { branch },
                11 => CommitScriptOperation::ReplayUnresolved,
                12 => CommitScriptOperation::MarkDeleting { branch },
                13 => CommitScriptOperation::MarkDeleted { branch },
                14 => CommitScriptOperation::RecreateBranch { branch },
                _ => CommitScriptOperation::TimelineCheck { branch },
            };
            script.push(operation);
            cursor += 4;
        }
        script
    }

    pub(crate) fn branches(&self) -> &[BranchId] {
        &self.branches
    }

    pub(crate) fn operations(&self) -> &[CommitScriptOperation] {
        &self.operations[..self.operation_count]
    }

    fn push_canonical_coverage(&mut self) {
        self.push(CommitScriptOperation::CachePut {
            branch: 0,
            key: 1,
            value: 0x21,
        });
        self.push(CommitScriptOperation::CachePut {
            branch: 0,
            key: 2,
            value: 0x20,
        });
        self.push(CommitScriptOperation::ReadOnlyDiagnostic { branch: 0 });
        self.push(CommitScriptOperation::ConflictPut {
            branch: 0,
            key: 1,
            value: 0x22,
        });
        self.push(CommitScriptOperation::CacheDelete { branch: 0, key: 1 });
        self.push(CommitScriptOperation::DurablePut {
            branch: 1,
            key: 2,
            value: 0x31,
            fault: CommitScriptDurableFault::None,
        });
        self.push(CommitScriptOperation::DurablePut {
            branch: 1,
            key: 3,
            value: 0x32,
            fault: CommitScriptDurableFault::CleanWalFailure,
        });
        self.push(CommitScriptOperation::DurablePut {
            branch: 1,
            key: 4,
            value: 0x33,
            fault: CommitScriptDurableFault::UncertainWalFailure,
        });
        self.push(CommitScriptOperation::DurablePut {
            branch: 1,
            key: 5,
            value: 0x34,
            fault: CommitScriptDurableFault::ApplyFailureAfterWal,
        });
        self.push(CommitScriptOperation::ReplayUnresolved);
        self.push(CommitScriptOperation::DurablePut {
            branch: 2,
            key: 6,
            value: 0x35,
            fault: CommitScriptDurableFault::VisibleFailureAfterApply,
        });
        self.push(CommitScriptOperation::ReplayUnresolved);
        self.push(CommitScriptOperation::DurablePut {
            branch: 3,
            key: 7,
            value: 0x36,
            fault: CommitScriptDurableFault::WriterHalted,
        });
        self.push(CommitScriptOperation::DurablePut {
            branch: 3,
            key: 8,
            value: 0x37,
            fault: CommitScriptDurableFault::SegmentIdOverflow,
        });
        self.push(CommitScriptOperation::BeginQuiesce);
        self.push(CommitScriptOperation::AcquireBranchGuard { branch: 0 });
        self.push(CommitScriptOperation::ReleaseQuiesce);
        self.push(CommitScriptOperation::AcquireBranchGuard { branch: 0 });
        self.push(CommitScriptOperation::BeginQuiesce);
        self.push(CommitScriptOperation::ReleaseBranchGuard { branch: 0 });
        self.push(CommitScriptOperation::BeginQuiesce);
        self.push(CommitScriptOperation::ReleaseQuiesce);
        self.push(CommitScriptOperation::CachePut {
            branch: 4,
            key: 9,
            value: 0x41,
        });
        self.push(CommitScriptOperation::MarkDeleting { branch: 4 });
        self.push(CommitScriptOperation::CachePut {
            branch: 4,
            key: 9,
            value: 0x42,
        });
        self.push(CommitScriptOperation::MarkDeleted { branch: 4 });
        self.push(CommitScriptOperation::RecreateBranch { branch: 4 });
        self.push(CommitScriptOperation::CachePut {
            branch: 4,
            key: 9,
            value: 0x43,
        });
        self.push(CommitScriptOperation::RecreateBranch { branch: 4 });
        for branch in 0..4 {
            self.push(CommitScriptOperation::TimelineCheck { branch });
        }
    }

    fn push(&mut self, operation: CommitScriptOperation) {
        if self.operation_count < COMMIT_RUNTIME_SCRIPT_MAX_OPS {
            self.operations[self.operation_count] = operation;
            self.operation_count += 1;
        }
    }
}

fn branch_id(tag: u8) -> BranchId {
    let mut bytes = [0u8; BranchId::BYTE_LEN];
    bytes[0] = 0xc7;
    bytes[1] = tag;
    BranchId::from_bytes(bytes)
}

fn bounded_branch(byte: u8) -> u8 {
    byte % u8::try_from(COMMIT_RUNTIME_SCRIPT_MAX_BRANCHES).expect("script branch count fits in u8")
}

fn durable_fault(byte: u8) -> CommitScriptDurableFault {
    match byte % 7 {
        0 => CommitScriptDurableFault::None,
        1 => CommitScriptDurableFault::CleanWalFailure,
        2 => CommitScriptDurableFault::UncertainWalFailure,
        3 => CommitScriptDurableFault::WriterHalted,
        4 => CommitScriptDurableFault::SegmentIdOverflow,
        5 => CommitScriptDurableFault::ApplyFailureAfterWal,
        _ => CommitScriptDurableFault::VisibleFailureAfterApply,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CommitRuntimeScript, CommitScriptDurableFault, CommitScriptOperation,
        COMMIT_RUNTIME_SCRIPT_MAX_BRANCHES, COMMIT_RUNTIME_SCRIPT_MAX_OPS,
    };

    #[test]
    fn empty_input_decodes_to_deterministic_bounded_coverage_script() {
        let first = CommitRuntimeScript::decode(&[]);
        let second = CommitRuntimeScript::decode(&[]);

        assert_eq!(first, second);
        assert_eq!(first.branches().len(), COMMIT_RUNTIME_SCRIPT_MAX_BRANCHES);
        assert!(!first.operations().is_empty());
        assert!(first.operations().len() <= COMMIT_RUNTIME_SCRIPT_MAX_OPS);
    }

    #[test]
    fn arbitrary_input_is_clamped_to_the_operation_limit() {
        let script = CommitRuntimeScript::decode(&[0xff; 4096]);

        assert_eq!(script.operations().len(), COMMIT_RUNTIME_SCRIPT_MAX_OPS);
        for operation in script.operations() {
            match *operation {
                CommitScriptOperation::CachePut { branch, .. }
                | CommitScriptOperation::CacheDelete { branch, .. }
                | CommitScriptOperation::DurablePut { branch, .. }
                | CommitScriptOperation::DurableDelete { branch, .. }
                | CommitScriptOperation::ConflictPut { branch, .. }
                | CommitScriptOperation::ReadOnlyDiagnostic { branch }
                | CommitScriptOperation::AcquireBranchGuard { branch }
                | CommitScriptOperation::ReleaseBranchGuard { branch }
                | CommitScriptOperation::TimelineCheck { branch }
                | CommitScriptOperation::MarkDeleting { branch }
                | CommitScriptOperation::MarkDeleted { branch }
                | CommitScriptOperation::RecreateBranch { branch } => {
                    assert!(usize::from(branch) < COMMIT_RUNTIME_SCRIPT_MAX_BRANCHES);
                }
                CommitScriptOperation::BeginQuiesce
                | CommitScriptOperation::ReleaseQuiesce
                | CommitScriptOperation::ReplayUnresolved => {}
            }
        }
    }

    #[test]
    fn decoder_can_emit_every_generated_operation_variant() {
        let mut bytes = Vec::new();
        for selector in 0u8..17 {
            bytes.extend_from_slice(&[selector, selector, selector, selector]);
        }
        let script = CommitRuntimeScript::decode(&bytes);
        let operations = script.operations();

        assert!(operations
            .iter()
            .any(|operation| matches!(operation, CommitScriptOperation::CachePut { .. })));
        assert!(operations
            .iter()
            .any(|operation| matches!(operation, CommitScriptOperation::CacheDelete { .. })));
        assert!(operations.iter().any(|operation| matches!(
            operation,
            CommitScriptOperation::DurablePut {
                fault: CommitScriptDurableFault::None,
                ..
            }
        )));
        assert!(operations
            .iter()
            .any(|operation| matches!(operation, CommitScriptOperation::DurableDelete { .. })));
        assert!(operations
            .iter()
            .any(|operation| matches!(operation, CommitScriptOperation::ConflictPut { .. })));
        assert!(operations.iter().any(|operation| matches!(
            operation,
            CommitScriptOperation::ReadOnlyDiagnostic { .. }
        )));
        assert!(operations
            .iter()
            .any(|operation| matches!(operation, CommitScriptOperation::BeginQuiesce)));
        assert!(operations
            .iter()
            .any(|operation| matches!(operation, CommitScriptOperation::ReleaseQuiesce)));
        assert!(operations.iter().any(|operation| matches!(
            operation,
            CommitScriptOperation::AcquireBranchGuard { .. }
        )));
        assert!(operations.iter().any(|operation| matches!(
            operation,
            CommitScriptOperation::ReleaseBranchGuard { .. }
        )));
        assert!(operations
            .iter()
            .any(|operation| matches!(operation, CommitScriptOperation::ReplayUnresolved)));
        assert!(operations
            .iter()
            .any(|operation| matches!(operation, CommitScriptOperation::TimelineCheck { .. })));
        assert!(operations
            .iter()
            .any(|operation| matches!(operation, CommitScriptOperation::MarkDeleting { .. })));
        assert!(operations
            .iter()
            .any(|operation| matches!(operation, CommitScriptOperation::MarkDeleted { .. })));
        assert!(operations
            .iter()
            .any(|operation| matches!(operation, CommitScriptOperation::RecreateBranch { .. })));
    }
}
