//! Branch identity atoms.

use serde::{
    de::{self, SeqAccess, Visitor},
    Deserialize, Deserializer, Serialize, Serializer,
};
use std::{fmt, str::FromStr};

/// Opaque branch identity shared by storage and engine.
///
/// The value is stored as exactly sixteen bytes because storage encodes branch
/// identity into physical keys. Core does not define branch-name derivation,
/// random allocation, default branch policy, or branch lifecycle behavior.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct BranchId([u8; Self::BYTE_LEN]);

impl BranchId {
    /// Number of bytes in a branch identity.
    pub const BYTE_LEN: usize = 16;

    /// Creates a branch identity from its canonical byte representation.
    #[must_use]
    pub const fn from_bytes(bytes: [u8; Self::BYTE_LEN]) -> Self {
        Self(bytes)
    }

    /// Decodes a branch identity from bytes, rejecting incorrect lengths.
    ///
    /// This is the fallible form storage and engine should use when decoding
    /// bytes from external or durable inputs.
    pub fn try_from_slice(bytes: &[u8]) -> Result<Self, BranchIdError> {
        let array =
            <[u8; Self::BYTE_LEN]>::try_from(bytes).map_err(|_| BranchIdError::InvalidLength {
                expected: Self::BYTE_LEN,
                actual: bytes.len(),
            })?;
        Ok(Self(array))
    }

    /// Returns the canonical byte representation.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; Self::BYTE_LEN] {
        &self.0
    }

    /// Parses a branch identity from canonical UUID text.
    ///
    /// The parser accepts lowercase or uppercase hex digits. Display always
    /// emits lowercase canonical UUID text.
    pub fn parse_str(text: &str) -> Result<Self, BranchIdError> {
        text.parse()
    }
}

impl TryFrom<&[u8]> for BranchId {
    type Error = BranchIdError;

    fn try_from(bytes: &[u8]) -> Result<Self, Self::Error> {
        Self::try_from_slice(bytes)
    }
}

impl fmt::Display for BranchId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, byte) in self.0.iter().enumerate() {
            if matches!(index, 4 | 6 | 8 | 10) {
                f.write_str("-")?;
            }
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

impl FromStr for BranchId {
    type Err = BranchIdError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        const HYPHEN_INDICES: [usize; 4] = [8, 13, 18, 23];

        let text_bytes = text.as_bytes();
        if text_bytes.len() != 36 {
            return Err(BranchIdError::InvalidText);
        }

        let mut bytes = [0; Self::BYTE_LEN];
        let mut byte_index = 0;
        let mut high_nibble = true;

        for (index, byte) in text_bytes.iter().copied().enumerate() {
            if HYPHEN_INDICES.contains(&index) {
                if byte != b'-' {
                    return Err(BranchIdError::InvalidText);
                }
                continue;
            }

            let nibble = decode_hex_nibble(byte).ok_or(BranchIdError::InvalidText)?;
            if high_nibble {
                bytes[byte_index] = nibble << 4;
                high_nibble = false;
            } else {
                bytes[byte_index] |= nibble;
                byte_index += 1;
                high_nibble = true;
            }
        }

        if byte_index != Self::BYTE_LEN || !high_nibble {
            return Err(BranchIdError::InvalidText);
        }

        Ok(Self(bytes))
    }
}

impl Serialize for BranchId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        if serializer.is_human_readable() {
            serializer.collect_str(self)
        } else {
            serializer.serialize_bytes(&self.0)
        }
    }
}

impl<'de> Deserialize<'de> for BranchId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        if deserializer.is_human_readable() {
            deserializer.deserialize_str(BranchIdTextVisitor)
        } else {
            deserializer.deserialize_bytes(BranchIdBytesVisitor)
        }
    }
}

fn decode_hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

struct BranchIdTextVisitor;

impl Visitor<'_> for BranchIdTextVisitor {
    type Value = BranchId;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("canonical UUID branch identity text")
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        BranchId::parse_str(value).map_err(E::custom)
    }
}

struct BranchIdBytesVisitor;

impl<'de> Visitor<'de> for BranchIdBytesVisitor {
    type Value = BranchId;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "exactly {} raw branch identity bytes",
            BranchId::BYTE_LEN
        )
    }

    fn visit_bytes<E>(self, value: &[u8]) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        BranchId::try_from_slice(value).map_err(E::custom)
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let mut bytes = [0; BranchId::BYTE_LEN];
        for (index, slot) in bytes.iter_mut().enumerate() {
            *slot = seq
                .next_element()?
                .ok_or_else(|| de::Error::invalid_length(index, &self))?;
        }

        if seq.next_element::<u8>()?.is_some() {
            return Err(de::Error::invalid_length(BranchId::BYTE_LEN + 1, &self));
        }

        Ok(BranchId::from_bytes(bytes))
    }
}

/// Error returned when input cannot form a [`BranchId`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum BranchIdError {
    /// The byte input did not contain exactly sixteen bytes.
    InvalidLength {
        /// Required byte count.
        expected: usize,
        /// Observed byte count.
        actual: usize,
    },
    /// The text input was not canonical UUID text.
    InvalidText,
}

impl fmt::Display for BranchIdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLength { expected, actual } => {
                write!(
                    f,
                    "invalid branch id length: expected {expected} bytes, got {actual}"
                )
            }
            Self::InvalidText => f.write_str("invalid branch id text"),
        }
    }
}

impl std::error::Error for BranchIdError {}

#[cfg(test)]
mod tests {
    use super::{BranchId, BranchIdError};
    use std::{error::Error, str::FromStr};

