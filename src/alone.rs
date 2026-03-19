//! Legacy `.lzma` (`LZMA_Alone`) container helpers.

use crate::error::{invalid_option, Error, Result};
use crate::{drain_pending, LzmaAction, LzmaOptions, LzmaProps, RawDecoder, RawEncoder};

/// Size of the legacy `.lzma` header in bytes.
pub const LZMA_ALONE_HEADER_SIZE: usize = 1 + 4 + 8;

const UNKNOWN_UNCOMPRESSED_SIZE: u64 = u64::MAX;

/// Buffered `.lzma` encoder built on top of the raw-LZMA encoder.
pub struct Encoder {
    inner: RawEncoder,
    pending: Option<Vec<u8>>,
    output_offset: usize,
    total_in: u64,
    total_out: u64,
}

impl Encoder {
    /// Creates a new `.lzma` encoder.
    pub fn new(options: LzmaOptions) -> Self {
        let options = options.with_end_marker(crate::lzma::EndMarkerMode::Enabled);
        Self {
            inner: RawEncoder::new(options),
            pending: None,
            output_offset: 0,
            total_in: 0,
            total_out: 0,
        }
    }

    /// Encodes the next chunk of input.
    pub fn process(
        &mut self,
        input: &[u8],
        output: &mut [u8],
        action: LzmaAction,
    ) -> Result<(usize, usize)> {
        if self.pending.is_none() {
            let (consumed, _) = self.inner.process(input, &mut [], action)?;
            self.total_in += consumed as u64;

            if action.is_finish() {
                let props = self.inner.properties().ok_or(Error::Fail)?;
                let payload = self.inner.encoded().ok_or(Error::Fail)?;
                let pending = encode_header(
                    props,
                    UNKNOWN_UNCOMPRESSED_SIZE,
                    payload.compressed.as_slice(),
                );
                self.pending = Some(pending);
            }
        } else if !input.is_empty() {
            return Err(Error::Param);
        }

        let written = match &self.pending {
            Some(pending) => {
                let written = drain_pending(pending, &mut self.output_offset, output);
                self.total_out += written as u64;
                written
            }
            None => 0,
        };

        Ok((input.len(), written))
    }

    /// Returns `true` once all buffered output has been drained.
    pub fn is_finished(&self) -> bool {
        self.pending
            .as_ref()
            .is_some_and(|pending| self.output_offset == pending.len())
    }

    /// Returns the total number of source bytes accepted by the encoder.
    pub fn total_in(&self) -> u64 {
        self.total_in
    }

    /// Returns the total number of encoded bytes written through [`Self::process`].
    pub fn total_out(&self) -> u64 {
        self.total_out
    }
}

/// Incremental `.lzma` decoder.
pub struct Decoder {
    header: Vec<u8>,
    inner: Option<RawDecoder>,
    expected_uncompressed_size: Option<u64>,
    finished: bool,
    total_in: u64,
    total_out: u64,
}

impl Decoder {
    /// Creates a new `.lzma` decoder.
    pub fn new() -> Result<Self> {
        Ok(Self {
            header: Vec::with_capacity(LZMA_ALONE_HEADER_SIZE),
            inner: None,
            expected_uncompressed_size: None,
            finished: false,
            total_in: 0,
            total_out: 0,
        })
    }

    /// Decodes the next chunk of `.lzma` input into `output`.
    pub fn process(
        &mut self,
        input: &[u8],
        output: &mut [u8],
        action: LzmaAction,
    ) -> Result<(usize, usize)> {
        if self.finished {
            return Err(Error::Param);
        }

        let mut consumed = 0usize;
        let mut written = 0usize;

        if self.inner.is_none() {
            let needed = LZMA_ALONE_HEADER_SIZE.saturating_sub(self.header.len());
            let take = needed.min(input.len());
            self.header.extend_from_slice(&input[..take]);
            consumed += take;

            if self.header.len() < LZMA_ALONE_HEADER_SIZE {
                if action.is_finish() {
                    return Err(Error::InputEof);
                }
                self.total_in += consumed as u64;
                return Ok((consumed, 0));
            }

            let header = decode_header(
                self.header
                    .as_slice()
                    .try_into()
                    .expect("fixed-size header"),
            )?;
            self.expected_uncompressed_size = header.uncompressed_size;
            self.inner = Some(RawDecoder::new(&header.props)?);
        }

        let payload = &input[consumed..];
        let inner = self.inner.as_mut().expect("decoder initialized");
        let (payload_read, payload_written) = inner.process(payload, output, action)?;
        consumed += payload_read;
        written += payload_written;

        let total_out = self
            .total_out
            .checked_add(written as u64)
            .ok_or(Error::SizeOverflow)?;
        self.finished = inner.is_finished();
        if let Some(expected_uncompressed_size) = self.expected_uncompressed_size {
            if total_out > expected_uncompressed_size {
                return Err(Error::Data);
            }

            if self.finished && total_out != expected_uncompressed_size {
                return Err(Error::Data);
            }

            if action.is_finish()
                && total_out == expected_uncompressed_size
                && consumed == input.len()
            {
                self.finished = true;
            }
        }
        self.total_in += consumed as u64;
        self.total_out = total_out;

        Ok((consumed, written))
    }

