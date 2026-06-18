//! Stable engine row-key encoding.

use crate::data::event::{EventSequence, EventType};
use crate::data::json::{JsonDocumentId, JsonIndexName};
use crate::data::kv::{KvKey, ProductSpace};
use crate::data::vector::{VectorCollectionName, VectorKey};
use crate::diagnostics::{EngineError, EngineResult};

const KEY_VERSION: u8 = 1;
const KV_DISCRIMINATOR: u8 = b'k';
const JSON_DISCRIMINATOR: u8 = b'j';
const JSON_INDEX_META_DISCRIMINATOR: u8 = b'm';
const JSON_INDEX_ENTRY_DISCRIMINATOR: u8 = b'i';
const VECTOR_COLLECTION_DISCRIMINATOR: u8 = b'c';
const VECTOR_ENTRY_DISCRIMINATOR: u8 = b'v';
const EVENT_RECORD_DISCRIMINATOR: u8 = b'e';
const EVENT_META_DISCRIMINATOR: u8 = b'E';
const EVENT_TYPE_INDEX_DISCRIMINATOR: u8 = b't';

pub(crate) fn encode_kv_key(space: &ProductSpace, key: &KvKey) -> Vec<u8> {
    encode_kv_key_bytes(space, key.as_bytes())
}

pub(crate) fn encode_kv_key_bytes(space: &ProductSpace, key_bytes: &[u8]) -> Vec<u8> {
    encode_user_key(KV_DISCRIMINATOR, space, key_bytes)
}

pub(crate) fn encode_json_key(space: &ProductSpace, id: &JsonDocumentId) -> Vec<u8> {
    encode_user_key(JSON_DISCRIMINATOR, space, id.as_str().as_bytes())
}

pub(crate) fn encode_json_space_prefix(space: &ProductSpace) -> Vec<u8> {
    encode_user_key(JSON_DISCRIMINATOR, space, &[])
}

pub(crate) fn decode_json_document_id(
    space: &ProductSpace,
    encoded: &[u8],
) -> EngineResult<JsonDocumentId> {
    let id_bytes = decode_user_key(
        space,
        encoded,
        JSON_DISCRIMINATOR,
        "data_loss.engine.json_key",
        "stored JSON row key is not valid for the selected product space",
    )?;
    let id = std::str::from_utf8(id_bytes).map_err(|_| {
        EngineError::corruption(
            "data_loss.engine.json_key",
            "stored JSON row key is not valid UTF-8",
        )
    })?;
    JsonDocumentId::new(id).map_err(|_| {
        EngineError::corruption(
            "data_loss.engine.json_key",
            "stored JSON row key is not a valid document id",
        )
    })
}

pub(crate) fn encode_json_index_meta_key(space: &ProductSpace, name: &JsonIndexName) -> Vec<u8> {
    encode_json_index_key(JSON_INDEX_META_DISCRIMINATOR, space, name, &[])
}

pub(crate) fn encode_json_index_meta_prefix(space: &ProductSpace) -> Vec<u8> {
    encode_user_key(JSON_INDEX_META_DISCRIMINATOR, space, &[])
}

pub(crate) fn decode_json_index_name(
    space: &ProductSpace,
    encoded: &[u8],
) -> EngineResult<JsonIndexName> {
    let bytes = decode_user_key(
        space,
        encoded,
        JSON_INDEX_META_DISCRIMINATOR,
        "data_loss.engine.json_index_key",
        "stored JSON index metadata key is not valid for the selected product space",
    )?;
    if bytes.len() < 2 {
        return Err(EngineError::corruption(
            "data_loss.engine.json_index_key",
            "stored JSON index metadata key is truncated",
        ));
    }
    let name_len = usize::from(u16::from_be_bytes([bytes[0], bytes[1]]));
    let name_end = 2usize.checked_add(name_len).ok_or_else(|| {
        EngineError::corruption(
            "data_loss.engine.json_index_key",
            "stored JSON index metadata key length overflowed",
        )
    })?;
    if bytes.len() != name_end {
        return Err(EngineError::corruption(
            "data_loss.engine.json_index_key",
            "stored JSON index metadata key has trailing bytes",
        ));
    }
    let name = std::str::from_utf8(&bytes[2..name_end]).map_err(|_| {
        EngineError::corruption(
            "data_loss.engine.json_index_key",
            "stored JSON index metadata key is not valid UTF-8",
        )
    })?;
    JsonIndexName::new(name).map_err(|_| {
        EngineError::corruption(
            "data_loss.engine.json_index_key",
            "stored JSON index metadata key is not a valid index name",
        )
    })
}

