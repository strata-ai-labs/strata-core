//! Stable engine row-key encoding.

use crate::data::kv::{KvKey, ProductSpace};
use crate::diagnostics::{EngineError, EngineResult};

const KEY_VERSION: u8 = 1;
const KV_DISCRIMINATOR: u8 = b'k';

pub(crate) fn encode_kv_key(space: &ProductSpace, key: &KvKey) -> Vec<u8> {
    encode_kv_key_bytes(space, key.as_bytes())
}

pub(crate) fn encode_kv_key_bytes(space: &ProductSpace, key_bytes: &[u8]) -> Vec<u8> {
    let space_bytes = space.as_str().as_bytes();
    let space_len = u16::try_from(space_bytes.len()).expect("validated product space length");
    let mut encoded = Vec::with_capacity(4 + space_bytes.len() + key_bytes.len());
    encoded.push(KEY_VERSION);
    encoded.push(KV_DISCRIMINATOR);
    encoded.extend_from_slice(&space_len.to_be_bytes());
    encoded.extend_from_slice(space_bytes);
    encoded.extend_from_slice(key_bytes);
    encoded
}

pub(crate) fn encode_kv_space_prefix(space: &ProductSpace) -> Vec<u8> {
    encode_kv_key_bytes(space, &[])
}

pub(crate) fn decode_kv_key(space: &ProductSpace, encoded: &[u8]) -> EngineResult<KvKey> {
    let corruption = || {
        EngineError::corruption(
            "data_loss.engine.kv_key",
            "stored KV row key is not valid for the selected product space",
        )
    };
    if encoded.len() < 4 {
        return Err(corruption());
    }
    if encoded[0] != KEY_VERSION || encoded[1] != KV_DISCRIMINATOR {
        return Err(corruption());
    }
    let space_len = usize::from(u16::from_be_bytes([encoded[2], encoded[3]]));
    let key_start = 4usize.checked_add(space_len).ok_or_else(corruption)?;
    if encoded.len() < key_start {
        return Err(corruption());
    }
    if &encoded[4..key_start] != space.as_str().as_bytes() {
        return Err(corruption());
    }
    let key_bytes = &encoded[key_start..];
    if key_bytes.is_empty() {
        return Err(corruption());
    }
    KvKey::new(key_bytes)
}

pub(crate) fn database_identity_key() -> Vec<u8> {
    b"\x01identity".to_vec()
}

pub(crate) fn storage_registry_key() -> Vec<u8> {
    b"\x01registry:storage-spaces".to_vec()
}

pub(crate) fn capability_registry_key() -> Vec<u8> {
    b"\x01registry:capabilities".to_vec()
}

pub(crate) fn branch_index_key() -> Vec<u8> {
    b"\x01branch:index".to_vec()
}

pub(crate) fn branch_default_key() -> Vec<u8> {
    b"\x01branch:default".to_vec()
}

pub(crate) fn branch_pending_index_key() -> Vec<u8> {
    b"\x01branch:pending-index".to_vec()
}

pub(crate) fn branch_catalog_key(name: &str) -> Vec<u8> {
    let name_len = u16::try_from(name.len()).expect("validated branch name length");
    let mut key = Vec::with_capacity(10 + name.len());
    key.extend_from_slice(b"\x01branch:");
    key.extend_from_slice(&name_len.to_be_bytes());
    key.extend_from_slice(name.as_bytes());
    key
}

pub(crate) fn branch_pending_key(name: &str) -> Vec<u8> {
    let name_len = u16::try_from(name.len()).expect("validated branch name length");
    let mut key = Vec::with_capacity(18 + name.len());
    key.extend_from_slice(b"\x01branch:pending:");
    key.extend_from_slice(&name_len.to_be_bytes());
    key.extend_from_slice(name.as_bytes());
    key
}

#[cfg(test)]
mod tests {
    use super::{
        branch_catalog_key, branch_default_key, branch_index_key, branch_pending_key,
        database_identity_key, decode_kv_key, encode_kv_key, encode_kv_space_prefix,
        storage_registry_key,
    };
    use crate::data::kv::{KvKey, ProductSpace};
    use crate::diagnostics::EngineErrorClass;

    #[test]
    fn kv_key_encoding_is_deterministic_for_ascii_keys() {
        let space = ProductSpace::new("users").expect("valid space");
        let key = KvKey::new(b"alice".as_slice()).expect("valid key");
        assert_eq!(
            encode_kv_key(&space, &key),
            b"\x01k\0\x05usersalice".to_vec()
        );
    }

