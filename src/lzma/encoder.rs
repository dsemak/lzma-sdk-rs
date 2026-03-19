//! Incremental, buffered wrapper for raw `LZMA` encoding.
//!
//! This module exposes a safe, ergonomic interface to the low-level LZMA encoding routines.

use std::ptr;

use crate::error::{map_status, Error, Result};
use crate::{alloc, drain_pending, grow_capacity, initial_output_capacity};

use super::{Action, LzmaOptions, LzmaProps, LZMA_PROPS_SIZE};

/// Output of a raw `LZMA` encode operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompressedData {
    /// Five-byte raw `LZMA` properties header required for decompression.
    pub props: [u8; super::LZMA_PROPS_SIZE],
    /// Number of bytes in the original uncompressed payload.
    pub uncompressed_len: usize,
    /// Encoded raw `LZMA` payload bytes.
    pub compressed: Vec<u8>,
}

impl CompressedData {
    /// Decodes the typed property view from the stored five-byte props header.
    ///
    /// Returns an error if `self.props` does not contain a valid raw `LZMA` property block.
    pub fn properties(&self) -> Result<LzmaProps> {
        LzmaProps::decode(&self.props)
    }
}

/// Buffered raw `LZMA` encoder that emits output after [`Action::Finish`].
pub struct Encoder {
    options: LzmaOptions,
    buffered_input: Vec<u8>,
    encoded: Option<CompressedData>,
    output_offset: usize,
    total_in: u64,
    total_out: u64,
}

impl Encoder {
    /// Creates a buffered raw `LZMA` encoder.
    ///
    /// # Arguments
    ///
    /// * `options` - The options to use for the encoder.
    ///
    /// # Returns
    ///
    /// A new [`Encoder`] instance.
    pub fn new(options: LzmaOptions) -> Self {
        Self {
            options,
            buffered_input: Vec::new(),
            encoded: None,
            output_offset: 0,
            total_in: 0,
            total_out: 0,
        }
    }

    /// Processes the next chunk of input and writes encoded bytes into `output`.
    ///
    /// The stream is only encoded when [`LzmaAction::Finish`](crate::LzmaAction::Finish) is passed. On that call,
    /// the buffered input is compressed, and all further calls drain the resulting
    /// encoded data into `output`. Any attempt to add more input after encoding
    /// has begun will result in an error.
    ///
    /// This function is designed for chunked input and output: call repeatedly with
    /// input parts and [`LzmaAction::Run`](crate::LzmaAction::Run),
    /// then with [`LzmaAction::Finish`](crate::LzmaAction::Finish) when done.
    /// Continue calling until all output bytes have been emitted.
    ///
    /// # Arguments
    ///
    /// * `input` - The next input buffer chunk.
    /// * `output` - Output buffer to receive encoded bytes.
    /// * `action` - Whether to continue or finalize encoding.
    ///
    /// # Returns
    ///
    /// Returns a [`Result`] of the number of input bytes consumed and output bytes written, or
    /// an [`Error`](crate::error::Error) if encoding fails.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Param`](crate::Error::Param) if input is supplied after encoding is finalized,
    /// or a lower-level error if encoding failed.
    pub fn process(
        &mut self,
        input: &[u8],
        output: &mut [u8],
        action: Action,
    ) -> Result<(usize, usize)> {
        if self.encoded.is_some() && !input.is_empty() {
            return Err(Error::Param);
        }

        self.buffered_input.extend_from_slice(input);
        self.total_in += input.len() as u64;

        if action.is_finish() && self.encoded.is_none() {
            self.encoded = Some(compress(&self.buffered_input, &self.options)?);
        }

        let bytes_written = match &self.encoded {
            Some(encoded) => {
                let written = drain_pending(&encoded.compressed, &mut self.output_offset, output);
                self.total_out += written as u64;
                written
            }
            None => 0,
        };

        Ok((input.len(), bytes_written))
    }

    /// Returns `true` once the encoded stream has been fully drained.
    pub fn is_finished(&self) -> bool {
        self.encoded
            .as_ref()
            .is_some_and(|encoded| self.output_offset == encoded.compressed.len())
    }

    /// Returns the total number of source bytes accepted by the encoder.
    pub fn total_in(&self) -> u64 {
        self.total_in
    }

    /// Returns the total number of encoded bytes written through [`Self::process`].
    pub fn total_out(&self) -> u64 {
        self.total_out
    }

    /// Returns the five-byte raw-LZMA properties header after finishing the stream.
    pub fn properties(&self) -> Option<[u8; LZMA_PROPS_SIZE]> {
        self.encoded.as_ref().map(|encoded| encoded.props)
    }

    /// Returns the encoded payload metadata once [`LzmaAction::Finish`](crate::LzmaAction::Finish) has been processed.
    pub fn encoded(&self) -> Option<&CompressedData> {
        self.encoded.as_ref()
    }
}

/// Compresses a slice of raw input data with the specified `LZMA` compression options.
///
/// Returns the complete compressed data, including the LZMA properties header and final output
/// buffer, or an error if compression fails.
///
/// # Arguments
///
/// * `input` - The raw data to be compressed.
/// * `options` - Compression settings to use.
///
/// # Errors
///
/// Returns an [`Error`](crate::error::Error) if the underlying encoder fails or memory allocation fails.
///
/// # Returns
///
/// [`CompressedData`] containing the properties, the encoded output, and uncompressed input length.
fn compress(input: &[u8], options: &LzmaOptions) -> Result<CompressedData> {
    let raw_props = options.to_raw_encoder_props();
    let mut encoded_props = [0_u8; LZMA_PROPS_SIZE];
    let mut props_len = encoded_props.len();
    let mut capacity = initial_output_capacity(input.len());

    loop {
        let mut output = vec![0_u8; capacity];
        let mut output_len = output.len();

        // SAFETY: All pointers are derived from live Rust slices/vectors, output capacity is
        // tracked by `output_len`, and allocator pointers come from the SDK's global allocator.
        let code = unsafe {
            lzma_sdk_sys::LzmaEncode(
                output.as_mut_ptr(),
                &mut output_len,
                input.as_ptr(),
                input.len(),
                &raw_props,
                encoded_props.as_mut_ptr(),
                &mut props_len,
                options.end_marker() as i32,
                ptr::null_mut(),
                alloc(),
                alloc(),
            )
        };

        if code == lzma_sdk_sys::SZ_OK as i32 {
            output.truncate(output_len);
            return Ok(CompressedData {
                props: encoded_props,
                uncompressed_len: input.len(),
                compressed: output,
            });
        }

        if code == lzma_sdk_sys::SZ_ERROR_OUTPUT_EOF as i32 {
            capacity = grow_capacity(capacity)?;
            continue;
        }

        return Err(map_status(code));
    }
}
