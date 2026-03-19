//! `CompressionLevel` option for raw `LZMA` encoding.
//
//! The `CompressionLevel` parameter determines the overall tradeoff between
//! compression speed and compression ratio in the LZMA encoder.
//! Level 0 is the fastest but produces the largest output, while level 9
//! yields the smallest output but is the slowest to encode.
//
//! The valid range for `CompressionLevel` is 0 (minimum, fastest, lowest compression)
//! to 9 (maximum, slowest, highest compression).
//! The default recommended value is 5, balancing speed and ratio.

use crate::error::{invalid_option, Error};

const MIN_COMPRESSION_LEVEL: u32 = 0;
const MAX_COMPRESSION_LEVEL: u32 = 9;
const DEFAULT_COMPRESSION_LEVEL: u32 = 5;

/// Compression level used by raw `LZMA` encoders.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct CompressionLevel(u32);

impl CompressionLevel {
    /// Minimum [`CompressionLevel`] value supported by the encoder.
    pub const MIN: Self = Self(MIN_COMPRESSION_LEVEL);
    /// Maximum [`CompressionLevel`] value supported by the encoder.
    pub const MAX: Self = Self(MAX_COMPRESSION_LEVEL);
    /// Default [`CompressionLevel`] value used by the encoder.
    pub const DEFAULT: Self = Self(DEFAULT_COMPRESSION_LEVEL);

    /// Creates a [`CompressionLevel`] from a raw `u32`.
    ///
    /// # Arguments
    ///
    /// * `value` - The value to create the [`CompressionLevel`] from.
    ///
    /// # Returns
    ///
    /// A [`Result`] containing the [`CompressionLevel`] or an [`Error`](crate::error::Error)
    /// if the value is outside the supported range.
    ///
    /// # Errors
    ///
    /// Returns an error if the value is outside the supported range.
    pub fn new(value: u32) -> Result<Self, Error> {
        if (MIN_COMPRESSION_LEVEL..=MAX_COMPRESSION_LEVEL).contains(&value) {
            Ok(Self(value))
        } else {
            Err(invalid_option(
                "level".into(),
                format!("expected a value in {MIN_COMPRESSION_LEVEL}..={MAX_COMPRESSION_LEVEL}"),
            ))
        }
    }

    /// Returns the raw numeric value expected by the SDK.
    pub fn get(self) -> u32 {
        self.0
    }
}

impl From<CompressionLevel> for i32 {
    fn from(level: CompressionLevel) -> Self {
        // SAFETY: compression level is always in the range of [0, 9].
        i32::try_from(level.0).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::CompressionLevel;

    #[test]
    fn validates_supported_range() {
        assert_eq!(CompressionLevel::MIN.get(), 0);
        assert_eq!(CompressionLevel::DEFAULT.get(), 5);
        assert_eq!(CompressionLevel::MAX.get(), 9);
        assert_eq!(CompressionLevel::new(0).unwrap(), CompressionLevel::MIN);
        assert_eq!(CompressionLevel::new(5).unwrap(), CompressionLevel::DEFAULT);
        assert_eq!(CompressionLevel::new(9).unwrap(), CompressionLevel::MAX);
        assert!(CompressionLevel::new(10).is_err());
    }
}