pub(crate) fn encode_json_index_entry_key(
    space: &ProductSpace,
    name: &JsonIndexName,
    encoded_value: &[u8],
    id: &JsonDocumentId,
) -> Vec<u8> {
    let mut suffix = Vec::with_capacity(encoded_value.len() + 1 + id.as_str().len());
    suffix.extend_from_slice(encoded_value);
    suffix.push(0xff);
    suffix.extend_from_slice(id.as_str().as_bytes());
    encode_json_index_key(JSON_INDEX_ENTRY_DISCRIMINATOR, space, name, &suffix)
}

pub(crate) fn encode_json_index_entry_prefix(
    space: &ProductSpace,
    name: &JsonIndexName,
) -> Vec<u8> {
    encode_json_index_key(JSON_INDEX_ENTRY_DISCRIMINATOR, space, name, &[])
}

pub(crate) fn encode_vector_collection_key(
    space: &ProductSpace,
    collection: &VectorCollectionName,
) -> Vec<u8> {
    let mut suffix = Vec::new();
    encode_length_prefixed_text(&mut suffix, collection.as_str());
    encode_user_key(VECTOR_COLLECTION_DISCRIMINATOR, space, &suffix)
}

pub(crate) fn encode_vector_collection_prefix(space: &ProductSpace) -> Vec<u8> {
    encode_user_key(VECTOR_COLLECTION_DISCRIMINATOR, space, &[])
}

pub(crate) fn decode_vector_collection_name(
    space: &ProductSpace,
    encoded: &[u8],
) -> EngineResult<VectorCollectionName> {
    let bytes = decode_user_key(
        space,
        encoded,
        VECTOR_COLLECTION_DISCRIMINATOR,
        "data_loss.engine.vector_collection_key",
        "stored vector collection row key is not valid for the selected product space",
    )?;
    let (collection, rest) = decode_length_prefixed_text(
        bytes,
        "data_loss.engine.vector_collection_key",
        "stored vector collection row key is truncated",
    )?;
    if !rest.is_empty() {
        return Err(EngineError::corruption(
            "data_loss.engine.vector_collection_key",
            "stored vector collection row key has trailing bytes",
        ));
    }
    VectorCollectionName::new(collection).map_err(|_| {
        EngineError::corruption(
            "data_loss.engine.vector_collection_key",
            "stored vector collection row key contains an invalid collection name",
        )
    })
}

pub(crate) fn encode_vector_key(
    space: &ProductSpace,
    collection: &VectorCollectionName,
    key: &VectorKey,
) -> Vec<u8> {
    let mut suffix = Vec::new();
    encode_length_prefixed_text(&mut suffix, collection.as_str());
    suffix.extend_from_slice(key.as_str().as_bytes());
    encode_user_key(VECTOR_ENTRY_DISCRIMINATOR, space, &suffix)
}

pub(crate) fn encode_vector_collection_entry_prefix(
    space: &ProductSpace,
    collection: &VectorCollectionName,
) -> Vec<u8> {
    let mut suffix = Vec::new();
    encode_length_prefixed_text(&mut suffix, collection.as_str());
    encode_user_key(VECTOR_ENTRY_DISCRIMINATOR, space, &suffix)
}

