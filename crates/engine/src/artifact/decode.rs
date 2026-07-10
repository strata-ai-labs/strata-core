//! SAP1 section decoding — the exact inverse of `export.rs` encoding.
//!
//! Consumers (the hub adapter's schema/preview generation today, bundle
//! import next) iterate typed [`ArtifactRecord`]s out of a section's byte
//! stream. Truncated or malformed streams surface
//! `corruption.engine.artifact_payload`.

use strata_core::Timestamp;

use crate::api::EngineError;

use super::ArtifactModel;

/// One decoded SAP1 record.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum ArtifactRecord {
    /// One KV row.
    Kv {
        /// Key bytes.
        key: Vec<u8>,
        /// Value bytes.
        value: Vec<u8>,
        /// Commit timestamp.
        timestamp: Timestamp,
    },
    /// One JSON document.
    Json {
        /// Document id.
        id: String,
        /// The document as serialized JSON.
        doc: Vec<u8>,
        /// Commit timestamp.
        timestamp: Timestamp,
    },
    /// One event record.
    Event {
        /// Event sequence number.
        sequence: u64,
        /// Event type.
        event_type: String,
        /// Payload as serialized JSON.
        payload: Vec<u8>,
        /// Event timestamp (wall clock at append).
        timestamp: Timestamp,
    },
    /// Vector section header: the collection config.
    VectorConfig {
        /// Collection config as serialized JSON.
        config: Vec<u8>,
        /// Collection creation timestamp.
        timestamp: Timestamp,
    },
    /// One vector entry.
    VectorEntry {
        /// Vector key.
        key: String,
        /// Embedding components.
        embedding: Vec<f32>,
        /// Metadata as serialized JSON, when present.
        metadata: Option<Vec<u8>>,
        /// Commit timestamp.
        timestamp: Timestamp,
    },
    /// Graph section header: graph metadata.
    GraphMeta {
        /// Ontology as serialized JSON, when defined.
        ontology: Option<Vec<u8>>,
    },
    /// One graph node.
    GraphNode {
        /// Node id.
        id: String,
        /// Node data as serialized JSON.
        data: Vec<u8>,
        /// Commit timestamp.
        timestamp: Timestamp,
    },
    /// One graph edge.
    GraphEdge {
        /// Source node id.
        src: String,
        /// Edge type.
        edge_type: String,
        /// Destination node id.
        dst: String,
        /// Edge data as serialized JSON.
        data: Vec<u8>,
        /// Commit timestamp.
        timestamp: Timestamp,
    },
}

/// Iterator of decoded records over one section's bytes.
#[derive(Debug)]
pub struct ArtifactRecordIter<'a> {
    model: ArtifactModel,
    bytes: &'a [u8],
    index: u64,
}

/// Decodes a SAP1 section byte stream (as produced by
/// [`crate::api::Database::export_branch_artifact`]) into typed records.
#[must_use]
pub fn decode_section(model: ArtifactModel, bytes: &[u8]) -> ArtifactRecordIter<'_> {
    ArtifactRecordIter {
        model,
        bytes,
        index: 0,
    }
}

impl Iterator for ArtifactRecordIter<'_> {
    type Item = Result<ArtifactRecord, EngineError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.bytes.is_empty() {
            return None;
        }
        let result = self.next_record();
        if result.is_err() {
            // Poison the iterator: a framing error makes every later
            // offset meaningless.
            self.bytes = &[];
        }
        self.index += 1;
        Some(result)
    }
}

