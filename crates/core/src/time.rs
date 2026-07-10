//! Timestamp representation atoms.

use serde::{Deserialize, Serialize};
use std::{fmt, num::ParseIntError, str::FromStr, time::Duration};

const MICROS_PER_MILLI: u64 = 1_000;
const MICROS_PER_SEC: u64 = 1_000_000;

/// Microsecond timestamp since the Unix epoch.
///
/// This type is only a representation and arithmetic contract. It does not
/// read clocks or define product time-travel policy.
#[derive(
    Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize,
)]
#[repr(transparent)]
#[serde(transparent)]
pub struct Timestamp(u64);

impl Timestamp {
    /// Unix epoch timestamp.
    pub const EPOCH: Self = Self(0);

    /// Largest representable timestamp.
    pub const MAX: Self = Self(u64::MAX);

    /// Creates a timestamp from microseconds since the Unix epoch.
    #[must_use]
    pub const fn from_micros(micros: u64) -> Self {
        Self(micros)
    }

    /// Creates a timestamp from milliseconds since the Unix epoch.
    ///
    /// Values that exceed the representable range saturate at
    /// [`Timestamp::MAX`].
    #[must_use]
    pub const fn from_millis(millis: u64) -> Self {
        Self(millis.saturating_mul(MICROS_PER_MILLI))
    }

    /// Creates a timestamp from seconds since the Unix epoch.
    ///
    /// Values that exceed the representable range saturate at
    /// [`Timestamp::MAX`].
    #[must_use]
    pub const fn from_secs(secs: u64) -> Self {
        Self(secs.saturating_mul(MICROS_PER_SEC))
    }

    /// Creates a timestamp from a duration since the Unix epoch.
    ///
    /// Values that exceed the representable range saturate at
    /// [`Timestamp::MAX`].
    #[must_use]
    pub fn from_duration_since_epoch(duration: Duration) -> Self {
        Self(u64::try_from(duration.as_micros()).unwrap_or(u64::MAX))
    }

    /// Returns microseconds since the Unix epoch.
    #[must_use]
    pub const fn as_micros(self) -> u64 {
        self.0
    }

    /// Returns milliseconds since the Unix epoch, truncating sub-millisecond
    /// precision.
    #[must_use]
    pub const fn as_millis(self) -> u64 {
        self.0 / MICROS_PER_MILLI
    }

    /// Returns seconds since the Unix epoch, truncating sub-second precision.
    #[must_use]
    pub const fn as_secs(self) -> u64 {
        self.0 / MICROS_PER_SEC
    }

    /// Returns the duration from `earlier` to this timestamp.
    ///
    /// `None` means `earlier` is after this timestamp.
    #[must_use]
    pub const fn duration_since(self, earlier: Self) -> Option<Duration> {
        match self.0.checked_sub(earlier.0) {
            Some(micros) => Some(Duration::from_micros(micros)),
            None => None,
        }
    }

    /// Adds a duration, saturating at [`Timestamp::MAX`].
    #[must_use]
    pub fn saturating_add(self, duration: Duration) -> Self {
        let addend = u64::try_from(duration.as_micros()).unwrap_or(u64::MAX);
        Self(self.0.saturating_add(addend))
    }

    /// Subtracts a duration, saturating at [`Timestamp::EPOCH`].
    #[must_use]
    pub fn saturating_sub(self, duration: Duration) -> Self {
        let subtrahend = u64::try_from(duration.as_micros()).unwrap_or(u64::MAX);
        Self(self.0.saturating_sub(subtrahend))
    }
}

impl fmt::Display for Timestamp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for Timestamp {
    type Err = ParseTimestampError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        if text.starts_with('+') {
            return Err(ParseTimestampError { source: None });
        }

        let micros = text.parse::<u64>().map_err(|source| ParseTimestampError {
            source: Some(source),
        })?;
        Ok(Self(micros))
    }
}

/// Error returned when text cannot form a [`Timestamp`].
#[derive(Debug)]
#[non_exhaustive]
pub struct ParseTimestampError {
    source: Option<ParseIntError>,
}

impl fmt::Display for ParseTimestampError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("invalid timestamp text")
    }
}

impl std::error::Error for ParseTimestampError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source
            .as_ref()
            .map(|source| source as &(dyn std::error::Error + 'static))
    }
}

#[cfg(test)]
mod tests {
    use super::Timestamp;
    use std::{error::Error, str::FromStr, time::Duration};

    #[test]
    fn explicit_microsecond_constructor_preserves_raw_value() {
        let timestamp = Timestamp::from_micros(1_234_567);

        assert_eq!(timestamp.as_micros(), 1_234_567);
    }