pub(crate) fn decode_vector_key(
    space: &ProductSpace,
    encoded: &[u8],
) -> EngineResult<(VectorCollectionName, VectorKey)> {
    let bytes = decode_user_key(
        space,
        encoded,
        VECTOR_ENTRY_DISCRIMINATOR,
        "data_loss.engine.vector_key",
        "stored vector row key is not valid for the selected product space",
    )?;
    let (collection, rest) = decode_length_prefixed_text(
        bytes,
        "data_loss.engine.vector_key",
        "stored vector row key is missing a collection name",
    )?;
    let key = std::str::from_utf8(rest).map_err(|_| {
        EngineError::corruption(
            "data_loss.engine.vector_key",
            "stored vector row key contains a non-UTF-8 vector key",
        )
    })?;
    let collection = VectorCollectionName::new(collection).map_err(|_| {
        EngineError::corruption(
            "data_loss.engine.vector_key",
            "stored vector row key contains an invalid collection name",
        )
    })?;
    let key = VectorKey::new(key).map_err(|_| {
        EngineError::corruption(
            "data_loss.engine.vector_key",
            "stored vector row key contains an invalid vector key",
        )
    })?;
    Ok((collection, key))
}

pub(crate) fn encode_event_key(space: &ProductSpace, sequence: EventSequence) -> Vec<u8> {
    encode_user_key(
        EVENT_RECORD_DISCRIMINATOR,
        space,
        &sequence.as_u64().to_be_bytes(),
    )
}

pub(crate) fn decode_event_key_sequence(
    space: &ProductSpace,
    encoded: &[u8],
) -> EngineResult<EventSequence> {
    let bytes = decode_user_key(
        space,
        encoded,
        EVENT_RECORD_DISCRIMINATOR,
        "data_loss.engine.event_key",
        "stored event row key is not valid for the selected product space",
    )?;
    decode_event_sequence_suffix(bytes, "data_loss.engine.event_key")
}

pub(crate) fn encode_event_space_prefix(space: &ProductSpace) -> Vec<u8> {
    encode_user_key(EVENT_RECORD_DISCRIMINATOR, space, &[])
}

pub(crate) fn encode_event_meta_key(space: &ProductSpace) -> Vec<u8> {
    encode_user_key(EVENT_META_DISCRIMINATOR, space, b"meta")
}

pub(crate) fn encode_event_type_index_key(
    space: &ProductSpace,
    event_type: &EventType,
    sequence: EventSequence,
) -> Vec<u8> {
    let mut suffix = Vec::new();
    encode_length_prefixed_text(&mut suffix, event_type.as_str());
    suffix.extend_from_slice(&sequence.as_u64().to_be_bytes());
    encode_user_key(EVENT_TYPE_INDEX_DISCRIMINATOR, space, &suffix)
}

#[cfg(test)]
fn encode_event_type_index_prefix(space: &ProductSpace, event_type: &EventType) -> Vec<u8> {
    let mut suffix = Vec::new();
    encode_length_prefixed_text(&mut suffix, event_type.as_str());
    encode_user_key(EVENT_TYPE_INDEX_DISCRIMINATOR, space, &suffix)
}

#[cfg(test)]
fn decode_event_sequence(space: &ProductSpace, encoded: &[u8]) -> EngineResult<EventSequence> {
    decode_event_type_index_key(space, encoded).map(|(_, sequence)| sequence)
}

#[cfg(test)]
fn decode_event_type_index_key(
    space: &ProductSpace,
    encoded: &[u8],
) -> EngineResult<(EventType, EventSequence)> {
    let bytes = decode_user_key(
        space,
        encoded,
        EVENT_TYPE_INDEX_DISCRIMINATOR,
        "data_loss.engine.event_index_key",
        "stored event type index row key is not valid for the selected product space",
    )?;
    let (event_type, rest) = decode_length_prefixed_text(
        bytes,
        "data_loss.engine.event_index_key",
        "stored event type index row key is missing an event type",
    )?;
    let event_type = EventType::new(event_type).map_err(|_| {
        EngineError::corruption(
            "data_loss.engine.event_index_key",
            "stored event type index row key contains an invalid event type",
        )
    })?;
    if rest.len() != 8 {
        return Err(EngineError::corruption(
            "data_loss.engine.event_index_key",
            "stored event type index row key has an invalid sequence length",
        ));
    }
    let sequence = decode_event_sequence_suffix(rest, "data_loss.engine.event_index_key")?;
    Ok((event_type, sequence))
}

