//! Factory object for the new high-level encoder and decoder surface.
//!
//! The type mirrors the entrypoint style used by `lzma-safe`: callers start from an owned
//! `Stream` value and turn it into a format-specific encoder or decoder.

use crate::error::Result;
use crate::{
    AloneDecoder, AloneEncoder, Decoder, Encoder, Lzma2Decoder, Lzma2Encoder, Lzma2Options,
    Lzma2Property, LzmaOptions, LzmaProps, RawDecoder, RawEncoder, XzOptions,
};

/// Owned entrypoint used to construct encoders and decoders.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Stream;

impl Stream {
    /// Creates a new stream factory.
    pub fn new() -> Self {
        Self
    }

    /// Creates a new raw `LZMA` encoder.
    pub fn raw_encoder(self, options: LzmaOptions) -> RawEncoder {
        let _ = self;
        RawEncoder::new(options)
    }

    /// Creates a new `LZMA2` encoder.
    pub fn lzma2_encoder(self, options: Lzma2Options) -> Lzma2Encoder {
        let _ = self;
        Lzma2Encoder::new(options)
    }

    /// Creates a new `XZ` encoder.
    pub fn encoder(self, options: XzOptions) -> Encoder {
        let _ = self;
        Encoder::new(options)
    }

    /// Creates a new raw `LZMA` decoder from typed properties.
    pub fn raw_decoder(self, props: &LzmaProps) -> Result<RawDecoder> {
        let _ = self;
        RawDecoder::new(props)
    }

    /// Creates a new `LZMA2` decoder.
    pub fn lzma2_decoder(self, property: Lzma2Property) -> Result<Lzma2Decoder> {
        let _ = self;
        Lzma2Decoder::new(property)
    }

    /// Creates a new `XZ` decoder.
    pub fn decoder(self) -> Result<Decoder> {
        let _ = self;
        Decoder::new()
    }

    /// Creates a new `.lzma` encoder.
    pub fn alone_encoder(self, options: LzmaOptions) -> AloneEncoder {
        let _ = self;
        AloneEncoder::new(options)
    }

    /// Creates a new `.lzma` decoder.
    pub fn alone_decoder(self) -> Result<AloneDecoder> {
        let _ = self;
        AloneDecoder::new()
    }
}

#[cfg(test)]
mod tests {
    use super::Stream;
    use crate::lzma::{LiteralContextBits, LiteralPosBits, PosBits};
    use crate::{Lzma2Options, Lzma2Property, LzmaOptions, LzmaProps, XzOptions};

    #[test]
    fn constructs_all_wrapper_types() {
        let stream = Stream::new();
        let props = LzmaProps::new(
            LiteralContextBits::DEFAULT,
            LiteralPosBits::DEFAULT,
            PosBits::DEFAULT,
            1 << 20,
        )
        .unwrap();

        let raw_encoder = stream.raw_encoder(LzmaOptions::default());
        let lzma2_encoder = stream.lzma2_encoder(Lzma2Options::default());
        let xz_encoder = stream.encoder(XzOptions::default());
        let raw_decoder = stream.raw_decoder(&props).unwrap();
        let lzma2_decoder = stream.lzma2_decoder(Lzma2Property::new(0)).unwrap();
        let xz_decoder = stream.decoder().unwrap();

        assert_eq!(raw_encoder.total_in(), 0);
        assert_eq!(lzma2_encoder.total_in(), 0);
        assert_eq!(xz_encoder.total_in(), 0);
        assert_eq!(raw_decoder.total_in(), 0);
        assert_eq!(lzma2_decoder.total_in(), 0);
        assert_eq!(xz_decoder.total_in(), 0);
    }
}
