//! `LiteralContextBits` option for raw `LZMA` encoding.
//
//! The `LiteralContextBits` parameter, also known as `lc`, controls
//! the number of high bits of the previous literal (byte) used to
//! select a context for literal byte encoding in LZMA. This option
//! influences how effectively the encoder can exploit redundancy in
//! repeated byte patterns, which can be important for certain types of input.
//
//! The valid range for `LiteralContextBits` is 0 (minimum) to 8 (maximum).
//! A value of 3 (`lc = 3`) is the default and generally provides balanced
//! performance for most data.

use crate::error::{invalid_option, Error};

const MIN_LITERAL_CONTEXT_BITS: u32 = 0;
const MAX_LITERAL_CONTEXT_BITS: u32 = 8;
const DEFAULT_LITERAL_CONTEXT_BITS: u32 = 3;

/// Number of literal context bits (`lc`) for raw `LZMA`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct LiteralContextBits(u32);

impl LiteralContextBits {
    /// Minimum [`LiteralContextBits`] value supported by the encoder.
    pub const MIN: Self = Self(MIN_LITERAL_CONTEXT_BITS);
    /// Maximum [`LiteralContextBits`] value supported by the encoder.
    pub const MAX: Self = Self(MAX_LITERAL_CONTEXT_BITS);
    /// Default [`LiteralContextBits`] value used by the encoder.
    pub const DEFAULT: Self = Self(DEFAULT_LITERAL_CONTEXT_BITS);

    /// Creates [`LiteralContextBits`] from a raw `u32`.
    ///
    /// # Arguments
    ///
    /// * `value` - The value to create the [`LiteralContextBits`] from.
    ///
    /// # Returns
    ///
    /// A [`Result`] containing the [`LiteralContextBits`] or an [`Error`](crate::error::Error)
    /// if the value is outside the supported range.
    ///
    /// # Errors
    ///
    /// Returns an error if the value is outside the supported range.
    pub fn new(value: u32) -> Result<Self, Error> {
        if (MIN_LITERAL_CONTEXT_BITS..=MAX_LITERAL_CONTEXT_BITS).contains(&value) {
            Ok(Self(value))
        } else {
            Err(invalid_option(
                "lc".into(),
                format!(
                    "expected a value in {MIN_LITERAL_CONTEXT_BITS}..={MAX_LITERAL_CONTEXT_BITS}"
                ),
            ))
        }
    }

    /// Returns the raw numeric value expected by the SDK.
    pub fn get(self) -> u32 {
        self.0
    }
}

impl From<LiteralContextBits> for i32 {
    fn from(value: LiteralContextBits) -> Self {
        // SAFETY: literal context bits are always in the range of [0, 8].
        i32::try_from(value.0).unwrap()
    }
}

impl From<LiteralContextBits> for u8 {
    fn from(value: LiteralContextBits) -> Self {
        // SAFETY: literal context bits are always in the range of [0, 8].
        u8::try_from(value.0).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::LiteralContextBits;

    #[test]
    fn validates_supported_range() {
        assert_eq!(LiteralContextBits::MIN.get(), 0);
        assert_eq!(LiteralContextBits::DEFAULT.get(), 3);
        assert_eq!(LiteralContextBits::MAX.get(), 8);
        assert_eq!(LiteralContextBits::new(0).unwrap(), LiteralContextBits::MIN);
        assert_eq!(
            LiteralContextBits::new(3).unwrap(),
            LiteralContextBits::DEFAULT
        );
        assert_eq!(LiteralContextBits::new(8).unwrap(), LiteralContextBits::MAX);
        assert!(LiteralContextBits::new(9).is_err());
    }
}