fn decode_event_sequence_suffix(bytes: &[u8], code: &'static str) -> EngineResult<EventSequence> {
    if bytes.len() != 8 {
        return Err(EngineError::corruption(
            code,
            "stored event row key has an invalid sequence length",
        ));
    }
    let sequence = u64::from_be_bytes(bytes.try_into().map_err(|_| {
        EngineError::corruption(code, "stored event row key sequence is malformed")
    })?);
    Ok(EventSequence::new(sequence))
}

fn encode_json_index_key(
    discriminator: u8,
    space: &ProductSpace,
    name: &JsonIndexName,
    suffix: &[u8],
) -> Vec<u8> {
    let name_bytes = name.as_str().as_bytes();
    let name_len = u16::try_from(name_bytes.len()).expect("validated JSON index name length");
    let mut key = encode_user_key(discriminator, space, &[]);
    key.extend_from_slice(&name_len.to_be_bytes());
    key.extend_from_slice(name_bytes);
    key.extend_from_slice(suffix);
    key
}

fn encode_length_prefixed_text(output: &mut Vec<u8>, value: &str) {
    let value_bytes = value.as_bytes();
    let value_len = u16::try_from(value_bytes.len()).expect("validated vector key field length");
    output.extend_from_slice(&value_len.to_be_bytes());
    output.extend_from_slice(value_bytes);
}

fn decode_length_prefixed_text<'a>(
    bytes: &'a [u8],
    code: &'static str,
    message: &'static str,
) -> EngineResult<(&'a str, &'a [u8])> {
    if bytes.len() < 2 {
        return Err(EngineError::corruption(code, message));
    }
    let value_len = usize::from(u16::from_be_bytes([bytes[0], bytes[1]]));
    let value_start = 2usize;
    let value_end = value_start
        .checked_add(value_len)
        .ok_or_else(|| EngineError::corruption(code, "stored vector row key length overflowed"))?;
    if bytes.len() < value_end {
        return Err(EngineError::corruption(code, message));
    }
    let value = std::str::from_utf8(&bytes[value_start..value_end]).map_err(|_| {
        EngineError::corruption(code, "stored vector row key field is not valid UTF-8")
    })?;
    Ok((value, &bytes[value_end..]))
}

fn encode_user_key(discriminator: u8, space: &ProductSpace, key_bytes: &[u8]) -> Vec<u8> {
    let space_bytes = space.as_str().as_bytes();
    let space_len = u16::try_from(space_bytes.len()).expect("validated product space length");
    let mut encoded = Vec::with_capacity(4 + space_bytes.len() + key_bytes.len());
    encoded.push(KEY_VERSION);
    encoded.push(discriminator);
    encoded.extend_from_slice(&space_len.to_be_bytes());
    encoded.extend_from_slice(space_bytes);
    encoded.extend_from_slice(key_bytes);
    encoded
}

pub(crate) fn encode_kv_space_prefix(space: &ProductSpace) -> Vec<u8> {
    encode_kv_key_bytes(space, &[])
}

pub(crate) fn decode_kv_key(space: &ProductSpace, encoded: &[u8]) -> EngineResult<KvKey> {
    let key_bytes = decode_user_key(
        space,
        encoded,
        KV_DISCRIMINATOR,
        "data_loss.engine.kv_key",
        "stored KV row key is not valid for the selected product space",
    )?;
    KvKey::new(key_bytes)
}

