//! Incremental raw `LZMA2` decoder wrapper.
//
//! This module provides a low-level, efficient, and incremental decoder for raw LZMA2 streams.

use std::mem::MaybeUninit;

use crate::alloc;
use crate::error::{map_status, Result};
use crate::{FinishMode, Lzma2Property, LzmaAction};

/// Outcome reported by `LZMA2` decoding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DecodeStatus {
    /// The SDK did not provide a more specific status value.
    NotSpecified,
    /// The stream finished and consumed an explicit end marker.
    FinishedWithMark,
    /// The stream is still in progress.
    NotFinished,
    /// More input is required to continue decoding.
    NeedsMoreInput,
    /// The stream may have finished even though no explicit marker was seen.
    MaybeFinishedWithoutMark,
}

impl DecodeStatus {
    /// Converts a raw `LZMA2` status code to a [`DecodeStatus`].
    fn from_raw(status: lzma_sdk_sys::ELzmaStatus) -> Self {
        match status {
            value if value == lzma_sdk_sys::ELzmaStatus_LZMA_STATUS_FINISHED_WITH_MARK => {
                Self::FinishedWithMark
            }
            value if value == lzma_sdk_sys::ELzmaStatus_LZMA_STATUS_NOT_FINISHED => {
                Self::NotFinished
            }
            value if value == lzma_sdk_sys::ELzmaStatus_LZMA_STATUS_NEEDS_MORE_INPUT => {
                Self::NeedsMoreInput
            }
            value if value == lzma_sdk_sys::ELzmaStatus_LZMA_STATUS_MAYBE_FINISHED_WITHOUT_MARK => {
                Self::MaybeFinishedWithoutMark
            }
            _ => Self::NotSpecified,
        }
    }

    /// Returns `true` when the status represents a completed stream.
    fn is_finished(self) -> bool {
        matches!(
            self,
            Self::FinishedWithMark | Self::MaybeFinishedWithoutMark
        )
    }
}

/// Stateful `LZMA2` decoder for incremental processing.
pub struct Decoder {
    state: lzma_sdk_sys::CLzma2Dec,
    allocated: bool,
    finished: bool,
    total_in: u64,
    total_out: u64,
}

impl Decoder {
    /// Creates a new `LZMA2` decoder for the provided property byte.
    ///
    /// # Arguments
    ///
    /// * `property` - The property byte to use for the decoding.
    ///
    /// # Returns
    ///
    /// A [`Result`] containing the [`Decoder`] or an [`Error`](crate::error::Error)
    /// if the decoder failed to initialize.
    ///
    /// # Errors
    ///
    /// Returns an error if the decoder failed to initialize.
    pub fn new(property: Lzma2Property) -> Result<Self> {
        let mut state = MaybeUninit::<lzma_sdk_sys::CLzma2Dec>::zeroed();

        // SAFETY: `state` is valid storage for the decoder state and the property byte is passed
        // exactly as required by the SDK.
        let code =
            unsafe { lzma_sdk_sys::Lzma2Dec_Allocate(state.as_mut_ptr(), property.raw(), alloc()) };

        if code != lzma_sdk_sys::SZ_OK as i32 {
            return Err(map_status(code));
        }

        // SAFETY: `Lzma2Dec_Allocate` returned success and initialized the decoder state.
        let state = unsafe { state.assume_init() };

        let mut decoder = Self {
            state,
            allocated: true,
            finished: false,
            total_in: 0,
            total_out: 0,
        };

        decoder.reset();

        Ok(decoder)
    }

    /// Resets the decoder so it can process a new `LZMA2` stream.
    pub fn reset(&mut self) {
        // SAFETY: `self.state` is an initialized decoder allocated by the SDK.
        unsafe {
            lzma_sdk_sys::Lzma2Dec_Init(&mut self.state);
        }

        self.finished = false;
        self.total_in = 0;
        self.total_out = 0;
    }

    /// Decodes a chunk of compressed `LZMA2` input data into the provided output buffer.
    ///
    /// Call this method repeatedly to incrementally decode streams; it supports chunked input and output.
    ///
    /// # Arguments
    ///
    /// * `input` - The next slice of compressed bytes to decode.
    /// * `output` - Mutable buffer to receive decompressed bytes.
    /// * `action` - Signals whether to continue or finish decoding (see [`LzmaAction`]).
    ///
    /// # Returns
    ///
    /// Returns a [`Result`] with the number of input bytes consumed and output bytes written,
    /// or an [`Error`](crate::error::Error) if decoding fails.
    ///
    /// # Errors
    ///
    /// Returns an error if the underlying SDK reports a failure during decoding.
    pub fn process(
        &mut self,
        input: &[u8],
        output: &mut [u8],
        action: LzmaAction,
    ) -> Result<(usize, usize)> {
        let step = decode(&mut self.state, input, output, action.finish_mode())?;
        self.finished = step.status.is_finished();
        self.total_in += step.input_consumed as u64;
        self.total_out += step.output_written as u64;
        Ok((step.input_consumed, step.output_written))
    }

    /// Returns `true` once the decoder has finished the current stream.
    pub fn is_finished(&self) -> bool {
        self.finished
    }

