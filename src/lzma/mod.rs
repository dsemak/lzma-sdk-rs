//! Safe, idiomatic wrappers for raw `LZMA` streams.
//!
//! This module provides a typed, high-level interface to the low-level LZMA encoder/decoder core,
//! suitable for use in streaming, one-shot, and fine-tuned compression scenarios.

mod decoder;
mod encoder;
mod options;
mod props;

pub use options::Options as LzmaOptions;
pub use options::OptionsBuilder as LzmaOptionsBuilder;
pub use options::{Algorithm, EndMarkerMode, HashBytes, MatchFinderMode};
pub use options::{CompressionLevel, FastBytes, LiteralContextBits, LiteralPosBits, PosBits};

pub use decoder::Decoder as RawDecoder;

pub use encoder::CompressedData as LzmaCompressedData;
pub use encoder::Encoder as RawEncoder;

pub use props::Props as LzmaProps;
pub use props::LZMA_PROPS_SIZE;

pub use Action as LzmaAction;

/// High-level action used by the owned encoder and decoder wrappers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Continue buffering or decoding input without finalizing the stream.
    Run,
    /// Finalize the current stream and flush all remaining output.
    Finish,
}

impl Action {
    pub(crate) fn finish_mode(self) -> FinishMode {
        match self {
            Self::Run => FinishMode::Any,
            Self::Finish => FinishMode::End,
        }
    }

    pub(crate) fn is_finish(self) -> bool {
        matches!(self, Self::Finish)
    }
}

/// Finish mode used by decode operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FinishMode {
    /// Decode as much as possible without requiring the stream to end.
    Any,
    /// Require the current call to finish the stream.
    End,
}

impl FinishMode {
    pub(crate) fn as_raw(self) -> lzma_sdk_sys::ELzmaFinishMode {
        match self {
            Self::Any => lzma_sdk_sys::ELzmaFinishMode_LZMA_FINISH_ANY,
            Self::End => lzma_sdk_sys::ELzmaFinishMode_LZMA_FINISH_END,
        }
    }
}
