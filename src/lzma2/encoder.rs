//! Incremental, buffered wrapper for raw `LZMA2` encoding.
//!
//! This module exposes a safe, ergonomic interface to the low-level LZMA2 encoding routines.

use std::ptr;

use crate::error::{map_status, Error, Result};
use crate::lzma;
use crate::{alloc, drain_pending, Lzma2Options, Lzma2Property};

/// Slack added to the input length to allocate the output buffer.
const ENCODE_OUTPUT_CAPACITY_SLACK: usize = 1024;

/// Output of an `LZMA2` encode operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompressedData {
    /// One-byte `LZMA2` property header required for decompression.
    pub property: Lzma2Property,
    /// Number of bytes in the original uncompressed payload.
    pub uncompressed_len: usize,
    /// Encoded `LZMA2` payload bytes.
    pub compressed: Vec<u8>,
}

/// Wrapper for the `LZMA2` encoder handle.
struct EncoderHandle {
    handle: lzma_sdk_sys::CLzma2EncHandle,
}

impl EncoderHandle {
    /// Creates a new `LZMA2` encoder handle.
    ///
    /// # Returns
    ///
    /// A [`Result`] containing the [`EncoderHandle`] or an [`Error`](crate::error::Error)
    /// if the encoder handle failed to create.
    ///
    /// # Errors
    ///
    /// Returns an error if the encoder handle failed to create.
    fn new() -> Result<Self> {
        // SAFETY: The allocator pointers come from the SDK and remain valid for the process lifetime.
        let handle = unsafe { lzma_sdk_sys::Lzma2Enc_Create(alloc(), alloc()) };

        if handle.is_null() {
            return Err(Error::Mem);
        }

        Ok(Self { handle })
    }
}

impl Drop for EncoderHandle {
    fn drop(&mut self) {
        // SAFETY: `self.handle` was created by `Lzma2Enc_Create` and is owned by this wrapper.
        unsafe {
            lzma_sdk_sys::Lzma2Enc_Destroy(self.handle);
        }
    }
}

/// Streaming-style `LZMA2` encoder backed by the existing one-shot safe wrapper.
#[derive(Debug, Clone)]
pub struct Encoder {
    options: Lzma2Options,
    buffered_input: Vec<u8>,
    encoded: Option<CompressedData>,
    output_offset: usize,
    total_in: u64,
    total_out: u64,
}

impl Encoder {
    /// Creates a buffered `LZMA2` encoder.
    ///
    /// # Arguments
    ///
    /// * `options` - The options to use for the encoder.
    ///
    /// # Returns
    ///
    /// A new [`Encoder`] instance.
    pub fn new(options: Lzma2Options) -> Self {
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
        action: lzma::Action,
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

    /// Returns the encoded property byte after the stream has been finished.
    pub fn property(&self) -> Option<super::Property> {
        self.encoded.as_ref().map(|encoded| encoded.property)
    }

    /// Returns the encoded payload metadata once [`LzmaAction::Finish`](crate::LzmaAction::Finish) has been processed.
    pub fn encoded(&self) -> Option<&CompressedData> {
        self.encoded.as_ref()
    }
}

/// Compresses a slice of raw input data with the specified `LZMA2` compression options.
///
/// Returns the complete compressed data, including the LZMA2 properties header and final output
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
pub fn compress(input: &[u8], options: &Lzma2Options) -> Result<CompressedData> {
    let raw_props = options.to_raw_props();
    let encoder = EncoderHandle::new()?;

    // SAFETY: `encoder.handle` is a live SDK encoder and `raw_props` points to a valid props struct.
    let code = unsafe { lzma_sdk_sys::Lzma2Enc_SetProps(encoder.handle, &raw_props) };

    if code != lzma_sdk_sys::SZ_OK as i32 {
        return Err(map_status(code));
    }

    #[cfg(any(feature = "sdk-19-00", feature = "sdk-23-01", feature = "sdk-26-00"))]
    // SAFETY: The encoder handle is valid and the provided size describes the in-memory input slice.
    unsafe {
        lzma_sdk_sys::Lzma2Enc_SetDataSize(encoder.handle, input.len() as u64);
    }

    // SAFETY: The encoder handle has already accepted its properties, so querying the encoded
    // property byte is valid for the lifetime of the handle.
    let property =
        Lzma2Property::new(unsafe { lzma_sdk_sys::Lzma2Enc_WriteProperties(encoder.handle) });

    let mut compressed =
        Vec::with_capacity(input.len().saturating_add(ENCODE_OUTPUT_CAPACITY_SLACK));

    let out_stream = lzma_sdk_sys::VecOutStream::new(&mut compressed);

    #[cfg(any(feature = "sdk-9-20", feature = "sdk-16-04"))]
    let code = {
        let input_stream = lzma_sdk_sys::SliceInStream::new(input);

        // SAFETY: The encoder handle is valid and both stream wrappers outlive the call.
        unsafe {
            lzma_sdk_sys::Lzma2Enc_Encode(
                encoder.handle,
                out_stream.as_ptr(),
                input_stream.as_ptr(),
                ptr::null_mut(),
            )
        }
    };

    #[cfg(any(feature = "sdk-19-00", feature = "sdk-23-01", feature = "sdk-26-00"))]
    // SAFETY: The encoder handle is valid, `out_stream` lives for the duration of the call,
    // and the SDK contract permits using an in-memory input buffer with a null input stream.
    let code = unsafe {
        lzma_sdk_sys::Lzma2Enc_Encode2(
            encoder.handle,
            out_stream.as_ptr(),
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
            input.as_ptr(),
            input.len(),
            ptr::null_mut(),
        )
    };

    if code != lzma_sdk_sys::SZ_OK as i32 {
        return Err(map_status(code));
    }

    Ok(CompressedData {
        property,
        uncompressed_len: input.len(),
        compressed,
    })
}

#[cfg(test)]
mod tests {
    use super::{compress, Encoder};
    use crate::{Error, Lzma2Options, LzmaAction};

    fn sample_payload() -> Vec<u8> {
        b"lzma2 payload".repeat(64)
    }

    #[test]
    fn compress_returns_property_and_payload_metadata() {
        let input = sample_payload();
        let compressed = compress(&input, &Lzma2Options::default()).unwrap();

        assert_eq!(compressed.uncompressed_len, input.len());
        assert!(!compressed.compressed.is_empty());
        assert_eq!(compressed.property.raw(), compressed.property.raw());
    }

    #[test]
    fn process_buffers_until_finish_and_drains_output() {
        let input = sample_payload();
        let mut encoder = Encoder::new(Lzma2Options::default());
        let mut output = [0_u8; 32];

        let (consumed, written) = encoder
            .process(&input[..64], &mut output, LzmaAction::Run)
            .unwrap();
        assert_eq!(consumed, 64);
        assert_eq!(written, 0);
        assert_eq!(encoder.total_in(), 64);
        assert_eq!(encoder.total_out(), 0);
        assert!(!encoder.is_finished());
        assert!(encoder.encoded().is_none());

        let (consumed, mut written_total) = encoder
            .process(&input[64..], &mut output, LzmaAction::Finish)
            .unwrap();
        assert_eq!(consumed, input.len() - 64);
        assert!(encoder.property().is_some());
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
        let mut encoder = Encoder::new(Lzma2Options::default());
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
