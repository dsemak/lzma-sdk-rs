//! Safe, idiomatic wrappers for raw `XZ` streams.
//!
//! This module provides a typed, high-level interface to the low-level XZ encoder/decoder core,
//! suitable for use in streaming, one-shot, and fine-tuned compression scenarios.

mod decoder;
mod encoder;
mod options;

pub use options::Options as XzOptions;
pub use options::OptionsBuilder as XzOptionsBuilder;
pub use options::{Check, Filter};

pub use decoder::Decoder;

pub use encoder::CompressedData as XzCompressedData;
pub use encoder::Encoder;
