//! Incremental, buffered wrapper for raw `XZ` encoding.
//!
//! This module exposes a safe, ergonomic interface to the low-level XZ encoding routines.

use crate::error::{map_status, Error, Result};
use crate::{drain_pending, initialize_xz_crc_tables, LzmaAction, XzOptions};

/// Slack added to the input length to allocate the output buffer.
const ENCODE_OUTPUT_CAPACITY_SLACK: usize = 1024;

/// Output of an `XZ` encode operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompressedData {
    /// Number of bytes in the original uncompressed payload.
    pub uncompressed_len: usize,
    /// Encoded `XZ` payload bytes.
    pub compressed: Vec<u8>,
}

/// Streaming-style `XZ` encoder backed by the existing one-shot safe wrapper.
#[derive(Debug, Clone)]
pub struct Encoder {
    options: XzOptions,
    buffered_input: Vec<u8>,
    encoded: Option<CompressedData>,
    output_offset: usize,
    total_in: u64,
    total_out: u64,
}

impl Encoder {
    /// Creates a buffered `XZ` encoder.
    ///
    /// # Arguments
    ///
    /// * `options` - The options to use for the encoder.
    ///
    /// # Returns
    ///
    /// A new [`Encoder`] instance.
    pub fn new(options: XzOptions) -> Self {
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
        action: LzmaAction,
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

    /// Returns the full encoded payload once [`LzmaAction::Finish`](crate::LzmaAction::Finish) has been processed.
    pub fn encoded(&self) -> Option<&CompressedData> {
        self.encoded.as_ref()
    }
}

/// Compresses a slice of raw input data with the specified `XZ` compression options.
///
/// Returns the complete compressed data, including the XZ properties header and final output
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
pub(crate) fn compress(input: &[u8], options: &XzOptions) -> Result<CompressedData> {
    initialize_xz_crc_tables();

    let mut output = Vec::with_capacity(input.len().saturating_add(ENCODE_OUTPUT_CAPACITY_SLACK));

    let out_stream = lzma_sdk_sys::VecOutStream::new(&mut output);

    if input.is_empty() {
        // SAFETY: `out_stream` owns a live `Vec<u8>` for the duration of this call.
        let code = unsafe { lzma_sdk_sys::Xz_EncodeEmpty(out_stream.as_ptr()) };

        if code != lzma_sdk_sys::SZ_OK as i32 {
            return Err(map_status(code));
        }

        return Ok(CompressedData {
            uncompressed_len: 0,
            compressed: output,
        });
    }

    let input_stream = lzma_sdk_sys::SliceInStream::new(input);
    let props = options.to_raw_props()?;

    #[cfg(any(feature = "sdk-9-20", feature = "sdk-16-04", feature = "sdk-19-00"))]
    // SAFETY: The stream wrappers and property struct remain live for the duration of the call.
    let code = unsafe {
        lzma_sdk_sys::Xz_Encode(
            out_stream.as_ptr(),
            input_stream.as_ptr(),
            &props,
            0,
            std::ptr::null_mut(),
        )
    };

    #[cfg(any(feature = "sdk-23-01", feature = "sdk-26-00"))]
    // SAFETY: The stream wrappers and property struct remain live for the duration of the call.
    let code = unsafe {
        lzma_sdk_sys::Xz_Encode(
            out_stream.as_ptr(),
            input_stream.as_ptr(),
            &props,
            std::ptr::null(),
        )
    };

    if code != lzma_sdk_sys::SZ_OK as i32 {
        return Err(map_status(code));
    }

    Ok(CompressedData {
        uncompressed_len: input.len(),
        compressed: output,
    })
}

#[cfg(test)]
mod tests {
    use crate::{Error, LzmaAction, XzOptions};

    use super::{compress, Encoder};

    fn sample_payload() -> Vec<u8> {
        b"xz payload".repeat(128)
    }

    #[test]
    fn compresses_empty_input() {
        let compressed = compress(&[], &XzOptions::default()).unwrap();

        assert_eq!(compressed.uncompressed_len, 0);
        assert!(!compressed.compressed.is_empty());
    }

    #[test]
    fn process_buffers_until_finish_and_drains_output() {
        let input = sample_payload();
        let mut encoder = Encoder::new(XzOptions::default());
        let mut output = [0_u8; 48];

        let (consumed, written) = encoder
            .process(&input[..80], &mut output, LzmaAction::Run)
            .unwrap();
        assert_eq!(consumed, 80);
        assert_eq!(written, 0);
        assert_eq!(encoder.total_in(), 80);
        assert_eq!(encoder.total_out(), 0);
        assert!(!encoder.is_finished());

        let (consumed, mut written_total) = encoder
            .process(&input[80..], &mut output, LzmaAction::Finish)
            .unwrap();
        assert_eq!(consumed, input.len() - 80);
        assert!(encoder.encoded().is_some());

        while !encoder.is_finished() {
            let (_, written) = encoder
                .process(&[], &mut output, LzmaAction::Finish)
                .unwrap();
            assert!(written != 0);
            written_total += written;
        }

        let encoded = encoder.encoded().unwrap();
        assert_eq!(written_total, encoded.compressed.len());
        assert_eq!(encoder.total_in(), input.len() as u64);
        assert_eq!(encoder.total_out(), encoded.compressed.len() as u64);
    }

    #[test]
    fn rejects_new_input_after_finishing() {
        let input = sample_payload();
        let mut encoder = Encoder::new(XzOptions::default());
        let mut output = [0_u8; 64];

        encoder
            .process(&input, &mut output, LzmaAction::Finish)
            .unwrap();

        let error = encoder
            .process(b"extra", &mut output, LzmaAction::Finish)
            .unwrap_err();

        assert_eq!(error, Error::Param);
    }
}
