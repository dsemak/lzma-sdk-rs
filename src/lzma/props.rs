//! Typed representation of the raw 5-byte `LZMA` properties header.
//!
//! This header is used for encoding and decoding low-level `LZMA` streams.
//! It consists of the literal context bits (lc), literal position bits (lp),
//! position bits (pb), and dictionary size, following the 7z/LZMA SDK layout.

use std::mem::MaybeUninit;

use crate::error::{invalid_option, map_status, Error};
use crate::lzma::options::{LiteralContextBits, LiteralPosBits, PosBits, MIN_DICT_SIZE};

/// Number of bytes in the encoded raw-LZMA properties header.
pub const LZMA_PROPS_SIZE: usize = lzma_sdk_sys::LZMA_PROPS_SIZE as usize;

/// Typed representation of the five-byte raw `LZMA` properties blob.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Props {
    lc: LiteralContextBits,
    lp: LiteralPosBits,
    pb: PosBits,
    dict_size: u32,
}

impl Props {
    /// Creates a typed raw `LZMA` properties value.
    ///
    /// # Arguments
    ///
    /// * `lc` - The literal context bits.
    /// * `lp` - The literal position bits.
    /// * `pb` - The position bits.
    /// * `dict_size` - The dictionary size.
    ///
    /// # Returns
    ///
    /// A [`Result`] containing the [`Props`] or an [`Error`](crate::error::Error)
    /// if the dictionary size is smaller than `MIN_DICT_SIZE`.
    ///
    /// # Errors
    ///
    /// Returns an error if the dictionary size is smaller than `MIN_DICT_SIZE`.
    pub fn new(
        lc: LiteralContextBits,
        lp: LiteralPosBits,
        pb: PosBits,
        dict_size: u32,
    ) -> Result<Self, Error> {
        if dict_size < MIN_DICT_SIZE {
            return Err(invalid_option(
                "dict_size".into(),
                format!("expected at least {} bytes", MIN_DICT_SIZE),
            ));
        }
        Ok(Self {
            lc,
            lp,
            pb,
            dict_size,
        })
    }

    /// Decodes the five-byte raw property block produced by the SDK.
    ///
    /// # Arguments
    ///
    /// * `encoded` - The five-byte raw `LZMA` properties block to decode.
    ///
    /// # Returns
    ///
    /// A [`Result`] containing the decoded properties or an [`Error`](crate::error::Error)
    /// if the encoded properties are not a valid raw `LZMA` properties block.
    ///
    /// # Errors
    ///
    /// Returns an error if the encoded properties are not a valid raw `LZMA` properties block.
    pub fn decode(encoded: &[u8; LZMA_PROPS_SIZE]) -> Result<Self, Error> {
        let mut props = MaybeUninit::<lzma_sdk_sys::CLzmaProps>::zeroed();

        // SAFETY: `props` points to uninitialized storage for `CProps`, and `encoded`
        // is a valid 5-byte property payload required by the SDK.
        let code = unsafe {
            let encoded_len = u32::try_from(encoded.len()).map_err(|_| Error::SizeOverflow)?;
            lzma_sdk_sys::LzmaProps_Decode(props.as_mut_ptr(), encoded.as_ptr(), encoded_len)
        };

        if code != lzma_sdk_sys::SZ_OK as i32 {
            return Err(map_status(code));
        }

        // SAFETY: `LzmaProps_Decode` returned `SZ_OK`, so `props` was fully initialized.
        let props = unsafe { props.assume_init() };
        Self::new(
            LiteralContextBits::new(props.lc as u32)?,
            LiteralPosBits::new(props.lp as u32)?,
            PosBits::new(props.pb as u32)?,
            props.dicSize,
        )
    }

    /// Encodes the typed properties into the five-byte raw SDK representation.
    pub fn encode(self) -> [u8; LZMA_PROPS_SIZE] {
        let first = (u8::from(self.pb) * 5 + u8::from(self.lp)) * 9 + u8::from(self.lc);
        let mut encoded = [0_u8; LZMA_PROPS_SIZE];
        encoded[0] = first;
        encoded[1..].copy_from_slice(&self.dict_size.to_le_bytes());
        encoded
    }

    /// Returns the literal context bits (`lc`) stored in the header.
    pub fn lc(self) -> LiteralContextBits {
        self.lc
    }

    /// Returns the literal position bits (`lp`) stored in the header.
    pub fn lp(self) -> LiteralPosBits {
        self.lp
    }

    /// Returns the position bits (`pb`) stored in the header.
    pub fn pb(self) -> PosBits {
        self.pb
    }

    /// Returns the configured dictionary size in bytes.
    pub fn dict_size(self) -> u32 {
        self.dict_size
    }
}

#[cfg(test)]
mod tests {
    use crate::lzma::options::{LiteralContextBits, LiteralPosBits, PosBits, MIN_DICT_SIZE};

    use super::{Props, LZMA_PROPS_SIZE};

    #[test]
    fn getters_expose_components_used_for_encoding() {
        let props = Props::new(
            LiteralContextBits::new(3).unwrap(),
            LiteralPosBits::new(1).unwrap(),
            PosBits::new(2).unwrap(),
            1 << 20,
        )
        .unwrap();

        assert_eq!(props.lc(), LiteralContextBits::new(3).unwrap());
        assert_eq!(props.lp(), LiteralPosBits::new(1).unwrap());
        assert_eq!(props.pb(), PosBits::new(2).unwrap());
        assert_eq!(props.dict_size(), 1 << 20);
        assert_eq!(props.encode().len(), LZMA_PROPS_SIZE);
    }

    #[test]
    fn encode_and_decode_round_trip() {
        let props = Props::new(
            LiteralContextBits::new(4).unwrap(),
            LiteralPosBits::new(0).unwrap(),
            PosBits::new(2).unwrap(),
            1 << 19,
        )
        .unwrap();

        let encoded = props.encode();
        let decoded = Props::decode(&encoded).unwrap();

        assert_eq!(decoded, props);
    }

    #[test]
    fn rejects_dictionary_sizes_smaller_than_minimum() {
        let result = Props::new(
            LiteralContextBits::DEFAULT,
            LiteralPosBits::DEFAULT,
            PosBits::DEFAULT,
            MIN_DICT_SIZE - 1,
        );

        assert!(result.is_err());
    }

    #[test]
    fn rejects_invalid_encoded_properties() {
        let encoded = [225, 0, 0, 0, 0];

        assert!(Props::decode(&encoded).is_err());
    }
}
