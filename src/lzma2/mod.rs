//! Safe, idiomatic wrappers for raw `LZMA2` streams.
//!
//! This module provides a typed, high-level interface to the low-level LZMA2 encoder/decoder core,
//! suitable for use in streaming, one-shot, and fine-tuned compression scenarios.

mod decoder;
mod encoder;
mod options;

pub use options::Options as Lzma2Options;
pub use options::OptionsBuilder as Lzma2OptionsBuilder;

pub use encoder::CompressedData as Lzma2CompressedData;
pub use encoder::Encoder as Lzma2Encoder;

pub use decoder::Decoder as Lzma2Decoder;

pub use Property as Lzma2Property;

/// Encoded one-byte `LZMA2` property value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Property(u8);

impl Property {
    /// Creates a property wrapper from the raw SDK byte.
    pub fn new(raw: u8) -> Self {
        Self(raw)
    }

    /// Returns the raw SDK property byte.
    pub fn raw(self) -> u8 {
        self.0
    }
}
