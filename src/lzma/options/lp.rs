//! `LiteralPosBits` option for raw `LZMA` encoding.
//
//! The `LiteralPosBits` parameter specifies the number of low bits of the match position to include
//! in the context modeling for literal bytes in the LZMA algorithm.
//!
//! Increasing the number of literal position bits (`lp`) can sometimes improve compression for files
//! with periodic patterns aligned to certain positions, but can also increase memory usage and slow
//! down encoding/decoding. The valid range for this option is 0 to 4, where 0 is the typical default.

use crate::error::{invalid_option, Error};

const MIN_LITERAL_POSITION_BITS: u32 = 0;
const MAX_LITERAL_POSITION_BITS: u32 = 4;
const DEFAULT_LITERAL_POSITION_BITS: u32 = 0;

/// Number of literal position bits (`lp`) for raw `LZMA`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct LiteralPosBits(u32);

impl LiteralPosBits {
    /// Minimum [`LiteralPosBits`] value supported by the encoder.
    pub const MIN: Self = Self(MIN_LITERAL_POSITION_BITS);
    /// Maximum [`LiteralPosBits`] value supported by the encoder.
    pub const MAX: Self = Self(MAX_LITERAL_POSITION_BITS);
    /// Default [`LiteralPosBits`] value used by the encoder.
    pub const DEFAULT: Self = Self(DEFAULT_LITERAL_POSITION_BITS);

    /// Creates [`LiteralPosBits`] from a raw `u32`.
    ///
    /// # Arguments
    ///
    /// * `value` - The value to create the [`LiteralPosBits`] from.
    ///
    /// # Returns
    ///
    /// A [`Result`] containing the [`LiteralPosBits`] or an [`Error`](crate::error::Error)
    /// if the value is outside the supported range.
    ///
    /// # Errors
    ///
    /// Returns an error if the value is outside the supported range.
    pub fn new(value: u32) -> Result<Self, Error> {
        if (MIN_LITERAL_POSITION_BITS..=MAX_LITERAL_POSITION_BITS).contains(&value) {
            Ok(Self(value))
        } else {
            Err(invalid_option(
                "lp".into(),
                format!(
                    "expected a value in {MIN_LITERAL_POSITION_BITS}..={MAX_LITERAL_POSITION_BITS}"
                ),
            ))
        }
    }

    /// Returns the raw numeric value expected by the SDK.
    pub fn get(self) -> u32 {
        self.0
    }
}

impl From<LiteralPosBits> for i32 {
    fn from(value: LiteralPosBits) -> Self {
        // SAFETY: literal position bits are always in the range of [0, 4].
        i32::try_from(value.0).unwrap()
    }
}

impl From<LiteralPosBits> for u8 {
    fn from(value: LiteralPosBits) -> Self {
        // SAFETY: literal position bits are always in the range of [0, 4].
        u8::try_from(value.0).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::LiteralPosBits;

    #[test]
    fn validates_supported_range() {
        assert_eq!(LiteralPosBits::MIN.get(), 0);
        assert_eq!(LiteralPosBits::DEFAULT.get(), 0);
        assert_eq!(LiteralPosBits::MAX.get(), 4);
        assert_eq!(LiteralPosBits::new(0).unwrap(), LiteralPosBits::MIN);
        assert_eq!(LiteralPosBits::new(4).unwrap(), LiteralPosBits::MAX);
        assert!(LiteralPosBits::new(5).is_err());
    }
}