    #[test]
    fn kv_key_encoding_is_deterministic_for_binary_keys() {
        let space = ProductSpace::new("default").expect("valid space");
        let key = KvKey::new([0, 1, 255]).expect("valid key");
        assert_eq!(
            encode_kv_key(&space, &key),
            vec![1, b'k', 0, 7, b'd', b'e', b'f', b'a', b'u', b'l', b't', 0, 1, 255]
        );
    }

    #[test]
    fn kv_key_encoding_preserves_order_inside_one_space() {
        let space = ProductSpace::new("default").expect("valid space");
        let a = encode_kv_key(&space, &KvKey::new(b"a".as_slice()).expect("valid key"));
        let b = encode_kv_key(&space, &KvKey::new(b"b".as_slice()).expect("valid key"));
        assert!(a < b);
    }

    #[test]
    fn kv_key_decoding_preserves_binary_user_key() {
        let space = ProductSpace::new("default").expect("valid space");
        let encoded = encode_kv_key(&space, &KvKey::new([0, 1, 255]).expect("valid key"));
        let decoded = decode_kv_key(&space, &encoded).expect("valid encoded key");
        assert_eq!(decoded.as_bytes(), &[0, 1, 255]);
    }

    #[test]
    fn kv_key_decoding_preserves_ascii_user_key() {
        let space = ProductSpace::new("users").expect("valid space");
        let encoded = encode_kv_key(&space, &KvKey::new(b"alice".as_slice()).expect("valid key"));
        let decoded = decode_kv_key(&space, &encoded).expect("valid encoded key");
        assert_eq!(decoded.as_bytes(), b"alice");
    }

    #[test]
    fn kv_key_space_prefix_is_not_a_valid_user_key() {
        let space = ProductSpace::new("default").expect("valid space");
        let error =
            decode_kv_key(&space, &encode_kv_space_prefix(&space)).expect_err("no user key");
        assert_eq!(error.class(), EngineErrorClass::Corruption);
        assert_eq!(error.code(), "data_loss.engine.kv_key");
    }

    #[test]
    fn kv_key_decoding_rejects_malformed_rows() {
        let space = ProductSpace::new("default").expect("valid space");
        for (case, encoded) in [
            ("truncated-header", vec![1, b'k', 0]),
            ("truncated-space-length", vec![1, b'k']),
            (
                "unknown-version",
                vec![
                    2, b'k', 0, 7, b'd', b'e', b'f', b'a', b'u', b'l', b't', b'a',
                ],
            ),
            (
                "unknown-discriminator",
                vec![
                    1, b'x', 0, 7, b'd', b'e', b'f', b'a', b'u', b'l', b't', b'a',
                ],
            ),
            (
                "truncated-space",
                vec![1, b'k', 0, 8, b'd', b'e', b'f', b'a', b'u', b'l', b't'],
            ),
            (
                "mismatched-space",
                vec![1, b'k', 0, 5, b'o', b't', b'h', b'e', b'r', b'a'],
            ),
        ] {
            let error = decode_kv_key(&space, &encoded).expect_err(case);
            assert_eq!(error.class(), EngineErrorClass::Corruption);
            assert_eq!(error.code(), "data_loss.engine.kv_key");
        }
    }

    #[test]
    fn kv_key_decoding_rejects_control_plane_rows() {
        let space = ProductSpace::new("default").expect("valid space");
        for encoded in [
            database_identity_key(),
            storage_registry_key(),
            branch_index_key(),
            branch_default_key(),
            branch_catalog_key("default"),
            branch_pending_key("feature"),
        ] {
            let error = decode_kv_key(&space, &encoded).expect_err("control row rejected");
            assert_eq!(error.class(), EngineErrorClass::Corruption);
            assert_eq!(error.code(), "data_loss.engine.kv_key");
        }
    }

    #[test]
    fn control_row_keys_are_deterministic() {
        assert_eq!(database_identity_key(), b"\x01identity".to_vec());
        assert_eq!(
            storage_registry_key(),
            b"\x01registry:storage-spaces".to_vec()
        );
        assert_eq!(branch_default_key(), b"\x01branch:default".to_vec());
        assert_eq!(
            branch_catalog_key("default"),
            b"\x01branch:\0\x07default".to_vec()
        );
        assert_eq!(
            branch_pending_key("feature"),
            b"\x01branch:pending:\0\x07feature".to_vec()
        );
    }
}
