//! `PosBits` option for raw `LZMA` encoding.
//
//! The `PosBits` parameter, also known as `pb`, specifies the number of low bits of the match position to include
//! in the LZMA encoder's modeling of match distances. The choice of this parameter can influence compression
//! effectiveness for data with periodic patterns aligned to particular positions.
//
//! The valid range for position bits (`pb`) is 0 (minimum) to 4 (maximum), and a value of 2 (`pb = 2`) is typical.
//! Higher values may improve compression with certain data patterns but can increase memory usage and marginally affect performance.

use crate::error::{invalid_option, Error};

const MIN_POSITION_BITS: u32 = 0;
const MAX_POSITION_BITS: u32 = 4;
const DEFAULT_POSITION_BITS: u32 = 2;

/// Number of position bits (`pb`) for raw `LZMA`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct PosBits(u32);

impl PosBits {
    /// Minimum [`PosBits`] value supported by the encoder.
    pub const MIN: Self = Self(MIN_POSITION_BITS);
    /// Maximum [`PosBits`] value supported by the encoder.
    pub const MAX: Self = Self(MAX_POSITION_BITS);
    /// Default [`PosBits`] value used by the encoder.
    pub const DEFAULT: Self = Self(DEFAULT_POSITION_BITS);

    /// Creates [`PosBits`] from a raw `u32`.
    ///
    /// # Arguments
    ///
    /// * `value` - The value to create the [`PosBits`] from.
    ///
    /// # Returns
    ///
    /// A [`Result`] containing the [`PosBits`] or an [`Error`](crate::error::Error)
    /// if the value is outside the supported range.
    ///
    /// # Errors
    ///
    /// Returns an error if the value is outside the supported range.
    pub fn new(value: u32) -> Result<Self, Error> {
        if value <= MAX_POSITION_BITS {
            Ok(Self(value))
        } else {
            Err(invalid_option(
                "pb".into(),
                format!("expected a value in {MIN_POSITION_BITS}..={MAX_POSITION_BITS}"),
            ))
        }
    }

    /// Returns the raw numeric value expected by the SDK.
    pub fn get(self) -> u32 {
        self.0
    }
}

impl From<PosBits> for i32 {
    fn from(value: PosBits) -> Self {
        // SAFETY: position bits are always in the range of [0, 4].
        i32::try_from(value.0).unwrap()
    }
}

impl From<PosBits> for u8 {
    fn from(value: PosBits) -> Self {
        // SAFETY: position bits are always in the range of [0, 4].
        u8::try_from(value.0).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::PosBits;

    #[test]
    fn validates_supported_range() {
        assert_eq!(PosBits::MIN.get(), 0);
        assert_eq!(PosBits::DEFAULT.get(), 2);
        assert_eq!(PosBits::MAX.get(), 4);
        assert_eq!(PosBits::new(0).unwrap(), PosBits::MIN);
        assert_eq!(PosBits::new(2).unwrap(), PosBits::DEFAULT);
        assert_eq!(PosBits::new(4).unwrap(), PosBits::MAX);
        assert!(PosBits::new(5).is_err());
    }
}
