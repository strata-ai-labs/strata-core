//! BS4.2: conformance for the filter-frame codec — round-trip fidelity plus the reject surface that
//! keeps a corrupt filter from ever surviving to answer a false `DefinitelyAbsent`.

use super::{
    decode_filter_frame, encode_filter_frame, encode_table_block_frame, TableBlockFrame,
    TableBlockKind, TableCompression, TableFilterFrame,
};
use crate::format::FormatError;
use proptest::collection::vec;
use proptest::prelude::{any, Strategy};
use proptest::prop_assert_eq;
use proptest::test_runner::{Config, FileFailurePersistence, TestRunner};

/// Well-formed frames: `bit_count` is exactly `bits.len() * 8`, so the decode length check holds.
fn filter_frame_strategy() -> impl Strategy<Value = TableFilterFrame> {
    (1u8..=30u8, any::<u64>(), vec(any::<u8>(), 0..=2048)).prop_map(|(probes, key_count, bits)| {
        let bit_count = u64::try_from(bits.len() * 8).expect("bit count fits in u64");
        TableFilterFrame {
            probes,
            key_count,
            bit_count,
            bits,
        }
    })
}

#[test]
fn filter_frame_round_trips_generated_frames() {
    let mut runner = TestRunner::new(Config {
        failure_persistence: Some(Box::new(FileFailurePersistence::Direct(
            "proptest-regressions/table_filter_frame.txt",
        ))),
        ..Config::default()
    });
    runner
        .run(&filter_frame_strategy(), |frame| {
            let encoded = encode_filter_frame(&frame).expect("encode filter frame");
            let (decoded, consumed) = decode_filter_frame(&encoded).expect("decode filter frame");
            prop_assert_eq!(consumed, encoded.len());
            prop_assert_eq!(decoded, frame);
            Ok(())
        })
        .expect("filter frame round-trips");
}

/// Wrap a raw payload in a valid (correctly CRC'd) filter block frame — the vehicle for testing
/// payload-level rejects that survive the frame CRC.
fn framed(payload: Vec<u8>) -> Vec<u8> {
    let block = TableBlockFrame::new(
        TableBlockKind::Filter,
        TableCompression::Uncompressed,
        payload,
    )
    .expect("filter block frame");
    encode_table_block_frame(&block).expect("encode block frame")
}

fn payload(version: u32, probes: u8, key_count: u64, bit_count: u64, bits: &[u8]) -> Vec<u8> {
    let mut payload = Vec::new();
    payload.extend_from_slice(&version.to_le_bytes());
    payload.push(probes);
    payload.extend_from_slice(&key_count.to_le_bytes());
    payload.extend_from_slice(&bit_count.to_le_bytes());
    payload.extend_from_slice(bits);
    payload
}

#[test]
fn filter_frame_rejects_unknown_version() {
    let frame = framed(payload(2, 1, 0, 0, &[]));
    assert!(matches!(
        decode_filter_frame(&frame),
        Err(FormatError::FutureFormat {
            version: 2,
            max_supported: 1,
            ..
        })
    ));
}

#[test]
fn filter_frame_rejects_probe_count_over_ceiling() {
    let frame = framed(payload(1, 31, 4, 64, &[0xAB; 8]));
    assert!(matches!(
        decode_filter_frame(&frame),
        Err(FormatError::InvalidValue {
            field: "filter_probes"
        })
    ));
}

#[test]
fn filter_frame_rejects_bit_count_length_mismatch() {
    // bit_count claims 128 bits (16 bytes) but only 8 bytes of bits are present.
    let frame = framed(payload(1, 3, 4, 128, &[0u8; 8]));
    assert!(matches!(
        decode_filter_frame(&frame),
        Err(FormatError::InvalidLength {
            field: "filter_bits"
        })
    ));
}

#[test]
fn filter_frame_rejects_truncated_frame() {
    let frame = TableFilterFrame {
        probes: 3,
        key_count: 4,
        bit_count: 64,
        bits: vec![0xAB; 8],
    };
    let encoded = encode_filter_frame(&frame).expect("encode");
    assert!(decode_filter_frame(&encoded[..encoded.len() - 1]).is_err());
}

#[test]
fn encode_filter_frame_rejects_malformed_frames() {
    // BS4.2: encode must refuse to persist what decode would later reject — probes over the ceiling.
    assert!(matches!(
        encode_filter_frame(&TableFilterFrame {
            probes: 31,
            key_count: 4,
            bit_count: 64,
            bits: vec![0xAB; 8],
        }),
        Err(FormatError::InvalidValue {
            field: "filter_probes"
        })
    ));
    // bits length inconsistent with bit_count (128 bits => 16 bytes expected, only 8 supplied).
    assert!(matches!(
        encode_filter_frame(&TableFilterFrame {
            probes: 3,
            key_count: 4,
            bit_count: 128,
            bits: vec![0u8; 8],
        }),
        Err(FormatError::InvalidLength {
            field: "filter_bits"
        })
    ));
}

#[test]
fn filter_frame_round_trips_non_multiple_of_eight_bit_count_and_zero_probes() {
    // bit_count = 100 is not a multiple of 8 (ceil(100/8) = 13 bytes); probes = 0 is legal.
    let frame = TableFilterFrame {
        probes: 0,
        key_count: 7,
        bit_count: 100,
        bits: vec![0x5A; 13],
    };
    let encoded = encode_filter_frame(&frame).expect("encode");
    let (decoded, consumed) = decode_filter_frame(&encoded).expect("decode");
    assert_eq!(consumed, encoded.len());
    assert_eq!(decoded, frame);
}

#[test]
fn filter_frame_rejects_flipped_bit() {
    let frame = TableFilterFrame {
        probes: 3,
        key_count: 4,
        bit_count: 64,
        bits: vec![0xAB; 8],
    };
    let mut encoded = encode_filter_frame(&frame).expect("encode");
    let midpoint = encoded.len() / 2;
    encoded[midpoint] ^= 0x01;
    assert!(matches!(
        decode_filter_frame(&encoded),
        Err(FormatError::ChecksumMismatch { .. })
    ));
}
