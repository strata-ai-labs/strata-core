//! Schema + preview blob generation (coordination doc §3.4, M8E3).
//!
//! Both blobs derive from the default branch's decoded payload sections,
//! so they are a pure function of exported content — two exports of the
//! same database produce byte-identical blobs. The typed shapes are
//! imported from `stratahub-protocol` (their M2E4 types), making
//! wire-format conformance structural rather than tested-for.

use std::collections::BTreeMap;

use serde_json::Value;
use stratahub_protocol::wire::{
    DatasetSchema, EventSampleEntry, EventStream, EventsSchema, JsonSampleEntry, JsonSchema,
    KvNamespace, KvSampleEntry, KvSchema, SamplePreview, VectorCollection, VectorMetric,
    VectorSampleEntry, VectorsSchema,
};

use strata_engine::artifact::{decode_section, ArtifactModel, ArtifactRecord, BranchArtifact};

use crate::error::BundleExportError;

/// Samples per primitive in the preview blob.
const PREVIEW_SAMPLES: usize = 3;

/// Preview string truncation budget (§3.4 invariant 4 convention).
const PREVIEW_TRUNCATE_CHARS: usize = 200;

/// Derives the schema + preview for a bundle from its default branch.
///
/// The preview's branches section is attached by the caller once every
/// branch has been exported.
pub(crate) fn generate(
    artifact: &BranchArtifact,
) -> Result<(DatasetSchema, SamplePreview), BundleExportError> {
    let mut schema = DatasetSchema::default();
    let mut preview = SamplePreview::default();
    let mut kv = KvAccumulator::default();
    let mut json = JsonAccumulator::default();
    let mut vectors = Vec::new();
    let mut vector_samples = Vec::new();
    let mut events = EventAccumulator::default();

    for section in artifact.sections() {
        let records = decode_section(section.model(), section.bytes());
        match section.model() {
            ArtifactModel::Kv => kv.consume(records)?,
            ArtifactModel::Json => json.consume(records)?,
            ArtifactModel::Event => events.consume(records)?,
            ArtifactModel::Vector => {
                let name = section.qualifier().unwrap_or_default().to_owned();
                consume_vector_section(name, records, &mut vectors, &mut vector_samples)?;
            }
            // The protocol schema has no graph sub-object yet (flagged
            // cross-repo); graph content rides the bundle undescribed.
            // Future engine models are likewise undescribed until the
            // protocol grows matching sub-objects.
            _ => {}
        }
    }

    schema.kv = kv.schema();
    schema.json = json.schema();
    schema.events = events.schema();
    if !vectors.is_empty() {
        schema.vectors = Some(VectorsSchema {
            collections: vectors,
        });
    }

    preview.kv = kv.samples();
    preview.json = json.into_samples();
    preview.events = events.into_samples();
    if !vector_samples.is_empty() {
        preview.vectors = Some(vector_samples);
    }
    Ok((schema, preview))
}

/// Truncates for display: at most [`PREVIEW_TRUNCATE_CHARS`] characters,
/// with `…` marking elision.
fn truncate_for_preview(text: &str) -> String {
    if text.chars().count() <= PREVIEW_TRUNCATE_CHARS {
        return text.to_owned();
    }
    let mut truncated: String = text.chars().take(PREVIEW_TRUNCATE_CHARS).collect();
    truncated.push('…');
    truncated
}

fn decode_error(error: &strata_engine::EngineError) -> BundleExportError {
    BundleExportError::Internal {
        detail: format!("payload section decode failed: {}", error.code()),
    }
}

/// Namespace grouping rule: everything up to and including the first `:`;
/// keys without a separator share the catch-all empty prefix.
fn kv_namespace_prefix(key: &str) -> String {
    key.find(':')
        .map_or_else(String::new, |position| key[..=position].to_owned())
}

fn json_type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

#[derive(Default)]
struct KvAccumulator {
    namespaces: BTreeMap<String, NamespaceFacts>,
    samples: Vec<KvSampleEntry>,
}

