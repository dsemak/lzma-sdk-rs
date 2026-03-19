//! Incremental raw `LZMA2` decoder wrapper.
//
//! This module provides a low-level, efficient, and incremental decoder for raw XZ streams.

use std::mem::MaybeUninit;

use crate::alloc;
use crate::error::{map_status, Result};
use crate::{FinishMode, LzmaAction};

/// Outcome reported by the `XZ` decoder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodeStatus {
    /// The SDK did not provide a more specific status value.
    NotSpecified,
    /// The stream finished with an end marker.
    FinishedWithMark,
    /// The stream is still in progress.
    NotFinished,
    /// More input is required to continue decoding.
    NeedsMoreInput,
}

impl DecodeStatus {
    fn from_raw(status: lzma_sdk_sys::ECoderStatus) -> Self {
        match status {
            value if value == lzma_sdk_sys::ECoderStatus_CODER_STATUS_FINISHED_WITH_MARK => {
                Self::FinishedWithMark
            }
            value if value == lzma_sdk_sys::ECoderStatus_CODER_STATUS_NOT_FINISHED => {
                Self::NotFinished
            }
            value if value == lzma_sdk_sys::ECoderStatus_CODER_STATUS_NEEDS_MORE_INPUT => {
                Self::NeedsMoreInput
            }
            _ => Self::NotSpecified,
        }
    }
}

/// Stateful `XZ` decoder for incremental processing.
pub struct Decoder {
    state: lzma_sdk_sys::CXzUnpacker,
    initialized: bool,
    finished: bool,
    total_in: u64,
    total_out: u64,
}

impl Decoder {
    /// Creates a new `XZ` decoder.
    ///
    /// # Returns
    ///
    /// A new [`Decoder`] instance.
    ///
    /// # Errors
    ///
    /// Returns an [`Error`](crate::error::Error) if the decoder failed to initialize.
    pub fn new() -> Result<Self> {
        let mut state = MaybeUninit::<lzma_sdk_sys::CXzUnpacker>::zeroed();

        init_unpacker(state.as_mut_ptr())?;

        let state = unsafe { state.assume_init() };

        let mut decoder = Self {
            state,
            initialized: true,
            finished: false,
            total_in: 0,
            total_out: 0,
        };

        decoder.reset();

        Ok(decoder)
    }

    /// Resets the decoder so it can process a new `XZ` stream.
    pub fn reset(&mut self) {
        reset_unpacker(&mut self.state);

        self.finished = false;
        self.total_in = 0;
        self.total_out = 0;
    }

    /// Decodes a chunk of raw `XZ`-encoded input into the provided output buffer.
    ///
    /// Advances the decoder state by processing up to `input.len()` bytes, writing
    /// up to `output.len()` bytes, according to the specified [`LzmaAction`].
    ///
    /// # Arguments
    ///
    /// * `input` - Encoded XZ input bytes to be consumed.
    /// * `output` - Mutable output buffer for decoded bytes.
    /// * `action` - Whether to continue decoding ([`LzmaAction::Run`](crate::LzmaAction::Run)) or
    ///   finish ([`LzmaAction::Finish`](crate::LzmaAction::Finish)).
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
        let step = decode(
            &mut self.state,
            input,
            output,
            action.is_finish(),
            action.finish_mode(),
        )?;

        self.finished = step.stream_finished;
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
        if self.initialized {
            // SAFETY: `self.state` was initialized by the SDK and is freed exactly once here.
            unsafe {
                lzma_sdk_sys::XzUnpacker_Free(&mut self.state);
            }
        }
    }
}

/// Step result of a raw `XZ` decode operation.
struct DecodeStep {
    /// Number of bytes of input consumed.
    input_consumed: usize,
    /// Number of bytes of output written.
    output_written: usize,
    /// Whether the stream has finished.
    stream_finished: bool,
}

/// Performs a single raw `XZ` decode operation into the provided output buffer.
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
    state: &mut lzma_sdk_sys::CXzUnpacker,
    input: &[u8],
    output: &mut [u8],
    src_finished: bool,
    finish_mode: FinishMode,
) -> Result<DecodeStep> {
    let mut input_consumed = input.len();
    let mut output_written = output.len();
    let mut status = lzma_sdk_sys::ECoderStatus_CODER_STATUS_NOT_SPECIFIED;

    let code = decode_impl(
        state,
        output.as_mut_ptr(),
        &mut output_written,
        input.as_ptr(),
        &mut input_consumed,
        src_finished,
        finish_mode,
        &mut status,
    );

    if code != lzma_sdk_sys::SZ_OK as i32 {
        return Err(map_status(code));
    }

    Ok(DecodeStep {
        input_consumed,
        output_written,
        stream_finished: stream_finished(state)
            || matches!(
                DecodeStatus::from_raw(status),
                DecodeStatus::FinishedWithMark
            ),
    })
}

/// Initializes the `XZ` unpacker state.
#[cfg(any(feature = "sdk-23-01", feature = "sdk-26-00"))]
fn init_unpacker(state: *mut lzma_sdk_sys::CXzUnpacker) -> Result<()> {
    // SAFETY: `state` points to valid storage for the unpacker and the allocator remains valid.
    unsafe {
        lzma_sdk_sys::XzUnpacker_Construct(state, alloc());
        lzma_sdk_sys::XzUnpacker_Init(state);
    }
    Ok(())
}

