//! Adversarial decode coverage for `BranchId`'s hand-written deserializer
//! (TCP3.1) — the only custom `Deserialize` in the crate, and before this
//! slice the only one whose rejection paths had zero tests. Malformed
//! durable or external input must be rejected with an error, never panic,
//! truncate, or zero-fill.

use serde::de::value::Error as ValueError;
use serde::de::{Deserializer, IntoDeserializer, SeqAccess, Visitor};
use serde::Deserialize;
use strata_core::BranchId;

// --- human-readable (text) path -----------------------------------------

#[test]
fn json_rejects_malformed_uuid_text() {
    for bad in [
        "\"\"",
        "\"not-a-uuid\"",
        "\"00010203-0405-0607-0809-0a0b0c0d0e0\"", // 35 chars
        "\"00010203-0405-0607-0809-0a0b0c0d0e0f0\"", // 37 chars
        "\"00010203+0405-0607-0809-0a0b0c0d0e0f\"", // wrong separator
        "\"0001020g-0405-0607-0809-0a0b0c0d0e0f\"", // non-hex
        "\"00010203-0405-0607-0809-0a0b0c0d0é0f\"", // multibyte char in hex slot
    ] {
        assert!(
            serde_json::from_str::<BranchId>(bad).is_err(),
            "malformed text must be rejected: {bad}"
        );
    }
}

#[test]
fn json_rejects_non_string_shapes() {
    for bad in ["42", "null", "[0,1,2]", "{}", "true"] {
        assert!(
            serde_json::from_str::<BranchId>(bad).is_err(),
            "non-string JSON must be rejected: {bad}"
        );
    }
}

#[test]
fn from_str_rejects_multibyte_and_boundary_lengths() {
    // 36 BYTES where a two-byte char occupies the leading hex slots: passes
    // the byte-length gate, must die on nibble decode without panicking.
    let leading = format!("é{}", "0".repeat(34));
    assert_eq!(leading.len(), 36);
    assert!(leading.parse::<BranchId>().is_err());

    // 36 BYTES, valid everywhere except a multibyte char in the tail slots.
    let tail = format!("{}é", &"00010203-0405-0607-0809-0a0b0c0d0e0f"[..34]);
    assert_eq!(tail.len(), 36);
    assert!(tail.parse::<BranchId>().is_err());

    // 36 CHARS but 37 bytes: the length gate must count bytes, not chars.
    let long = format!("{}é", &"00010203-0405-0607-0809-0a0b0c0d0e0f"[..35]);
    assert_eq!(long.chars().count(), 36);
    assert_eq!(long.len(), 37);
    assert!(long.parse::<BranchId>().is_err());
}

// --- binary (bytes/seq) path ---------------------------------------------

#[test]
fn bincode_rejects_wrong_byte_lengths() {
    // bincode drives visit_bytes: a 15- or 17-byte buffer must be refused.
    for len in [0usize, 1, 15, 17, 64] {
        let mut encoded = (len as u64).to_le_bytes().to_vec();
        encoded.extend(std::iter::repeat_n(0xab, len));
        assert!(
            bincode::deserialize::<BranchId>(&encoded).is_err(),
            "visit_bytes must reject {len} bytes"
        );
    }
}

/// Minimal non-human-readable deserializer that drives the visitor's
/// `visit_seq` path — the arm bincode never exercises (it always calls
/// `visit_bytes`), which therefore had zero coverage.
struct SeqBytesDeserializer(Vec<u8>);

impl<'de> Deserializer<'de> for SeqBytesDeserializer {
    type Error = ValueError;

    fn is_human_readable(&self) -> bool {
        false
    }

    fn deserialize_bytes<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        struct Access(std::vec::IntoIter<u8>);
        impl<'de> SeqAccess<'de> for Access {
            type Error = ValueError;
            fn next_element_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>, Self::Error>
            where
                T: serde::de::DeserializeSeed<'de>,
            {
                match self.0.next() {
                    Some(byte) => seed.deserialize(byte.into_deserializer()).map(Some),
                    None => Ok(None),
                }
            }
        }
        visitor.visit_seq(Access(self.0.into_iter()))
    }

    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_bytes(visitor)
    }

    serde::forward_to_deserialize_any! {
        bool i8 i16 i32 i64 u8 u16 u32 u64 f32 f64 char str string
        byte_buf option unit unit_struct newtype_struct seq tuple
        tuple_struct map struct enum identifier ignored_any
    }
}

#[test]
fn seq_path_accepts_exactly_sixteen_elements() {
    let branch = BranchId::deserialize(SeqBytesDeserializer((0u8..16).collect()))
        .expect("16-element sequence decodes");
    assert_eq!(branch.as_bytes()[0], 0);
    assert_eq!(branch.as_bytes()[15], 15);
}

#[test]
fn seq_path_rejects_too_few_and_too_many_elements() {
    assert!(
        BranchId::deserialize(SeqBytesDeserializer((0u8..15).collect())).is_err(),
        "15-element sequence must be rejected"
    );
    assert!(
        BranchId::deserialize(SeqBytesDeserializer((0u8..17).collect())).is_err(),
        "17-element sequence must be rejected"
    );
    assert!(
        BranchId::deserialize(SeqBytesDeserializer(Vec::new())).is_err(),
        "empty sequence must be rejected"
    );
}