#[derive(Default)]
struct NamespaceFacts {
    entry_count: u64,
    types: std::collections::BTreeSet<&'static str>,
}

impl KvAccumulator {
    fn consume(
        &mut self,
        records: strata_engine::artifact::ArtifactRecordIter<'_>,
    ) -> Result<(), BundleExportError> {
        for record in records {
            let ArtifactRecord::Kv { key, value, .. } =
                record.map_err(|error| decode_error(&error))?
            else {
                continue;
            };
            let key = String::from_utf8_lossy(&key).into_owned();
            let value_text = String::from_utf8_lossy(&value).into_owned();
            let facts = self
                .namespaces
                .entry(kv_namespace_prefix(&key))
                .or_default();
            facts.entry_count += 1;
            facts
                .types
                .insert(if serde_json::from_slice::<Value>(&value).is_ok() {
                    "json"
                } else {
                    "raw"
                });
            if self.samples.len() < PREVIEW_SAMPLES {
                self.samples.push(KvSampleEntry {
                    key,
                    value_summary: truncate_for_preview(&value_text),
                });
            }
        }
        Ok(())
    }

    fn schema(&self) -> Option<KvSchema> {
        if self.namespaces.is_empty() {
            return None;
        }
        Some(KvSchema {
            namespaces: self
                .namespaces
                .iter()
                .map(|(prefix, facts)| KvNamespace {
                    prefix: prefix.clone(),
                    value_type: facts.types.iter().copied().collect::<Vec<_>>().join(" | "),
                    entry_count: facts.entry_count,
                })
                .collect(),
        })
    }

    fn samples(&self) -> Option<Vec<KvSampleEntry>> {
        (!self.samples.is_empty()).then(|| self.samples.clone())
    }
}

#[derive(Default)]
struct JsonAccumulator {
    fields: BTreeMap<String, std::collections::BTreeSet<&'static str>>,
    samples: Vec<JsonSampleEntry>,
}

impl JsonAccumulator {
    fn consume(
        &mut self,
        records: strata_engine::artifact::ArtifactRecordIter<'_>,
    ) -> Result<(), BundleExportError> {
        for record in records {
            let ArtifactRecord::Json { doc, .. } = record.map_err(|error| decode_error(&error))?
            else {
                continue;
            };
            let Ok(Value::Object(document)) = serde_json::from_slice::<Value>(&doc) else {
                continue;
            };
            let sample_this = self.samples.is_empty();
            for (field, value) in &document {
                self.fields
                    .entry(field.clone())
                    .or_default()
                    .insert(json_type_name(value));
                if sample_this && self.samples.len() < PREVIEW_SAMPLES {
                    let example = match value {
                        Value::String(text) => Value::String(truncate_for_preview(text)),
                        other => other.clone(),
                    };
                    self.samples.push(JsonSampleEntry {
                        path: format!("$.{field}"),
                        example_value: example,
                    });
                }
            }
        }
        Ok(())
    }

    fn schema(&self) -> Option<JsonSchema> {
        if self.fields.is_empty() {
            return None;
        }
        Some(JsonSchema {
            fields: self
                .fields
                .iter()
                .map(|(field, types)| {
                    let expression = types.iter().copied().collect::<Vec<_>>().join(" | ");
                    (field.clone(), expression)
                })
                .collect(),
        })
    }

    fn into_samples(self) -> Option<Vec<JsonSampleEntry>> {
        (!self.samples.is_empty()).then_some(self.samples)
    }
}

#[derive(Default)]
struct EventAccumulator {
    streams: BTreeMap<String, BTreeMap<String, std::collections::BTreeSet<&'static str>>>,
    samples: Vec<EventSampleEntry>,
}

