use crate::service::QuarantineGate;
use strata_core::{BranchId, Timestamp};

// Fuzz bytes are decoded into bounded operations rather than arbitrary service
// inputs, which keeps generated histories small enough to shrink and replay.
#[derive(Clone, Debug)]
pub(super) enum QuarantineOperation {
    SeedSource {
        branch_id: BranchId,
        object_id: String,
        payload_len: usize,
        payload_seed: u8,
    },
    QuarantineObject {
        branch_id: BranchId,
        object_id: String,
        gate: QuarantineGate,
        fault: QuarantineFault,
        quarantined_at: Timestamp,
        allow_epoch_timestamp: bool,
    },
    PurgeBranch {
        branch_id: BranchId,
        object_id: String,
        gate: QuarantineGate,
        fail_delete: bool,
    },
    CorruptInventory {
        branch_id: BranchId,
    },
    InsertUnlistedObject {
        branch_id: BranchId,
        object_id: String,
        payload_len: usize,
        payload_seed: u8,
    },
    DeleteQuarantineObject {
        branch_id: BranchId,
        object_id: String,
    },
    ReconcileBranch {
        branch_id: BranchId,
    },
    LoadInventory {
        branch_id: BranchId,
    },
}

impl QuarantineOperation {
    pub(super) fn from_chunk(chunk: &[u8]) -> Self {
        let branch_id = branch_id(chunk[1]);
        let object_id = object_id(chunk[2]);
        let payload_len = payload_len(chunk[3]);
        let payload_seed = chunk[6];
        // Keep the eight operation families uniformly reachable from the first
        // byte. Histories often begin with harmless missing-source operations,
        // but uniform decoding makes reduced failing scripts easier to replay.
        match chunk[0] % 8 {
            0 => Self::SeedSource {
                branch_id,
                object_id,
                payload_len,
                payload_seed,
            },
            1 => Self::QuarantineObject {
                branch_id,
                object_id,
                gate: gate(chunk[4]),
                fault: QuarantineFault::from_byte(chunk[5]),
                quarantined_at: quarantined_at(chunk[7]),
                allow_epoch_timestamp: chunk[6] % 2 == 1,
            },
            2 => Self::PurgeBranch {
                branch_id,
                object_id,
                gate: gate(chunk[4]),
                fail_delete: chunk[5] % 3 == 1,
            },
            3 => Self::CorruptInventory { branch_id },
            4 => Self::InsertUnlistedObject {
                branch_id,
                object_id,
                payload_len,
                payload_seed,
            },
            5 => Self::DeleteQuarantineObject {
                branch_id,
                object_id,
            },
            6 => Self::ReconcileBranch { branch_id },
            _ => Self::LoadInventory { branch_id },
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum QuarantineFault {
    None,
    InventoryNoVisible,
    InventoryVisibilityUnknownInvisible,
    InventoryVisibilityUnknownVisible,
    CopyNoVisible,
    CopyVisibilityUnknownVisible,
    CopyDurabilityUnconfirmed,
    SourceDeleteFailure,
}

impl QuarantineFault {
    fn from_byte(byte: u8) -> Self {
        match byte % 8 {
            1 => Self::InventoryNoVisible,
            2 => Self::InventoryVisibilityUnknownInvisible,
            3 => Self::InventoryVisibilityUnknownVisible,
            4 => Self::CopyNoVisible,
            5 => Self::CopyVisibilityUnknownVisible,
            6 => Self::CopyDurabilityUnconfirmed,
            7 => Self::SourceDeleteFailure,
            _ => Self::None,
        }
    }
}

fn branch_id(byte: u8) -> BranchId {
    let branch_byte = byte % 8 + 1;
    BranchId::from_bytes([branch_byte; BranchId::BYTE_LEN])
}

fn object_id(byte: u8) -> String {
    format!("q{:04}", byte % 32 + 1)
}

fn payload_len(byte: u8) -> usize {
    // The distribution deliberately includes tiny, page-ish, and max-sized
    // payloads so the same target covers empty copies and larger object reads.
    match byte % 8 {
        0 => 0,
        1 => 1,
        2 => 2,
        3 => 31,
        4 => 255,
        5 => 1024,
        6 => 2048,
        _ => 4096,
    }
}

fn gate(byte: u8) -> QuarantineGate {
    match byte % 4 {
        1 => QuarantineGate::Referenced,
        2 => QuarantineGate::UnsafeRecovery,
        3 => QuarantineGate::ProofIncomplete,
        _ => QuarantineGate::Safe,
    }
}

fn quarantined_at(byte: u8) -> Timestamp {
    if byte % 2 == 0 {
        Timestamp::EPOCH
    } else {
        Timestamp::from_micros(1_700_000_000_000_000 + u64::from(byte))
    }
}