impl ArtifactRecordIter<'_> {
    fn next_record(&mut self) -> Result<ArtifactRecord, EngineError> {
        let length = read_u32(&mut self.bytes)? as usize;
        if self.bytes.len() < length {
            return Err(payload_corruption("record body is truncated"));
        }
        let (mut body, rest) = self.bytes.split_at(length);
        self.bytes = rest;

        let record = match self.model {
            ArtifactModel::Kv => ArtifactRecord::Kv {
                key: read_bytes(&mut body)?,
                value: read_bytes(&mut body)?,
                timestamp: read_timestamp(&mut body)?,
            },
            ArtifactModel::Json => ArtifactRecord::Json {
                id: read_string(&mut body)?,
                doc: read_bytes(&mut body)?,
                timestamp: read_timestamp(&mut body)?,
            },
            ArtifactModel::Event => ArtifactRecord::Event {
                sequence: read_u64(&mut body)?,
                event_type: read_string(&mut body)?,
                payload: read_bytes(&mut body)?,
                timestamp: read_timestamp(&mut body)?,
            },
            ArtifactModel::Vector if self.index == 0 => ArtifactRecord::VectorConfig {
                config: read_bytes(&mut body)?,
                timestamp: read_timestamp(&mut body)?,
            },
            ArtifactModel::Vector => ArtifactRecord::VectorEntry {
                key: read_string(&mut body)?,
                embedding: read_f32s(&mut body)?,
                metadata: read_opt_bytes(&mut body)?,
                timestamp: read_timestamp(&mut body)?,
            },
            ArtifactModel::Graph if self.index == 0 => ArtifactRecord::GraphMeta {
                ontology: read_opt_bytes(&mut body)?,
            },
            ArtifactModel::Graph => match read_u8(&mut body)? {
                0 => ArtifactRecord::GraphNode {
                    id: read_string(&mut body)?,
                    data: read_bytes(&mut body)?,
                    timestamp: read_timestamp(&mut body)?,
                },
                1 => ArtifactRecord::GraphEdge {
                    src: read_string(&mut body)?,
                    edge_type: read_string(&mut body)?,
                    dst: read_string(&mut body)?,
                    data: read_bytes(&mut body)?,
                    timestamp: read_timestamp(&mut body)?,
                },
                _ => return Err(payload_corruption("unknown graph record tag")),
            },
        };
        if !body.is_empty() {
            return Err(payload_corruption("record carries trailing bytes"));
        }
        Ok(record)
    }
}

fn payload_corruption(reason: &str) -> EngineError {
    EngineError::corruption(
        "corruption.engine.artifact_payload",
        format!("artifact payload section is malformed: {reason}"),
    )
}

fn read_u8(bytes: &mut &[u8]) -> Result<u8, EngineError> {
    let Some((&first, rest)) = bytes.split_first() else {
        return Err(payload_corruption("record body is truncated"));
    };
    *bytes = rest;
    Ok(first)
}

fn read_u32(bytes: &mut &[u8]) -> Result<u32, EngineError> {
    if bytes.len() < 4 {
        return Err(payload_corruption("record body is truncated"));
    }
    let (head, rest) = bytes.split_at(4);
    *bytes = rest;
    let mut buffer = [0_u8; 4];
    buffer.copy_from_slice(head);
    Ok(u32::from_le_bytes(buffer))
}

fn read_u64(bytes: &mut &[u8]) -> Result<u64, EngineError> {
    if bytes.len() < 8 {
        return Err(payload_corruption("record body is truncated"));
    }
    let (head, rest) = bytes.split_at(8);
    *bytes = rest;
    let mut buffer = [0_u8; 8];
    buffer.copy_from_slice(head);
    Ok(u64::from_le_bytes(buffer))
}

fn read_timestamp(bytes: &mut &[u8]) -> Result<Timestamp, EngineError> {
    Ok(Timestamp::from_micros(read_u64(bytes)?))
}

fn read_bytes(bytes: &mut &[u8]) -> Result<Vec<u8>, EngineError> {
    let length = read_u32(bytes)? as usize;
    if bytes.len() < length {
        return Err(payload_corruption("field is truncated"));
    }
    let (head, rest) = bytes.split_at(length);
    *bytes = rest;
    Ok(head.to_vec())
}

fn read_string(bytes: &mut &[u8]) -> Result<String, EngineError> {
    String::from_utf8(read_bytes(bytes)?)
        .map_err(|_| payload_corruption("string field is not UTF-8"))
}

fn read_opt_bytes(bytes: &mut &[u8]) -> Result<Option<Vec<u8>>, EngineError> {
    match read_u8(bytes)? {
        0 => Ok(None),
        1 => Ok(Some(read_bytes(bytes)?)),
        _ => Err(payload_corruption("unknown option tag")),
    }
}

fn read_f32s(bytes: &mut &[u8]) -> Result<Vec<f32>, EngineError> {
    let count = read_u32(bytes)? as usize;
    if bytes.len() < count * 4 {
        return Err(payload_corruption("embedding field is truncated"));
    }
    let (head, rest) = bytes.split_at(count * 4);
    *bytes = rest;
    Ok(head
        .chunks_exact(4)
        .map(|chunk| {
            let mut buffer = [0_u8; 4];
            buffer.copy_from_slice(chunk);
            f32::from_le_bytes(buffer)
        })
        .collect())
}