    #[test]
    fn millisecond_constructor_converts_to_microseconds() {
        let timestamp = Timestamp::from_millis(5_000);

        assert_eq!(timestamp.as_micros(), 5_000_000);
        assert_eq!(timestamp.as_millis(), 5_000);
        assert_eq!(timestamp.as_secs(), 5);
    }

    #[test]
    fn second_constructor_converts_to_microseconds() {
        let timestamp = Timestamp::from_secs(7);

        assert_eq!(timestamp.as_micros(), 7_000_000);
        assert_eq!(timestamp.as_millis(), 7_000);
        assert_eq!(timestamp.as_secs(), 7);
    }

    #[test]
    fn constants_preserve_numeric_bounds() {
        assert_eq!(Timestamp::EPOCH.as_micros(), 0);
        assert_eq!(Timestamp::MAX.as_micros(), u64::MAX);
    }

    #[test]
    fn default_is_epoch() {
        assert_eq!(Timestamp::default(), Timestamp::EPOCH);
    }

    #[test]
    fn ordering_follows_microsecond_order() {
        assert!(Timestamp::from_micros(2) > Timestamp::from_micros(1));
        assert!(Timestamp::EPOCH < Timestamp::MAX);
    }

    #[test]
    fn larger_unit_constructors_saturate() {
        assert_eq!(Timestamp::from_millis(u64::MAX), Timestamp::MAX);
        assert_eq!(Timestamp::from_secs(u64::MAX), Timestamp::MAX);
    }

    #[test]
    fn duration_since_returns_elapsed_duration() {
        let earlier = Timestamp::from_micros(10);
        let later = Timestamp::from_micros(15);

        assert_eq!(
            later.duration_since(earlier),
            Some(Duration::from_micros(5))
        );
    }

    #[test]
    fn duration_since_refuses_negative_duration() {
        let earlier = Timestamp::from_micros(10);
        let later = Timestamp::from_micros(15);

        assert_eq!(earlier.duration_since(later), None);
    }

    #[test]
    fn saturating_add_caps_at_max() {
        assert_eq!(
            Timestamp::MAX.saturating_add(Duration::from_micros(1)),
            Timestamp::MAX
        );
    }

    #[test]
    fn saturating_sub_caps_at_epoch() {
        assert_eq!(
            Timestamp::EPOCH.saturating_sub(Duration::from_micros(1)),
            Timestamp::EPOCH
        );
    }

    #[test]
    fn duration_since_epoch_constructor_saturates() {
        let duration = Duration::from_micros(u64::MAX).saturating_add(Duration::from_micros(1));

        assert_eq!(
            Timestamp::from_duration_since_epoch(duration),
            Timestamp::MAX
        );
    }

    #[test]
    fn display_uses_raw_microsecond_value() {
        assert_eq!(Timestamp::from_micros(1_234_567).to_string(), "1234567");
    }

    #[test]
    fn from_str_accepts_raw_microsecond_value() {
        let timestamp = Timestamp::from_str("1234567").expect("valid timestamp");

        assert_eq!(timestamp, Timestamp::from_micros(1_234_567));
    }

    #[test]
    fn from_str_rejects_invalid_text() {
        let err = Timestamp::from_str("1.234567").expect_err("decorated text should fail");

        assert_eq!(err.to_string(), "invalid timestamp text");
        assert!(err.source().is_some());
    }

    #[test]
    fn from_str_rejects_leading_plus() {
        let err = Timestamp::from_str("+1234567").expect_err("signed text should fail");

        assert_eq!(err.to_string(), "invalid timestamp text");
        assert!(err.source().is_none());
    }

    #[test]
    fn serde_uses_transparent_numeric_value() {
        let json =
            serde_json::to_string(&Timestamp::from_micros(1_234_567)).expect("serialize timestamp");

        assert_eq!(json, "1234567");
        assert_eq!(
            serde_json::from_str::<Timestamp>(&json).expect("deserialize timestamp"),
            Timestamp::from_micros(1_234_567)
        );
    }

    #[test]
    fn serde_binary_round_trips_numeric_value() {
        let bytes =
            bincode::serialize(&Timestamp::from_micros(1_234_567)).expect("serialize timestamp");

        assert_eq!(
            bincode::deserialize::<Timestamp>(&bytes).expect("deserialize timestamp"),
            Timestamp::from_micros(1_234_567)
        );
    }

    #[test]
    fn representation_matches_u64_layout() {
        assert_eq!(std::mem::size_of::<Timestamp>(), std::mem::size_of::<u64>());
        assert_eq!(
            std::mem::align_of::<Timestamp>(),
            std::mem::align_of::<u64>()
        );
    }
}
