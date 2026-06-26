use super::{Deserialize, Serialize, Value};

/// Event range direction exposed through the command boundary.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventRangeDirection {
    /// Increasing sequence or timestamp order.
    #[default]
    Forward,
    /// Decreasing sequence or timestamp order.
    Reverse,
}

/// One event batch append entry.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BatchEventEntry {
    event_type: String,
    payload: Value,
}

impl BatchEventEntry {
    /// Creates an event batch append entry.
    pub fn new(event_type: impl Into<String>, payload: Value) -> Self {
        Self {
            event_type: event_type.into(),
            payload,
        }
    }

    /// Returns the event type.
    pub fn event_type(&self) -> &str {
        &self.event_type
    }

    /// Returns the event payload.
    pub const fn payload(&self) -> &Value {
        &self.payload
    }

    /// Consumes the entry.
    pub fn into_parts(self) -> (String, Value) {
        (self.event_type, self.payload)
    }
}