fn decode_user_key<'a>(
    space: &ProductSpace,
    encoded: &'a [u8],
    discriminator: u8,
    code: &'static str,
    message: &'static str,
) -> EngineResult<&'a [u8]> {
    let corruption = || EngineError::corruption(code, message);
    if encoded.len() < 4 {
        return Err(corruption());
    }
    if encoded[0] != KEY_VERSION || encoded[1] != discriminator {
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
    Ok(key_bytes)
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
        database_identity_key, decode_event_key_sequence, decode_event_sequence,
        decode_event_type_index_key, decode_json_document_id, decode_json_index_name,
        decode_kv_key, decode_vector_collection_name, decode_vector_key, encode_event_key,
        encode_event_meta_key, encode_event_space_prefix, encode_event_type_index_key,
        encode_event_type_index_prefix, encode_json_index_entry_key,
        encode_json_index_entry_prefix, encode_json_index_meta_key, encode_json_key,
        encode_json_space_prefix, encode_kv_key, encode_kv_space_prefix,
        encode_vector_collection_entry_prefix, encode_vector_collection_key,
        encode_vector_collection_prefix, encode_vector_key, storage_registry_key,
    };
    use crate::data::event::{EventSequence, EventType};
    use crate::data::json::{JsonDocumentId, JsonIndexName};
    use crate::data::kv::{KvKey, ProductSpace};
    use crate::data::vector::{VectorCollectionName, VectorKey};
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
    fn json_key_encoding_is_deterministic_and_ordered() {
        let space = ProductSpace::new("users").expect("valid space");
        let alice = JsonDocumentId::new("alice").expect("valid document id");
        let bob = JsonDocumentId::new("bob").expect("valid document id");

        assert_eq!(
            encode_json_key(&space, &alice),
            b"\x01j\0\x05usersalice".to_vec()
        );
        assert!(encode_json_key(&space, &alice) < encode_json_key(&space, &bob));
    }

    #[test]
    fn json_key_decoding_preserves_utf8_document_ids() {
        let space = ProductSpace::new("default").expect("valid space");
        let id = JsonDocumentId::new("café").expect("valid document id");
        let decoded =
            decode_json_document_id(&space, &encode_json_key(&space, &id)).expect("decode");
        assert_eq!(decoded, id);
    }

    #[test]
    fn json_key_space_prefix_is_not_a_valid_document_key() {
        let space = ProductSpace::new("default").expect("valid space");
        let error = decode_json_document_id(&space, &encode_json_space_prefix(&space))
            .expect_err("space prefix rejected");
        assert_eq!(error.class(), EngineErrorClass::Corruption);
        assert_eq!(error.code(), "data_loss.engine.json_key");
    }

    #[test]
    fn json_key_decoding_rejects_malformed_rows() {
        let space = ProductSpace::new("default").expect("valid space");
        for (case, encoded) in [
            ("truncated-header", vec![1, b'j', 0]),
            (
                "unknown-version",
                vec![
                    2, b'j', 0, 7, b'd', b'e', b'f', b'a', b'u', b'l', b't', b'a',
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
                vec![1, b'j', 0, 8, b'd', b'e', b'f', b'a', b'u', b'l', b't'],
            ),
            (
                "mismatched-space",
                vec![1, b'j', 0, 5, b'o', b't', b'h', b'e', b'r', b'a'],
            ),
            (
                "invalid-utf8-id",
                vec![
                    1, b'j', 0, 7, b'd', b'e', b'f', b'a', b'u', b'l', b't', 0xff,
                ],
            ),
        ] {
            let error = decode_json_document_id(&space, &encoded).expect_err(case);
            assert_eq!(error.class(), EngineErrorClass::Corruption);
            assert_eq!(error.code(), "data_loss.engine.json_key");
        }
    }

    #[test]
    fn json_key_decoding_rejects_kv_and_control_plane_rows() {
        let space = ProductSpace::new("default").expect("valid space");
        let kv = encode_kv_key(&space, &KvKey::new(b"alice".as_slice()).expect("valid key"));
        for encoded in [
            kv,
            database_identity_key(),
            storage_registry_key(),
            branch_index_key(),
            branch_default_key(),
            branch_catalog_key("default"),
            branch_pending_key("feature"),
        ] {
            let error =
                decode_json_document_id(&space, &encoded).expect_err("non-JSON row rejected");
            assert_eq!(error.class(), EngineErrorClass::Corruption);
            assert_eq!(error.code(), "data_loss.engine.json_key");
        }
    }

    #[test]
    fn json_index_keys_are_deterministic_and_decodable() {
        let space = ProductSpace::new("users").expect("valid space");
        let name = JsonIndexName::new("by_name").expect("valid index name");
        let id = JsonDocumentId::new("alice").expect("valid document id");

        assert_eq!(
            encode_json_index_meta_key(&space, &name),
            b"\x01m\0\x05users\0\x07by_name".to_vec()
        );
        assert_eq!(
            decode_json_index_name(&space, &encode_json_index_meta_key(&space, &name))
                .expect("decode"),
            name
        );
        assert_eq!(
            encode_json_index_entry_prefix(&space, &name),
            b"\x01i\0\x05users\0\x07by_name".to_vec()
        );
        assert_eq!(
            encode_json_index_entry_key(&space, &name, b"alice", &id),
            b"\x01i\0\x05users\0\x07by_namealice\xffalice".to_vec()
        );
    }

    #[test]
    fn json_index_name_decoding_rejects_malformed_metadata_keys() {
        let space = ProductSpace::new("default").expect("valid space");
        for (case, encoded) in [
            (
                "truncated-name-len",
                vec![1, b'm', 0, 7, b'd', b'e', b'f', b'a', b'u', b'l', b't', 0],
            ),
            (
                "truncated-name",
                vec![
                    1, b'm', 0, 7, b'd', b'e', b'f', b'a', b'u', b'l', b't', 0, 4, b'a',
                ],
            ),
            (
                "trailing-bytes",
                vec![
                    1, b'm', 0, 7, b'd', b'e', b'f', b'a', b'u', b'l', b't', 0, 1, b'a', b'x',
                ],
            ),
        ] {
            let error = decode_json_index_name(&space, &encoded).expect_err(case);
            assert_eq!(error.class(), EngineErrorClass::Corruption);
            assert_eq!(error.code(), "data_loss.engine.json_index_key");
        }
    }

    #[test]
    fn event_keys_are_deterministic_and_ordered() {
        let space = ProductSpace::new("default").expect("valid space");
        let event_type = EventType::new("order.created").expect("valid event type");
        assert_eq!(
            encode_event_key(&space, EventSequence::new(7)),
            b"\x01e\0\x07default\0\0\0\0\0\0\0\x07".to_vec()
        );
        assert_eq!(
            encode_event_meta_key(&space),
            b"\x01E\0\x07defaultmeta".to_vec()
        );
        assert_eq!(
            encode_event_type_index_prefix(&space, &event_type),
            b"\x01t\0\x07default\0\rorder.created".to_vec()
        );
        let index = encode_event_type_index_key(&space, &event_type, EventSequence::new(7));
        assert_eq!(
            decode_event_type_index_key(&space, &index).expect("decode index key"),
            (event_type, EventSequence::new(7))
        );
        assert_eq!(
            decode_event_key_sequence(&space, &encode_event_key(&space, EventSequence::new(7)))
                .expect("decode event key"),
            EventSequence::new(7)
        );
        assert!(
            encode_event_key(&space, EventSequence::new(7))
                < encode_event_key(&space, EventSequence::new(8))
        );
        assert!(
            encode_event_space_prefix(&space) < encode_event_key(&space, EventSequence::new(0))
        );
    }

    #[test]
    fn event_key_decoding_preserves_separators_and_boundary_type_names() {
        let space = ProductSpace::new("default").expect("valid space");
        for event_type in [
            EventType::new("order/created:tenant.one").expect("valid event type"),
            EventType::new("e".repeat(256)).expect("valid event type"),
        ] {
            let encoded = encode_event_type_index_key(&space, &event_type, EventSequence::new(42));
            assert_eq!(
                decode_event_type_index_key(&space, &encoded).expect("decode index key"),
                (event_type, EventSequence::new(42))
            );
        }
    }

    #[test]
    fn event_key_decoding_rejects_malformed_rows() {
        let space = ProductSpace::new("default").expect("valid space");
        for (case, encoded, code, decode_type_index) in [
            (
                "event-truncated-header",
                vec![1, b'e', 0],
                "data_loss.engine.event_key",
                false,
            ),
            (
                "event-unknown-version",
                vec![
                    2, b'e', 0, 7, b'd', b'e', b'f', b'a', b'u', b'l', b't', 0, 0, 0, 0, 0, 0, 0, 0,
                ],
                "data_loss.engine.event_key",
                false,
            ),
            (
                "event-unknown-discriminator",
                vec![
                    1, b'x', 0, 7, b'd', b'e', b'f', b'a', b'u', b'l', b't', 0, 0, 0, 0, 0, 0, 0, 0,
                ],
                "data_loss.engine.event_key",
                false,
            ),
            (
                "event-truncated-sequence",
                vec![1, b'e', 0, 7, b'd', b'e', b'f', b'a', b'u', b'l', b't', 0],
                "data_loss.engine.event_key",
                false,
            ),
            (
                "index-truncated-type-length",
                vec![1, b't', 0, 7, b'd', b'e', b'f', b'a', b'u', b'l', b't', 0],
                "data_loss.engine.event_index_key",
                true,
            ),
            (
                "index-truncated-type",
                vec![
                    1, b't', 0, 7, b'd', b'e', b'f', b'a', b'u', b'l', b't', 0, 4, b'a',
                ],
                "data_loss.engine.event_index_key",
                true,
            ),
            (
                "index-invalid-utf8",
                vec![
                    1, b't', 0, 7, b'd', b'e', b'f', b'a', b'u', b'l', b't', 0, 1, 0xff, 0, 0, 0,
                    0, 0, 0, 0, 1,
                ],
                "data_loss.engine.event_index_key",
                true,
            ),
            (
                "index-invalid-event-type",
                vec![
                    1, b't', 0, 7, b'd', b'e', b'f', b'a', b'u', b'l', b't', 0, 1, b' ', 0, 0, 0,
                    0, 0, 0, 0, 1,
                ],
                "data_loss.engine.event_index_key",
                true,
            ),
            (
                "index-truncated-sequence",
                vec![
                    1, b't', 0, 7, b'd', b'e', b'f', b'a', b'u', b'l', b't', 0, 1, b'a', 0,
                ],
                "data_loss.engine.event_index_key",
                true,
            ),
        ] {
            let error = if decode_type_index {
                decode_event_sequence(&space, &encoded).expect_err(case)
            } else {
                decode_event_key_sequence(&space, &encoded).expect_err(case)
            };
            assert_eq!(error.class(), EngineErrorClass::Corruption);
            assert_eq!(error.code(), code);
        }
    }

    #[test]
    fn event_key_decoding_rejects_other_row_families() {
        let space = ProductSpace::new("default").expect("valid space");
        let kv = encode_kv_key(&space, &KvKey::new(b"event".as_slice()).expect("valid key"));
        let json = encode_json_key(
            &space,
            &JsonDocumentId::new("event").expect("valid document id"),
        );
        let vector = encode_vector_key(
            &space,
            &VectorCollectionName::new("docs").expect("valid collection"),
            &VectorKey::new("event").expect("valid key"),
        );

        for encoded in [kv, json, vector] {
            let error =
                decode_event_key_sequence(&space, &encoded).expect_err("non-event row rejected");
            assert_eq!(error.class(), EngineErrorClass::Corruption);
            assert_eq!(error.code(), "data_loss.engine.event_key");
        }
    }

    #[test]
    fn vector_collection_keys_are_deterministic_and_decodable() {
        let space = ProductSpace::new("users").expect("valid space");
        let collection = VectorCollectionName::new("docs").expect("valid collection");

        assert_eq!(
            encode_vector_collection_key(&space, &collection),
            b"\x01c\0\x05users\0\x04docs".to_vec()
        );
        assert_eq!(
            decode_vector_collection_name(
                &space,
                &encode_vector_collection_key(&space, &collection)
            )
            .expect("decode"),
            collection
        );
        assert_eq!(
            encode_vector_collection_prefix(&space),
            b"\x01c\0\x05users".to_vec()
        );
    }

    #[test]
    fn vector_entry_keys_preserve_public_key_order() {
        let space = ProductSpace::new("users").expect("valid space");
        let collection = VectorCollectionName::new("docs").expect("valid collection");
        let keys = ["a", "aa", "ab", "b", "ba"];
        let mut encoded = keys
            .iter()
            .map(|key| {
                encode_vector_key(
                    &space,
                    &collection,
                    &VectorKey::new(*key).expect("valid key"),
                )
            })
            .collect::<Vec<_>>();
        encoded.sort();
        let decoded = encoded
            .iter()
            .map(|key| {
                decode_vector_key(&space, key)
                    .expect("decode")
                    .1
                    .as_str()
                    .to_owned()
            })
            .collect::<Vec<_>>();
        assert_eq!(decoded, keys);
        assert_eq!(
            encode_vector_collection_entry_prefix(&space, &collection),
            b"\x01v\0\x05users\0\x04docs".to_vec()
        );
    }

    #[test]
    fn vector_key_decoding_preserves_separators_and_empty_keys() {
        let space = ProductSpace::new("default").expect("valid space");
        let collection = VectorCollectionName::new("docs").expect("valid collection");
        for key in ["", "doc/1", "nested/path/key"] {
            let key = VectorKey::new(key).expect("valid key");
            let (decoded_collection, decoded_key) =
                decode_vector_key(&space, &encode_vector_key(&space, &collection, &key))
                    .expect("decode");
            assert_eq!(decoded_collection, collection);
            assert_eq!(decoded_key, key);
        }
    }

    #[test]
    fn vector_key_decoding_rejects_malformed_rows() {
        let space = ProductSpace::new("default").expect("valid space");
        for (case, encoded, code) in [
            (
                "collection-truncated-header",
                vec![1, b'c', 0],
                "data_loss.engine.vector_collection_key",
            ),
            (
                "collection-truncated-name-length",
                vec![1, b'c', 0, 7, b'd', b'e', b'f', b'a', b'u', b'l', b't', 0],
                "data_loss.engine.vector_collection_key",
            ),
            (
                "collection-truncated-name",
                vec![
                    1, b'c', 0, 7, b'd', b'e', b'f', b'a', b'u', b'l', b't', 0, 4, b'd',
                ],
                "data_loss.engine.vector_collection_key",
            ),
            (
                "collection-trailing-bytes",
                vec![
                    1, b'c', 0, 7, b'd', b'e', b'f', b'a', b'u', b'l', b't', 0, 1, b'd', b'x',
                ],
                "data_loss.engine.vector_collection_key",
            ),
            (
                "entry-truncated-collection",
                vec![
                    1, b'v', 0, 7, b'd', b'e', b'f', b'a', b'u', b'l', b't', 0, 4, b'd',
                ],
                "data_loss.engine.vector_key",
            ),
            (
                "entry-invalid-key-utf8",
                vec![
                    1, b'v', 0, 7, b'd', b'e', b'f', b'a', b'u', b'l', b't', 0, 4, b'd', b'o',
                    b'c', b's', 0xff,
                ],
                "data_loss.engine.vector_key",
            ),
            (
                "unknown-version",
                vec![
                    2, b'v', 0, 7, b'd', b'e', b'f', b'a', b'u', b'l', b't', 0, 4, b'd', b'o',
                    b'c', b's', b'a',
                ],
                "data_loss.engine.vector_key",
            ),
        ] {
            let error = if code == "data_loss.engine.vector_collection_key" {
                decode_vector_collection_name(&space, &encoded).expect_err(case)
            } else {
                decode_vector_key(&space, &encoded).expect_err(case)
            };
            assert_eq!(error.class(), EngineErrorClass::Corruption);
            assert_eq!(error.code(), code);
        }
    }

    #[test]
    fn vector_decoding_rejects_other_row_families() {
        let space = ProductSpace::new("default").expect("valid space");
        let collection = VectorCollectionName::new("docs").expect("valid collection");
        let vector = encode_vector_key(
            &space,
            &collection,
            &VectorKey::new("doc").expect("valid key"),
        );
        let kv = encode_kv_key(&space, &KvKey::new(b"doc".as_slice()).expect("valid key"));
        let json = encode_json_key(
            &space,
            &JsonDocumentId::new("doc").expect("valid document id"),
        );

        assert_eq!(
            decode_vector_key(&space, &kv)
                .expect_err("KV row rejected")
                .code(),
            "data_loss.engine.vector_key"
        );
        assert_eq!(
            decode_vector_key(&space, &json)
                .expect_err("JSON row rejected")
                .code(),
            "data_loss.engine.vector_key"
        );
        assert_eq!(
            decode_kv_key(&space, &vector)
                .expect_err("vector row rejected")
                .code(),
            "data_loss.engine.kv_key"
        );
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
