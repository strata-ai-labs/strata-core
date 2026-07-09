//! Typed device-runtime errors following the V1 error contract
//! (`<class>.<area>.<detail>`, area `gpu`).

use std::fmt;

/// Device runtime error.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum GpuError {
    /// The CUDA driver library is not present or failed to load.
    DriverMissing {
        /// Loader detail (library name, dlopen message).
        detail: String,
    },
    /// A resolved driver call returned a non-success status.
    DriverCall {
        /// Driver entry point name.
        call: &'static str,
        /// CUDA result code.
        code: i32,
        /// Driver-rendered error string when available.
        detail: String,
    },
    /// No CUDA device is present.
    NoDevice,
    /// The device's compute capability is below the supported floor.
    ComputeCapability {
        /// Detected major version.
        major: i32,
        /// Detected minor version.
        minor: i32,
        /// Required floor, e.g. (8, 0).
        floor: (i32, i32),
    },
    /// The arena cannot satisfy an allocation within its fixed budget.
    ArenaExhausted {
        /// Requesting region name.
        region: &'static str,
        /// Bytes requested.
        requested: u64,
        /// Bytes available.
        available: u64,
    },
    /// An arena/region configuration is invalid (zero sizes, overflow,
    /// misalignment).
    InvalidConfig {
        /// What was wrong.
        detail: String,
    },
}

impl GpuError {
    /// Stable public error code (`<class>.gpu.<detail>`).
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::DriverMissing { .. } => "unavailable.gpu.driver_missing",
            Self::DriverCall { .. } => "unavailable.gpu.driver_call",
            Self::NoDevice => "failed_precondition.gpu.no_device",
            Self::ComputeCapability { .. } => "failed_precondition.gpu.compute_capability",
            Self::ArenaExhausted { .. } => "resource_exhausted.gpu.arena",
            Self::InvalidConfig { .. } => "invalid_argument.gpu.config",
        }
    }
}

impl fmt::Display for GpuError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DriverMissing { detail } => {
                write!(f, "{}: CUDA driver unavailable: {detail}", self.code())
            }
            Self::DriverCall { call, code, detail } => {
                write!(f, "{}: {call} failed ({code}): {detail}", self.code())
            }
            Self::NoDevice => write!(f, "{}: no CUDA device present", self.code()),
            Self::ComputeCapability {
                major,
                minor,
                floor,
            } => write!(
                f,
                "{}: compute capability {major}.{minor} below required {}.{}",
                self.code(),
                floor.0,
                floor.1
            ),
            Self::ArenaExhausted {
                region,
                requested,
                available,
            } => write!(
                f,
                "{}: region `{region}` requested {requested} bytes, {available} available",
                self.code()
            ),
            Self::InvalidConfig { detail } => write!(f, "{}: {detail}", self.code()),
        }
    }
}

impl std::error::Error for GpuError {}

#[cfg(test)]
mod tests {
    use super::GpuError;

    #[test]
    fn codes_follow_the_contract_shape() {
        let samples = [
            GpuError::DriverMissing {
                detail: String::new(),
            }
            .code(),
            GpuError::NoDevice.code(),
            GpuError::ArenaExhausted {
                region: "pages",
                requested: 1,
                available: 0,
            }
            .code(),
        ];
        for code in samples {
            let parts: Vec<&str> = code.split('.').collect();
            assert_eq!(parts.len(), 3, "{code} must be <class>.<area>.<detail>");
            assert_eq!(parts[1], "gpu");
        }
    }
}