/// Initializes the legacy `XZ` unpacker state.
#[cfg(any(feature = "sdk-9-20", feature = "sdk-16-04", feature = "sdk-19-00"))]
fn init_unpacker(state: *mut lzma_sdk_sys::CXzUnpacker) -> Result<()> {
    // SAFETY: `state` points to valid storage for the legacy unpacker create routine.
    let code = unsafe { lzma_sdk_sys::XzUnpacker_Create(state, alloc()) };

    if code != lzma_sdk_sys::SZ_OK as i32 {
        return Err(map_status(code));
    }
    Ok(())
}

/// Resets the `XZ` unpacker state.
#[cfg(any(feature = "sdk-23-01", feature = "sdk-26-00"))]
fn reset_unpacker(state: &mut lzma_sdk_sys::CXzUnpacker) {
    // SAFETY: `state` is an initialized unpacker owned by the caller.
    unsafe {
        lzma_sdk_sys::XzUnpacker_Init(state);
    }
}

/// Resets the legacy `XZ` unpacker state.
#[cfg(any(feature = "sdk-9-20", feature = "sdk-16-04", feature = "sdk-19-00"))]
fn reset_unpacker(_state: &mut lzma_sdk_sys::CXzUnpacker) {}

/// Performs a single raw `XZ` decode operation into the provided output buffer.
#[cfg(any(feature = "sdk-23-01", feature = "sdk-26-00"))]
#[allow(clippy::too_many_arguments)]
fn decode_impl(
    state: &mut lzma_sdk_sys::CXzUnpacker,
    dest: *mut u8,
    dest_len: &mut usize,
    src: *const u8,
    src_len: &mut usize,
    src_finished: bool,
    finish_mode: FinishMode,
    status: &mut lzma_sdk_sys::ECoderStatus,
) -> i32 {
    // SAFETY: The unpacker state is initialized, buffers are valid for the supplied lengths,
    // and `status` is a live out-parameter.
    unsafe {
        lzma_sdk_sys::XzUnpacker_Code(
            state,
            dest,
            dest_len,
            src,
            src_len,
            i32::from(src_finished),
            match finish_mode {
                FinishMode::Any => lzma_sdk_sys::ECoderFinishMode_CODER_FINISH_ANY,
                FinishMode::End => lzma_sdk_sys::ECoderFinishMode_CODER_FINISH_END,
            },
            status,
        )
    }
}

/// Performs a single raw `XZ` decode operation into the provided output buffer.
#[cfg(any(feature = "sdk-9-20", feature = "sdk-16-04", feature = "sdk-19-00"))]
fn decode_impl(
    state: &mut lzma_sdk_sys::CXzUnpacker,
    dest: *mut u8,
    dest_len: &mut usize,
    src: *const u8,
    src_len: &mut usize,
    _src_finished: bool,
    finish_mode: FinishMode,
    status: &mut lzma_sdk_sys::ECoderStatus,
) -> i32 {
    // SAFETY: The legacy unpacker state is initialized, buffers are valid, and `status`
    // remains live for the duration of the call.
    unsafe {
        lzma_sdk_sys::XzUnpacker_Code(
            state,
            dest,
            dest_len,
            src,
            src_len,
            match finish_mode {
                FinishMode::Any => lzma_sdk_sys::ECoderFinishMode_CODER_FINISH_ANY as i32,
                FinishMode::End => lzma_sdk_sys::ECoderFinishMode_CODER_FINISH_END as i32,
            },
            status,
        )
    }
}

/// Checks if the `XZ` stream has finished.
///
/// # Arguments
///
/// - `state`: Reference to the decoder state allocated by the LZMA SDK.
///
/// # Returns
///
/// Returns `true` if the stream has finished, otherwise `false`.
fn stream_finished(state: &lzma_sdk_sys::CXzUnpacker) -> bool {
    // SAFETY: `state` points to a live unpacker owned by the caller.
    unsafe {
        lzma_sdk_sys::XzUnpacker_IsStreamWasFinished(
            state as *const lzma_sdk_sys::CXzUnpacker as *mut lzma_sdk_sys::CXzUnpacker,
        ) != 0
    }
}

#[cfg(test)]
mod tests {
    use crate::{LzmaAction, XzOptions};

    use super::super::encoder::compress;
    use super::Decoder;

    fn sample_payload() -> Vec<u8> {
        b"xz incremental decode".repeat(96)
    }

    fn compressible_payload() -> Vec<u8> {
        vec![0_u8; 10_000]
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
        let compressed = compress(&input, &XzOptions::default()).unwrap();
        let mut decoder = Decoder::new().unwrap();

        let restored = decode_all(&mut decoder, &compressed.compressed, 53);

        assert_eq!(restored, input);
        assert!(decoder.is_finished());
        assert_eq!(decoder.total_in(), compressed.compressed.len() as u64);
        assert_eq!(decoder.total_out(), input.len() as u64);
    }

    #[test]
    fn reset_clears_state_for_a_new_stream() {
        let input = sample_payload();
        let compressed = compress(&input, &XzOptions::default()).unwrap();
        let mut decoder = Decoder::new().unwrap();

        let restored = decode_all(&mut decoder, &compressed.compressed, 47);
        assert_eq!(restored, input);

        decoder.reset();
        assert!(!decoder.is_finished());
        assert_eq!(decoder.total_in(), 0);
        assert_eq!(decoder.total_out(), 0);

        let restored = decode_all(&mut decoder, &compressed.compressed, 31);
        assert_eq!(restored, input);
    }

    #[test]
    fn decodes_highly_compressible_payload() {
        let input = compressible_payload();
        let compressed = compress(&input, &XzOptions::default()).unwrap();
        let mut decoder = Decoder::new().unwrap();

        let restored = decode_all(&mut decoder, &compressed.compressed, 257);

        assert_eq!(restored, input);
        assert!(decoder.is_finished());
    }
}
