//! Incremental raw `LZMA` decoder wrapper.
//
//! This module provides a low-level, efficient, and incremental decoder for raw LZMA streams.

use std::mem::MaybeUninit;

use crate::alloc;
use crate::error::{map_status, Result};
use crate::{FinishMode, LzmaAction, LzmaProps};

/// Outcome reported by raw `LZMA` decoding.
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
    /// Converts a raw `LZMA` status code to a [`DecodeStatus`].
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

/// Stateful raw `LZMA` decoder for incremental processing.
pub struct Decoder {
    state: lzma_sdk_sys::CLzmaDec,
    allocated: bool,
    finished: bool,
    total_in: u64,
    total_out: u64,
}

impl Decoder {
    /// Creates a new raw `LZMA` decoder for the provided property set.
    ///
    /// # Arguments
    ///
    /// * `props` - The properties to use for the decoding.
    ///
    /// # Returns
    ///
    /// A [`Result`] containing the decoder or an [`Error`](crate::error::Error) if the decoder failed to initialize.
    ///
    /// # Errors
    ///
    /// Returns an error if the SDK cannot allocate or initialize the decoder.
    pub fn new(props: &LzmaProps) -> Result<Self> {
        let encoded = props.encode();
        let mut state = MaybeUninit::<lzma_sdk_sys::CLzmaDec>::zeroed();

        // SAFETY: `state` is valid storage for the decoder state and `encoded` contains the
        // exact 5-byte properties blob expected by `LzmaDec_Allocate`.
        let code = unsafe {
            lzma_sdk_sys::LzmaDec_Allocate(
                state.as_mut_ptr(),
                encoded.as_ptr(),
                encoded.len() as u32,
                alloc(),
            )
        };

        if code != lzma_sdk_sys::SZ_OK as i32 {
            return Err(map_status(code));
        }

        // SAFETY: `LzmaDec_Allocate` returned success, so the decoder state is initialized.
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

    /// Resets the decoder so it can process a new raw `LZMA` stream.
    pub fn reset(&mut self) {
        // SAFETY: `self.state` was allocated by `LzmaDec_Allocate` and remains owned by `self`.
        unsafe {
            lzma_sdk_sys::LzmaDec_Init(&mut self.state);
        }

        self.finished = false;
        self.total_in = 0;
        self.total_out = 0;
    }

    /// Decodes a chunk of raw `LZMA`-encoded input into the provided output buffer.
    ///
    /// Advances the decoder state by processing up to `input.len()` bytes, writing
    /// up to `output.len()` bytes, according to the specified [`LzmaAction`].
    ///
    /// # Arguments
    ///
    /// * `input` - Encoded LZMA input bytes to be consumed.
    /// * `output` - Mutable output buffer for decoded bytes.
    /// * `action` - Whether to continue decoding ([`LzmaAction::Run`](crate::LzmaAction::Run)) or
    ///   finish ([`LzmaAction::Finish`](crate::LzmaAction::Finish)) the stream.
    ///
    /// # Returns
    ///
    /// Returns [`Ok((input_consumed, output_written))`] with the number of input bytes consumed and
    /// output bytes written in this step.
    ///
    /// # Errors
    ///
    /// Returns an error if the decode operation cannot proceed or the stream is corrupted.
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
            // SAFETY: `self.state` was initialized by the SDK and has not been freed yet.
            unsafe {
                lzma_sdk_sys::LzmaDec_Free(&mut self.state, alloc());
            }
        }
    }
}

/// Step result of a raw `LZMA` decode operation.
struct DecodeStep {
    /// Number of bytes of input consumed.
    input_consumed: usize,
    /// Number of bytes of output written.
    output_written: usize,
    /// Status of the last decode operation.
    status: DecodeStatus,
}

/// Performs a single raw LZMA decode operation into the provided output buffer.
///
/// This function advances the decoder state by consuming bytes from the input and producing
/// decoded output. It handles incremental decoding and may be called repeatedly for streaming.
///
/// # Arguments
///
/// * `state` - Mutable reference to the decoder state allocated by the LZMA SDK.
/// * `input` - Slice of input (compressed) bytes to decode from.
/// * `output` - Mutable slice to receive decoded bytes.
/// * `finish_mode` - Indicates the decoding termination behavior (see [`FinishMode`]).
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
    state: &mut lzma_sdk_sys::CLzmaDec,
    input: &[u8],
    output: &mut [u8],
    finish_mode: FinishMode,
) -> Result<DecodeStep> {
    let mut input_consumed = input.len();
    let mut output_written = output.len();
    let mut status = lzma_sdk_sys::ELzmaStatus_LZMA_STATUS_NOT_SPECIFIED;

    // SAFETY: Decoder state is initialized, buffers are valid for the passed lengths, and
    // `status` is a valid out-parameter until the call returns.
    let code = unsafe {
        lzma_sdk_sys::LzmaDec_DecodeToBuf(
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