    /// Returns `true` once the decoder has finished the current stream.
    pub fn is_finished(&self) -> bool {
        self.finished
    }

    /// Returns the total number of encoded bytes consumed so far.
    pub fn total_in(&self) -> u64 {
        self.total_in
    }

    /// Returns the total number of decoded bytes produced so far.
    pub fn total_out(&self) -> u64 {
        self.total_out
    }
}

fn encode_header(
    props: [u8; crate::LZMA_PROPS_SIZE],
    uncompressed_size: u64,
    payload: &[u8],
) -> Vec<u8> {
    let mut output = Vec::with_capacity(LZMA_ALONE_HEADER_SIZE + payload.len());
    output.extend_from_slice(&props);
    output.extend_from_slice(&uncompressed_size.to_le_bytes());
    output.extend_from_slice(payload);
    output
}

struct AloneHeader {
    props: LzmaProps,
    uncompressed_size: Option<u64>,
}

fn decode_header(header: &[u8; LZMA_ALONE_HEADER_SIZE]) -> Result<AloneHeader> {
    let mut props = [0u8; crate::LZMA_PROPS_SIZE];
    props.copy_from_slice(&header[..crate::LZMA_PROPS_SIZE]);
    let props = LzmaProps::decode(&props)?;

    let dict_size = props.dict_size();
    if dict_size == 0 {
        return Err(invalid_option(
            "dict_size".into(),
            "expected a non-zero dictionary size".into(),
        ));
    }

    let uncompressed_size = u64::from_le_bytes(
        header[crate::LZMA_PROPS_SIZE..]
            .try_into()
            .expect("fixed-size length"),
    );

    Ok(AloneHeader {
        props,
        uncompressed_size: (uncompressed_size != UNKNOWN_UNCOMPRESSED_SIZE)
            .then_some(uncompressed_size),
    })
}

#[cfg(test)]
mod tests {
    use super::{Decoder, Encoder, LZMA_ALONE_HEADER_SIZE};
    use crate::lzma::CompressionLevel;
    use crate::{LzmaAction, LzmaOptions};

    #[test]
    fn round_trip_lzma_alone_stream() {
        let options = LzmaOptions::default()
            .with_end_marker(crate::lzma::EndMarkerMode::Enabled)
            .with_reduce_size(32);
        let mut encoder = Encoder::new(options);
        let input = b"legacy container payload";
        let mut encoded = vec![0u8; 512];

        let (read, _) = encoder.process(input, &mut [], LzmaAction::Run).unwrap();
        assert_eq!(read, input.len());
        let (_, written) = encoder
            .process(&[], &mut encoded, LzmaAction::Finish)
            .unwrap();
        encoded.truncate(written);
        assert!(encoded.len() > LZMA_ALONE_HEADER_SIZE);

        let mut decoder = Decoder::new().unwrap();
        let mut output = vec![0u8; 128];
        let (consumed, restored) = decoder
            .process(&encoded, &mut output, LzmaAction::Finish)
            .unwrap();
        assert_eq!(consumed, encoded.len());
        assert_eq!(&output[..restored], input);
    }

    #[test]
    fn rejects_truncated_header_on_finish() {
        let mut decoder = Decoder::new().unwrap();
        let mut output = vec![0u8; 16];
        let err = decoder
            .process(&[0u8; 4], &mut output, LzmaAction::Finish)
            .unwrap_err();
        assert_eq!(err, crate::Error::InputEof);
    }

    #[test]
    fn encoder_drains_output_incrementally() {
        let options = LzmaOptions::builder()
            .level(CompressionLevel::DEFAULT)
            .build();
        let mut encoder = Encoder::new(options);
        let input = b"drain me slowly";
        let _ = encoder.process(input, &mut [], LzmaAction::Finish).unwrap();
        let mut first = [0u8; 8];
        let (_, written_first) = encoder
            .process(&[], &mut first, LzmaAction::Finish)
            .unwrap();
        assert!(written_first > 0);
    }

    #[test]
    fn decodes_known_size_without_end_marker() {
        let encoded = [
            93, 0, 16, 0, 0, 13, 0, 0, 0, 0, 0, 0, 0, 0, 36, 25, 73, 152, 111, 5, 21, 39, 39, 13,
            118, 120, 208, 41, 29, 32, 0,
        ];
        let mut decoder = Decoder::new().unwrap();
        let mut output = vec![0u8; 64];
        let (_, written) = decoder
            .process(&encoded, &mut output, LzmaAction::Finish)
            .unwrap();
        assert_eq!(&output[..written], b"Hello\nWorld!\n");
        assert!(decoder.is_finished());
    }

    #[test]
    fn rejects_mismatched_known_size_with_end_marker() {
        let encoded = [
            93, 0, 16, 0, 0, 14, 0, 0, 0, 0, 0, 0, 0, 0, 36, 25, 73, 152, 111, 5, 21, 39, 39, 13,
            118, 120, 208, 42, 104, 23, 21, 255, 255, 117, 248, 0, 0,
        ];
        let mut decoder = Decoder::new().unwrap();
        let mut output = vec![0u8; 64];
        let err = decoder
            .process(&encoded, &mut output, LzmaAction::Finish)
            .unwrap_err();
        assert_eq!(err, crate::Error::Data);
    }
}
