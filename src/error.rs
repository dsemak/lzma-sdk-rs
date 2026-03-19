//! Error types used by the safe `LZMA SDK` wrappers.

use std::error::Error as StdError;
use std::fmt;
use std::io;

/// Type alias for `Result<T, Error>`.
pub type Result<T> = std::result::Result<T, Error>;

/// Error values returned by the safe LZMA SDK wrappers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// Input data is structurally invalid or corrupt.
    Data,
    /// The SDK could not allocate enough memory to complete the operation.
    Mem,
    /// A checksum embedded in the payload did not match the decoded contents.
    Crc,
    /// The requested SDK operation or encoded feature is not supported.
    Unsupported,
    /// The caller supplied invalid parameters to the encoder or decoder.
    Param,
    /// The decoder reached the end of input before the stream finished.
    InputEof,
    /// The provided output buffer was too small to complete the operation.
    OutputEof,
    /// A read callback returned an SDK read error.
    Read,
    /// A write callback returned an SDK write error.
    Write,
    /// The SDK progress callback reported a failure.
    Progress,
    /// The SDK reported an internal failure without a more specific status.
    Fail,
    /// The SDK reported a threading-related failure.
    Thread,
    /// The SDK detected an archive-level error while decoding container data.
    Archive,
    /// The input did not contain a recognized archive.
    NoArchive,
    /// A requested or computed buffer size overflowed `usize`.
    SizeOverflow,
    /// A Rust [`std::io`] operation failed while reading from or writing to a safe wrapper.
    Io {
        /// Lossy [`std::io`] error classification kept by the wrapper.
        kind: io::ErrorKind,
        /// Short context string describing the operation that failed.
        context: String,
    },
    /// A high-level option value failed validation before reaching the SDK.
    InvalidOption {
        /// Name of the rejected option.
        name: String,
        /// Human-readable validation failure details.
        details: String,
    },
    /// The current SDK feature set cannot support a requested high-level capability.
    UnsupportedFeature(String),
    /// Fallback for status codes not modeled by this wrapper yet.
    Unknown(i32),
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Data => formatter.write_str("LZMA data error"),
            Self::Mem => formatter.write_str("LZMA memory allocation error"),
            Self::Crc => formatter.write_str("LZMA CRC error"),
            Self::Unsupported => formatter.write_str("LZMA unsupported feature"),
            Self::Param => formatter.write_str("LZMA invalid parameter"),
            Self::InputEof => formatter.write_str("LZMA input ended early"),
            Self::OutputEof => formatter.write_str("LZMA output buffer too small"),
            Self::Read => formatter.write_str("LZMA read error"),
            Self::Write => formatter.write_str("LZMA write error"),
            Self::Progress => formatter.write_str("LZMA progress callback error"),
            Self::Fail => formatter.write_str("LZMA internal failure"),
            Self::Thread => formatter.write_str("LZMA threading error"),
            Self::Archive => formatter.write_str("LZMA archive error"),
            Self::NoArchive => formatter.write_str("LZMA archive not found"),
            Self::SizeOverflow => formatter.write_str("LZMA buffer size overflow"),
            Self::Io { kind, context } => write!(formatter, "{context} I/O error: {kind}"),
            Self::InvalidOption { name, details } => {
                write!(formatter, "invalid option `{name}`: {details}")
            }
            Self::UnsupportedFeature(feature) => {
                write!(formatter, "unsupported feature: {feature}")
            }
            Self::Unknown(code) => write!(formatter, "unknown LZMA status code {code}"),
        }
    }
}

impl StdError for Error {}

impl Error {
    /// Returns a short `xz(1)`-style message for the current error.
    pub fn xz_message(&self) -> &'static str {
        match self {
            Self::Data => "Compressed data is corrupt",
            Self::Mem => "Memory allocation failed",
            Self::Crc => "Integrity check failed",
            Self::Unsupported | Self::UnsupportedFeature(_) => "Unsupported options",
            Self::Param | Self::InvalidOption { .. } => "Invalid or unsupported options",
            Self::InputEof => "Unexpected end of input",
            Self::OutputEof => "Output buffer is too small",
            Self::Read => "Read error",
            Self::Write => "Write error",
            Self::Progress | Self::Fail | Self::Thread | Self::Archive | Self::NoArchive => {
                "Internal error (bug)"
            }
            Self::SizeOverflow => "Size overflow",
            Self::Io { .. } => "I/O error",
            Self::Unknown(_) => "Unknown error",
        }
    }
}

/// Returns an `InvalidOption` error with the provided name and details.
pub(crate) fn invalid_option(name: String, details: String) -> Error {
    Error::InvalidOption { name, details }
}

/// Maps an SDK status code to an error variant.
pub(crate) fn map_status(code: i32) -> Error {
    match code {
        value if value == lzma_sdk_sys::SZ_OK as i32 => {
            unreachable!("success code should not be mapped as an error")
        }
        value if value == lzma_sdk_sys::SZ_ERROR_DATA as i32 => Error::Data,
        value if value == lzma_sdk_sys::SZ_ERROR_MEM as i32 => Error::Mem,
        value if value == lzma_sdk_sys::SZ_ERROR_CRC as i32 => Error::Crc,
        value if value == lzma_sdk_sys::SZ_ERROR_UNSUPPORTED as i32 => Error::Unsupported,
        value if value == lzma_sdk_sys::SZ_ERROR_PARAM as i32 => Error::Param,
        value if value == lzma_sdk_sys::SZ_ERROR_INPUT_EOF as i32 => Error::InputEof,
        value if value == lzma_sdk_sys::SZ_ERROR_OUTPUT_EOF as i32 => Error::OutputEof,
        value if value == lzma_sdk_sys::SZ_ERROR_READ as i32 => Error::Read,
        value if value == lzma_sdk_sys::SZ_ERROR_WRITE as i32 => Error::Write,
        value if value == lzma_sdk_sys::SZ_ERROR_PROGRESS as i32 => Error::Progress,
        value if value == lzma_sdk_sys::SZ_ERROR_FAIL as i32 => Error::Fail,
        value if value == lzma_sdk_sys::SZ_ERROR_THREAD as i32 => Error::Thread,
        value if value == lzma_sdk_sys::SZ_ERROR_ARCHIVE as i32 => Error::Archive,
        value if value == lzma_sdk_sys::SZ_ERROR_NO_ARCHIVE as i32 => Error::NoArchive,
        value => Error::Unknown(value),
    }
}
