//! `FastBytes` option for raw `LZMA` encoding.
//
//! The `FastBytes` parameter specifies the number of fast bytes that the raw LZMA encoder uses when searching for matches.
//! "Fast bytes" determine the maximum number of bytes for which the encoder performs a fast match search before switching to a slower, more thorough match finding.
//!
//! Higher values for `FastBytes` can improve compression ratio for some kinds of data, but may also increase memory usage
//! and decrease encoding speed. The valid range for this option is 5 (minimum) to 273 (maximum), following the LZMA SDK specification.

use crate::error::{invalid_option, Error};

const MIN_FAST_BYTES: u32 = 5;
const MAX_FAST_BYTES: u32 = 273;
const DEFAULT_FAST_BYTES: u32 = 32;

/// Number of fast bytes used by the encoder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct FastBytes(u32);

impl FastBytes {
    /// Minimum [`FastBytes`] value supported by the encoder.
    pub const MIN: Self = Self(MIN_FAST_BYTES);
    /// Maximum [`FastBytes`] value supported by the encoder.
    pub const MAX: Self = Self(MAX_FAST_BYTES);
    /// Default [`FastBytes`] value used by the encoder.
    pub const DEFAULT: Self = Self(DEFAULT_FAST_BYTES);

    /// Creates [`FastBytes`] from a raw `u32`.
    ///
    /// # Arguments
    ///
    /// * `value` - The value to create the [`FastBytes`] from.
    ///
    /// # Returns
    ///
    /// A [`Result`] containing the [`FastBytes`] or an [`Error`](crate::error::Error)
    /// if the value is outside the supported range.
    ///
    /// # Errors
    ///
    /// Returns an error if the value is outside the supported range.
    pub fn new(value: u32) -> Result<Self, Error> {
        if (MIN_FAST_BYTES..=MAX_FAST_BYTES).contains(&value) {
            Ok(Self(value))
        } else {
            Err(invalid_option(
                "fast_bytes".into(),
                format!("expected a value in {MIN_FAST_BYTES}..={MAX_FAST_BYTES}"),
            ))
        }
    }

    /// Returns the raw numeric value expected by the SDK.
    pub fn get(self) -> u32 {
        self.0
    }
}

impl From<FastBytes> for i32 {
    fn from(value: FastBytes) -> Self {
        // SAFETY: fast bytes are always in the range of [5, 273].
        i32::try_from(value.0).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::FastBytes;

    #[test]
    fn validates_supported_range() {
        assert_eq!(FastBytes::MIN.get(), 5);
        assert_eq!(FastBytes::DEFAULT.get(), 32);
        assert_eq!(FastBytes::MAX.get(), 273);
        assert_eq!(FastBytes::new(5).unwrap(), FastBytes::MIN);
        assert_eq!(FastBytes::new(32).unwrap(), FastBytes::DEFAULT);
        assert_eq!(FastBytes::new(273).unwrap(), FastBytes::MAX);
        assert!(FastBytes::new(4).is_err());
        assert!(FastBytes::new(274).is_err());
    }
}
