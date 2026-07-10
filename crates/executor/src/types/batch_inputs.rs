use super::{Bytes, Deserialize, Serialize, Value};

/// Entry for a batch KV write.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct BatchKvEntry {
    key: Bytes,
    value: Bytes,
}

impl BatchKvEntry {
    /// Creates a batch write entry.
    pub fn new(key: Bytes, value: Bytes) -> Self {
        Self { key, value }
    }

    /// Returns the entry key.
    pub const fn key(&self) -> &Bytes {
        &self.key
    }

    /// Returns the entry value.
    pub const fn value(&self) -> &Bytes {
        &self.value
    }

    /// Consumes the entry.
    pub fn into_parts(self) -> (Bytes, Bytes) {
        (self.key, self.value)
    }
}

/// Entry for a batch JSON set.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BatchJsonEntry {
    key: String,
    path: String,
    value: Value,
}

impl BatchJsonEntry {
    /// Creates a batch JSON set entry.
    pub fn new(key: impl Into<String>, path: impl Into<String>, value: Value) -> Self {
        Self {
            key: key.into(),
            path: path.into(),
            value,
        }
    }

    /// Returns the document key.
    pub fn key(&self) -> &str {
        &self.key
    }

    /// Returns the JSON path.
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Returns the JSON value.
    pub const fn value(&self) -> &Value {
        &self.value
    }

    /// Consumes the entry.
    pub fn into_parts(self) -> (String, String, Value) {
        (self.key, self.path, self.value)
    }
}

/// Entry for a batch JSON get.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct BatchJsonGetEntry {
    key: String,
    path: String,
}

impl BatchJsonGetEntry {
    /// Creates a batch JSON get entry.
    pub fn new(key: impl Into<String>, path: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            path: path.into(),
        }
    }

    /// Returns the document key.
    pub fn key(&self) -> &str {
        &self.key
    }

    /// Returns the JSON path.
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Consumes the entry.
    pub fn into_parts(self) -> (String, String) {
        (self.key, self.path)
    }
}

/// Entry for a batch JSON delete.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct BatchJsonDeleteEntry {
    key: String,
    path: String,
}

impl BatchJsonDeleteEntry {
    /// Creates a batch JSON delete entry.
    pub fn new(key: impl Into<String>, path: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            path: path.into(),
        }
    }

    /// Returns the document key.
    pub fn key(&self) -> &str {
        &self.key
    }

    /// Returns the JSON path.
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Consumes the entry.
    pub fn into_parts(self) -> (String, String) {
        (self.key, self.path)
    }
}