    /// Returns the total number of compressed bytes consumed so far.
    pub fn total_in(&self) -> u64 {
        self.total_in
    }

    /// Returns the total number of decoded bytes produced so far.
    pub fn total_out(&self) -> u64 {
        self.total_out
    }
}

impl Drop for Decoder {
    fn drop(&mut self) {
        if self.allocated {
            // SAFETY: `self.state.decoder` was allocated by the SDK and is freed exactly once here.
            unsafe {
                lzma_sdk_sys::LzmaDec_Free(&mut self.state.decoder, alloc());
            }
        }
    }
}

/// Step result of a raw `LZMA2` decode operation.
struct DecodeStep {
    /// Number of bytes of input consumed.
    input_consumed: usize,
    /// Number of bytes of output written.
    output_written: usize,
    /// Status of the last decode operation.
    status: DecodeStatus,
}

/// Performs a single raw `LZMA2` decode operation into the provided output buffer.
///
/// This function advances the decoder state by consuming bytes from the input and producing
/// decoded output. It handles incremental decoding and may be called repeatedly for streaming.
///
/// # Arguments
///
/// - `state`: Mutable reference to the decoder state allocated by the LZMA SDK.
/// - `input`: Slice of input (compressed) bytes to decode from.
/// - `output`: Mutable slice to receive decoded bytes.
/// - `finish_mode`: Indicates the decoding termination behavior (see [`FinishMode`]).
///
/// # Returns
///
/// Returns a [`Result`] containing a [`DecodeStep`] with details on how many bytes
/// were consumed from the input and written to the output, as well as the operation status.
/// Returns an [`Error`](crate::error::Error) if the decode operation fails.
///
/// # Errors
///
/// Returns an error if the underlying SDK returns a non-success status code, or if any other
/// issue occurs during the decode operation.
fn decode(
    state: &mut lzma_sdk_sys::CLzma2Dec,
    input: &[u8],
    output: &mut [u8],
    finish_mode: FinishMode,
) -> Result<DecodeStep> {
    let mut input_consumed = input.len();
    let mut output_written = output.len();
    let mut status = lzma_sdk_sys::ELzmaStatus_LZMA_STATUS_NOT_SPECIFIED;

    // SAFETY: Decoder state is initialized, input/output buffers are valid for the passed
    // lengths, and `status` is a valid out-parameter for this call.
    let code = unsafe {
        lzma_sdk_sys::Lzma2Dec_DecodeToBuf(
            state,
            output.as_mut_ptr(),
            &mut output_written,
            input.as_ptr(),
            &mut input_consumed,
            finish_mode.as_raw(),
            &mut status,
        )
    };

    if code != lzma_sdk_sys::SZ_OK as i32 {
        return Err(map_status(code));
    }

    Ok(DecodeStep {
        input_consumed,
        output_written,
        status: DecodeStatus::from_raw(status),
    })
}

#[cfg(test)]
mod tests {
    use super::Decoder;
    use crate::lzma2::encoder::compress;
    use crate::{Lzma2Options, LzmaAction};

    fn sample_payload() -> Vec<u8> {
        b"incremental lzma2 decode".repeat(64)
    }

    fn decode_all(decoder: &mut Decoder, compressed: &[u8], output_chunk_len: usize) -> Vec<u8> {
        let mut restored = Vec::new();
        let mut offset = 0;
        let mut output = vec![0_u8; output_chunk_len];

        while !decoder.is_finished() {
            let action = if offset == compressed.len() {
                LzmaAction::Finish
            } else {
                LzmaAction::Run
            };
            let (consumed, written) = decoder
                .process(&compressed[offset..], &mut output, action)
                .unwrap();
            assert!(
                decoder.is_finished() || consumed != 0 || written != 0,
                "decoder made no progress"
            );
            offset += consumed;
            restored.extend_from_slice(&output[..written]);
        }

        restored
    }

    #[test]
    fn process_round_trips_incrementally_and_tracks_totals() {
        let input = sample_payload();
        let compressed = compress(&input, &Lzma2Options::default()).unwrap();
        let mut decoder = Decoder::new(compressed.property).unwrap();

        let restored = decode_all(&mut decoder, &compressed.compressed, 37);

        assert_eq!(restored, input);
        assert!(decoder.is_finished());
        assert_eq!(decoder.total_in(), compressed.compressed.len() as u64);
        assert_eq!(decoder.total_out(), input.len() as u64);
    }

    #[test]
    fn reset_clears_state_for_a_new_stream() {
        let input = sample_payload();
        let compressed = compress(&input, &Lzma2Options::default()).unwrap();
        let mut decoder = Decoder::new(compressed.property).unwrap();

        let restored = decode_all(&mut decoder, &compressed.compressed, 41);
        assert_eq!(restored, input);

        decoder.reset();
        assert!(!decoder.is_finished());
        assert_eq!(decoder.total_in(), 0);
        assert_eq!(decoder.total_out(), 0);

        let restored = decode_all(&mut decoder, &compressed.compressed, 29);
        assert_eq!(restored, input);
    }
}
