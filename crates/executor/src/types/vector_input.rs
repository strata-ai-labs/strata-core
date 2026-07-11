use super::{Deserialize, Serialize, Value};

/// Vector distance metric exposed through the command boundary.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "idl-tooling", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum VectorDistanceMetric {
    /// Cosine similarity.
    #[default]
    Cosine,
    /// Euclidean similarity.
    Euclidean,
    /// Raw dot product.
    DotProduct,
}

/// Scalar value used by vector metadata filters.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "idl-tooling", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case", tag = "type", content = "value")]
pub enum VectorScalar {
    /// JSON null.
    Null,
    /// Boolean scalar.
    Bool(bool),
    /// Numeric scalar.
    Number(f64),
    /// String scalar.
    String(String),
}

impl From<bool> for VectorScalar {
    fn from(value: bool) -> Self {
        Self::Bool(value)
    }
}

impl From<i32> for VectorScalar {
    fn from(value: i32) -> Self {
        Self::Number(f64::from(value))
    }
}

impl From<f64> for VectorScalar {
    fn from(value: f64) -> Self {
        Self::Number(value)
    }
}

impl From<&str> for VectorScalar {
    fn from(value: &str) -> Self {
        Self::String(value.to_owned())
    }
}

/// Vector metadata filter operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "idl-tooling", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum VectorFilterOp {
    /// Top-level equality.
    Eq,
}

/// One vector metadata filter condition.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "idl-tooling", derive(schemars::JsonSchema))]
pub struct VectorFilterCondition {
    field: String,
    op: VectorFilterOp,
    value: VectorScalar,
}

impl VectorFilterCondition {
    /// Creates a vector metadata filter condition.
    pub fn new(field: impl Into<String>, op: VectorFilterOp, value: VectorScalar) -> Self {
        Self {
            field: field.into(),
            op,
            value,
        }
    }

    /// Creates an equality condition.
    pub fn eq(field: impl Into<String>, value: impl Into<VectorScalar>) -> Self {
        Self::new(field, VectorFilterOp::Eq, value.into())
    }

    /// Returns the metadata field.
    pub fn field(&self) -> &str {
        &self.field
    }

    /// Returns the filter operation.
    pub const fn op(&self) -> VectorFilterOp {
        self.op
    }

    /// Returns the comparison value.
    pub const fn value(&self) -> &VectorScalar {
        &self.value
    }

    /// Consumes the condition.
    pub fn into_parts(self) -> (String, VectorFilterOp, VectorScalar) {
        (self.field, self.op, self.value)
    }
}

/// AND-composed vector metadata filter.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "idl-tooling", derive(schemars::JsonSchema))]
pub struct VectorMetadataFilter {
    conditions: Vec<VectorFilterCondition>,
}

impl VectorMetadataFilter {
    /// Creates a vector metadata filter.
    pub fn new(conditions: Vec<VectorFilterCondition>) -> Self {
        Self { conditions }
    }

    /// Returns filter conditions.
    pub fn conditions(&self) -> &[VectorFilterCondition] {
        &self.conditions
    }

    /// Consumes the filter.
    pub fn into_conditions(self) -> Vec<VectorFilterCondition> {
        self.conditions
    }
}

/// One vector batch upsert entry.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "idl-tooling", derive(schemars::JsonSchema))]
pub struct BatchVectorEntry {
    key: String,
    vector: Vec<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    metadata: Option<Value>,
}

impl BatchVectorEntry {
    /// Creates a vector batch upsert entry.
    pub fn new(key: impl Into<String>, vector: Vec<f32>, metadata: Option<Value>) -> Self {
        Self {
            key: key.into(),
            vector,
            metadata,
        }
    }

    /// Returns the vector key.
    pub fn key(&self) -> &str {
        &self.key
    }

    /// Returns the embedding.
    pub fn vector(&self) -> &[f32] {
        &self.vector
    }

    /// Returns optional metadata.
    pub const fn metadata(&self) -> Option<&Value> {
        self.metadata.as_ref()
    }

    /// Consumes the entry.
    pub fn into_parts(self) -> (String, Vec<f32>, Option<Value>) {
        (self.key, self.vector, self.metadata)
    }
}