    #[test]
    fn explicit_constructor_preserves_bytes() {
        let bytes = [7; BranchId::BYTE_LEN];
        let branch = BranchId::from_bytes(bytes);

        assert_eq!(*branch.as_bytes(), bytes);
    }

    #[test]
    fn equality_uses_all_bytes() {
        let first = BranchId::from_bytes([1; BranchId::BYTE_LEN]);
        let second = BranchId::from_bytes([2; BranchId::BYTE_LEN]);

        assert_ne!(first, second);
        assert_eq!(first, BranchId::from_bytes([1; BranchId::BYTE_LEN]));
    }

    #[test]
    fn try_from_slice_accepts_exact_length() {
        let bytes = [3; BranchId::BYTE_LEN];
        let branch = BranchId::try_from_slice(&bytes).expect("valid branch id bytes");

        assert_eq!(*branch.as_bytes(), bytes);
    }

    #[test]
    fn try_from_slice_rejects_short_input() {
        let err = BranchId::try_from_slice(&[0; BranchId::BYTE_LEN - 1])
            .expect_err("short input should fail");

        assert_eq!(
            err,
            BranchIdError::InvalidLength {
                expected: BranchId::BYTE_LEN,
                actual: BranchId::BYTE_LEN - 1,
            }
        );
    }

    #[test]
    fn try_from_slice_rejects_long_input() {
        let err = BranchId::try_from_slice(&[0; BranchId::BYTE_LEN + 1])
            .expect_err("long input should fail");

        assert_eq!(
            err,
            BranchIdError::InvalidLength {
                expected: BranchId::BYTE_LEN,
                actual: BranchId::BYTE_LEN + 1,
            }
        );
    }

    #[test]
    fn try_from_slice_trait_uses_same_validation() {
        let bytes = [4; BranchId::BYTE_LEN];
        let branch = BranchId::try_from(bytes.as_slice()).expect("valid branch id bytes");

        assert_eq!(*branch.as_bytes(), bytes);
    }

    #[test]
    fn branch_id_error_display_is_stable() {
        let err = BranchIdError::InvalidLength {
            expected: BranchId::BYTE_LEN,
            actual: 1,
        };

        assert_eq!(
            err.to_string(),
            "invalid branch id length: expected 16 bytes, got 1"
        );
        assert!(err.source().is_none());
    }

    #[test]
    fn display_uses_canonical_uuid_text() {
        let branch = BranchId::from_bytes([0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15]);

        assert_eq!(branch.to_string(), "00010203-0405-0607-0809-0a0b0c0d0e0f");
    }

    #[test]
    fn from_str_accepts_canonical_uuid_text() {
        let branch = BranchId::from_str("00010203-0405-0607-0809-0a0b0c0d0e0f")
            .expect("valid branch id text");

        assert_eq!(
            *branch.as_bytes(),
            [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15]
        );
    }

    #[test]
    fn from_str_accepts_uppercase_hex() {
        let branch = BranchId::from_str("00010203-0405-0607-0809-0A0B0C0D0E0F")
            .expect("valid uppercase branch id text");

        assert_eq!(branch.to_string(), "00010203-0405-0607-0809-0a0b0c0d0e0f");
    }

    #[test]
    fn parse_str_uses_from_str_contract() {
        let branch = BranchId::parse_str("00010203-0405-0607-0809-0a0b0c0d0e0f")
            .expect("valid branch id text");

        assert_eq!(
            branch,
            BranchId::from_str("00010203-0405-0607-0809-0a0b0c0d0e0f")
                .expect("valid branch id text")
        );
    }

    #[test]
    fn from_str_rejects_invalid_text_length() {
        let err = BranchId::from_str("00010203-0405-0607-0809-0a0b0c0d0e0")
            .expect_err("short text should fail");

        assert_eq!(err, BranchIdError::InvalidText);
        assert_eq!(err.to_string(), "invalid branch id text");
    }

    #[test]
    fn from_str_rejects_invalid_hyphen_position() {
        let err = BranchId::from_str("000102030405-0607-0809-0a0b0c0d0e0f")
            .expect_err("bad hyphen position should fail");

        assert_eq!(err, BranchIdError::InvalidText);
    }

    #[test]
    fn from_str_rejects_invalid_hex() {
        let err = BranchId::from_str("00010203-0405-0607-0809-0a0b0c0d0e0x")
            .expect_err("bad hex should fail");

        assert_eq!(err, BranchIdError::InvalidText);
    }

    #[test]
    fn serde_human_readable_uses_display_text() {
        let branch = BranchId::from_bytes([0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15]);
        let json = serde_json::to_string(&branch).expect("serialize branch id");

        assert_eq!(json, "\"00010203-0405-0607-0809-0a0b0c0d0e0f\"");
        assert_eq!(
            serde_json::from_str::<BranchId>(&json).expect("deserialize branch id"),
            branch
        );
    }

    #[test]
    fn serde_binary_round_trips_raw_bytes() {
        let branch = BranchId::from_bytes([0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15]);
        let bytes = bincode::serialize(&branch).expect("serialize branch id");

        assert_eq!(
            bincode::deserialize::<BranchId>(&bytes).expect("deserialize branch id"),
            branch
        );
    }

    #[test]
    fn representation_matches_raw_bytes_layout() {
        assert_eq!(
            std::mem::size_of::<BranchId>(),
            std::mem::size_of::<[u8; BranchId::BYTE_LEN]>()
        );
        assert_eq!(
            std::mem::align_of::<BranchId>(),
            std::mem::align_of::<[u8; BranchId::BYTE_LEN]>()
        );
    }
}
