//! Safe, high-level wrappers around the official `LZMA SDK`.
//!
//! The crate exposes two complementary surfaces:
//!
//! - A new owned API built around [`Stream`], [`Encoder`], and [`Decoder`].
//! - Compatibility helpers in [`lzma`], [`lzma2`], and [`xz`] for direct access to the existing
//!   format-specific wrappers.
//!
//! # Highlights
//!
//! - safe wrappers over the vendored SDK FFI
//! - typed raw-`LZMA`, `LZMA2`, and `XZ` option models
//! - owned encoder and decoder types with consistent `process()` methods
//! - compatibility helpers for one-shot memory-oriented workflows
//!
//! # Example
//!
//! ```rust
//! use lzma_sdk::{LzmaAction, Stream, XzOptions};
//!
//! let stream = Stream::new();
//! let mut encoder = stream.encoder(XzOptions::default());
//! let input = b"hello from lzma-sdk";
//! let mut compressed = Vec::new();
//! let mut encoded_chunk = [0_u8; 64];
//!
//! let (_, written) = encoder.process(input, &mut encoded_chunk, LzmaAction::Finish)?;
//! compressed.extend_from_slice(&encoded_chunk[..written]);
//!
//! while !encoder.is_finished() {
//!     let (_, written) = encoder.process(&[], &mut encoded_chunk, LzmaAction::Finish)?;
//!     compressed.extend_from_slice(&encoded_chunk[..written]);
//! }
//!
//! let stream = Stream::new();
//! let mut decoder = stream.decoder()?;
//! let mut restored = Vec::new();
//! let mut decoded_chunk = [0_u8; 64];
//! let mut offset = 0;
//!
//! loop {
//!     let action = if offset == compressed.len() {
//!         LzmaAction::Finish
//!     } else {
//!         LzmaAction::Run
//!     };
//!     let (consumed, written) =
//!         decoder.process(&compressed[offset..], &mut decoded_chunk, action)?;
//!     offset += consumed;
//!     restored.extend_from_slice(&decoded_chunk[..written]);
//!     if decoder.is_finished() {
//!         break;
//!     }
//! }
//!
//! assert_eq!(restored, input);
//! # Ok::<(), lzma_sdk::Error>(())
//! ```
//!
//! # Notes
//!
//! - Raw `LZMA` unknown-size decoding is reliable only for streams with an end marker.
//! - The high-level encoders currently buffer input until [`LzmaAction::Finish`],
//!   because the safe wrapper still builds on one-shot SDK encode primitives.
//! - The vendored SDK is currently built in single-thread mode.
#![deny(unsafe_op_in_unsafe_fn)]

pub mod alone;
pub mod error;
pub mod filter;
pub mod lzma;
pub mod lzma2;
pub mod stream;
pub mod xz;

pub use alone::{Decoder as AloneDecoder, Encoder as AloneEncoder, LZMA_ALONE_HEADER_SIZE};
pub use error::{Error, Result};
pub use filter::{BranchConverter, BranchFilter, BranchMode, DeltaFilter};
pub use lzma::{
    FinishMode, LzmaAction, LzmaOptions, LzmaOptionsBuilder, LzmaProps, RawDecoder, RawEncoder,
    LZMA_PROPS_SIZE,
};
pub use lzma2::{Lzma2Decoder, Lzma2Encoder, Lzma2Options, Lzma2OptionsBuilder, Lzma2Property};
pub use stream::Stream;
pub use xz::{Decoder, Encoder, XzCompressedData, XzOptions, XzOptionsBuilder};

/// Drains pending bytes from the input buffer into the output buffer.
///
/// # Arguments
///
/// - `pending`: The pending bytes to drain.
/// - `offset`: The offset into the pending bytes.
/// - `output`: The output buffer to write the drained bytes to.
///
/// # Returns
///
/// The number of bytes drained from the pending bytes.
pub(crate) fn drain_pending(pending: &[u8], offset: &mut usize, output: &mut [u8]) -> usize {
    let available = pending.len().saturating_sub(*offset);
    let to_write = available.min(output.len());
    if to_write != 0 {
        output[..to_write].copy_from_slice(&pending[*offset..*offset + to_write]);
        *offset += to_write;
    }
    to_write
}

/// Returns a pointer to the SDK's default memory allocator.
///
/// # Safety
///
/// The returned pointer is expected to remain valid
/// for the lifetime of all LZMA SDK operations in the process.
pub(crate) fn alloc() -> *mut lzma_sdk_sys::ISzAlloc {
    lzma_sdk_sys::default_alloc()
}

/// Ensures that the XZ CRC tables are initialized in the SDK.
///
/// Should be called before any calls to checksum or decode using XZ format.
pub(crate) fn initialize_xz_crc_tables() {
    lzma_sdk_sys::initialize_xz_crc_tables()
}

/// Calculates the SDK's CRC32 checksum for a given byte slice.
///
/// The result matches the checksum produced by the underlying LZMA SDK.
///
/// # Example
///
/// ```
/// let sum = lzma_sdk::crc32(b"example");
/// ```
pub fn crc32(data: &[u8]) -> u32 {
    initialize_xz_crc_tables();
    unsafe { lzma_sdk_sys::CrcCalc(data.as_ptr().cast(), data.len()) }
}

/// Suggests an initial output buffer size for a given input length.
///
/// Follows LZMA/XZ output growth heuristics: 33% extra plus a safety pad.
pub(crate) fn initial_output_capacity(input_len: usize) -> usize {
    input_len
        .saturating_add(input_len / 3)
        .saturating_add(1024)
        .max(1024)
}

/// Doubles the provided capacity as a safe way to grow a buffer,
/// returning an error if the operation would overflow.
///
/// Returns [`Error::SizeOverflow`] on overflow.
pub(crate) fn grow_capacity(capacity: usize) -> Result<usize> {
    capacity.checked_mul(2).ok_or(Error::SizeOverflow)
}

/// Returns the version string for the upstream SDK snapshot
/// selected by the active Cargo feature.
///
/// This string is provided by the linked `lzma_sdk_sys` crate and
/// usually corresponds to the SDK's upstream release or commit.
pub fn sdk_version() -> &'static str {
    lzma_sdk_sys::SDK_VERSION
}