impl EventAccumulator {
    fn consume(
        &mut self,
        records: strata_engine::artifact::ArtifactRecordIter<'_>,
    ) -> Result<(), BundleExportError> {
        for record in records {
            let ArtifactRecord::Event {
                event_type,
                payload,
                timestamp,
                ..
            } = record.map_err(|error| decode_error(&error))?
            else {
                continue;
            };
            let payload_value = serde_json::from_slice::<Value>(&payload).ok();
            let fields = self.streams.entry(event_type.clone()).or_default();
            if let Some(Value::Object(object)) = &payload_value {
                for (field, value) in object {
                    fields
                        .entry(field.clone())
                        .or_default()
                        .insert(json_type_name(value));
                }
            }
            if self.samples.len() < PREVIEW_SAMPLES {
                let summary = payload_value
                    .as_ref()
                    .map_or_else(String::new, ToString::to_string);
                let timestamp = time::OffsetDateTime::from_unix_timestamp_nanos(
                    i128::from(timestamp.as_micros()) * 1_000,
                )
                .map_err(|error| BundleExportError::Internal {
                    detail: format!("event timestamp out of range: {error}"),
                })?;
                self.samples.push(EventSampleEntry {
                    stream: event_type,
                    timestamp,
                    event_summary: truncate_for_preview(&summary),
                });
            }
        }
        Ok(())
    }

    fn schema(&self) -> Option<EventsSchema> {
        if self.streams.is_empty() {
            return None;
        }
        Some(EventsSchema {
            streams: self
                .streams
                .iter()
                .map(|(name, fields)| {
                    let properties: serde_json::Map<String, Value> = fields
                        .iter()
                        .map(|(field, types)| {
                            let expression = types.iter().copied().collect::<Vec<_>>().join(" | ");
                            (field.clone(), serde_json::json!({ "type": expression }))
                        })
                        .collect();
                    EventStream {
                        name: name.clone(),
                        event_shape: serde_json::json!({
                            "type": "object",
                            "properties": properties,
                        }),
                    }
                })
                .collect(),
        })
    }

    fn into_samples(self) -> Option<Vec<EventSampleEntry>> {
        (!self.samples.is_empty()).then_some(self.samples)
    }
}

fn consume_vector_section(
    name: String,
    records: strata_engine::artifact::ArtifactRecordIter<'_>,
    collections: &mut Vec<VectorCollection>,
    samples: &mut Vec<VectorSampleEntry>,
) -> Result<(), BundleExportError> {
    let mut dimension = 0_u32;
    let mut count = 0_u64;
    let mut metric = VectorMetric::Cosine;
    for record in records {
        match record.map_err(|error| decode_error(&error))? {
            ArtifactRecord::VectorConfig { config, .. } => {
                let config: Value = serde_json::from_slice(&config).map_err(|error| {
                    BundleExportError::Internal {
                        detail: format!("vector config decode: {error}"),
                    }
                })?;
                dimension =
                    u32::try_from(config["dimension"].as_u64().unwrap_or(0)).unwrap_or(u32::MAX);
                metric = match config["metric"].as_str() {
                    Some("euclidean") => VectorMetric::L2,
                    Some("dot_product") => VectorMetric::Dot,
                    // Cosine is the engine default and the safe fallback
                    // for any future metric the schema cannot express yet.
                    _ => VectorMetric::Cosine,
                };
            }
            ArtifactRecord::VectorEntry {
                embedding,
                metadata,
                ..
            } => {
                count += 1;
                if count == 1 {
                    let example_metadata = metadata
                        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
                        .and_then(|value| match value {
                            Value::Object(map) => Some(map),
                            _ => None,
                        })
                        .unwrap_or_default();
                    samples.push(VectorSampleEntry {
                        collection: name.clone(),
                        dimension,
                        example_metadata,
                        vector_preview: embedding
                            .iter()
                            .take(8)
                            .map(|component| f64::from(*component))
                            .collect(),
                    });
                }
            }
            _ => {}
        }
    }
    collections.push(VectorCollection {
        name,
        dimension,
        count,
        metric,
    });
    Ok(())
}
