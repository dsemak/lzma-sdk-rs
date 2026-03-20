//! # FFI Adapters for LZMA Stream Interfaces
//!
//! This module provides Rust implementations that act as bridges ("adapters") for
//! callback-based stream interfaces required by the C-based LZMA SDK.

use std::ffi::c_void;
use std::io::{self, Read, Seek, SeekFrom};
use std::ptr;
use std::slice;

use lzma_sdk_sys::{
    ESzSeek, ESzSeek_SZ_SEEK_CUR, ESzSeek_SZ_SEEK_END, ESzSeek_SZ_SEEK_SET, ILookInStream,
    ISeekInStream, ISeqInStream, ISeqOutStream, Int64, SRes, SZ_ERROR_PARAM, SZ_ERROR_READ, SZ_OK,
};

/// FFI handle types for matching SDK variations (input).
#[cfg(any(feature = "sdk-9-20", feature = "sdk-16-04"))]
type InStreamApiPtr = *mut ISeqInStream;
#[cfg(feature = "sdk-19-00")]
type InStreamApiPtr = *mut ISeqInStream;
#[cfg(any(feature = "sdk-23-01", feature = "sdk-26-00"))]
type InStreamApiPtr = lzma_sdk_sys::ISeqInStreamPtr;

/// FFI handle types for matching SDK variations (output).
#[cfg(any(feature = "sdk-9-20", feature = "sdk-16-04"))]
type OutStreamApiPtr = *mut ISeqOutStream;
#[cfg(feature = "sdk-19-00")]
type OutStreamApiPtr = *mut ISeqOutStream;
#[cfg(any(feature = "sdk-23-01", feature = "sdk-26-00"))]
type OutStreamApiPtr = lzma_sdk_sys::ISeqOutStreamPtr;

/// Callback pointer types for different SDK versions (input).
#[cfg(any(feature = "sdk-9-20", feature = "sdk-16-04"))]
type InStreamCallbackPtr = *mut c_void;
#[cfg(feature = "sdk-19-00")]
type InStreamCallbackPtr = *const ISeqInStream;
#[cfg(any(feature = "sdk-23-01", feature = "sdk-26-00"))]
type InStreamCallbackPtr = lzma_sdk_sys::ISeqInStreamPtr;

/// Callback pointer types for different SDK versions (output).
#[cfg(any(feature = "sdk-9-20", feature = "sdk-16-04"))]
type OutStreamCallbackPtr = *mut c_void;
#[cfg(feature = "sdk-19-00")]
type OutStreamCallbackPtr = *const ISeqOutStream;
#[cfg(any(feature = "sdk-23-01", feature = "sdk-26-00"))]
type OutStreamCallbackPtr = lzma_sdk_sys::ISeqOutStreamPtr;

/// Callback pointer type for seekable input streams.
#[cfg(any(feature = "sdk-9-20", feature = "sdk-16-04"))]
type SeekInStreamCallbackPtr = *mut c_void;
#[cfg(feature = "sdk-19-00")]
type SeekInStreamCallbackPtr = *const ISeekInStream;
#[cfg(any(feature = "sdk-23-01", feature = "sdk-26-00"))]
type SeekInStreamCallbackPtr = lzma_sdk_sys::ISeekInStreamPtr;

/// FFI pointer for look-in-stream vtable.
#[cfg(any(feature = "sdk-9-20", feature = "sdk-16-04"))]
type LookInStreamApiPtr = *mut ILookInStream;
#[cfg(feature = "sdk-19-00")]
type LookInStreamApiPtr = *mut ILookInStream;
#[cfg(any(feature = "sdk-23-01", feature = "sdk-26-00"))]
type LookInStreamApiPtr = lzma_sdk_sys::ILookInStreamPtr;

/// Marker trait ensuring a type is suitable for our stream wrappers: must support Read, Seek, and
/// Send (which is required for safe boxing and FFI boundaries).
pub(crate) trait ReadSeek: Read + Seek + Send {}
impl<T> ReadSeek for T where T: Read + Seek + Send {}

/// FFI-safe struct adapter to present an immutable `[u8]` slice as a C-style LZMA
/// ISeqInStream input source.
#[repr(C)]
pub(crate) struct SliceInStream {
    /// the FFI vtable as understood by LZMA
    vtable: ISeqInStream,
    /// Pointer to the base data of the slice
    data: *const u8,
    /// Total number of bytes in slice
    len: usize,
    /// Current offset into the slice (where the next read will begin)
    pos: usize,
}

impl SliceInStream {
    /// Construct a new FFI stream adapter for a byte slice.
    ///
    /// # Arguments
    ///
    /// * `data` - The byte slice to wrap as an input stream.
    ///
    /// # Returns
    ///
    /// A new [`SliceInStream`] instance wrapping the provided byte slice.
    pub(crate) fn new(data: &[u8]) -> Self {
        Self {
            vtable: ISeqInStream {
                Read: Some(slice_in_stream_read),
            },
            data: data.as_ptr(),
            len: data.len(),
            pos: 0,
        }
    }

    /// Get a raw vtable pointer suitable for passing to SDK,
    /// casting through the type alias set by feature configuration.
    pub(crate) fn as_ptr(&self) -> InStreamApiPtr {
        (&raw const self.vtable).cast_mut()
    }
}

/// Callback implementation for the LZMA "Read" operation for `SliceInStream`.
///
/// Copies up to the requested number of bytes from the slice to C memory;
/// advances cursor, and writes the actual amount read back to the SDK.
///
/// # Arguments
///
/// * `stream` - points to our wrapped `SliceInStream` (possibly cast by ABI)
/// * `buf` - pointer to C buffer where bytes must be written
/// * `size` - pointer to input/output variable: the requested byte count, set to bytes actually read
///
/// # Returns
///
/// Returns SZ_OK if read finished (could be less than requested, if at end).
unsafe extern "C" fn slice_in_stream_read(
    stream: InStreamCallbackPtr,
    buf: *mut c_void,
    size: *mut usize,
) -> SRes {
    // SDK provides variant ABI pointer types per version: convert to &mut SliceInStream.
    #[cfg(any(feature = "sdk-9-20", feature = "sdk-16-04"))]
    let stream = stream.cast::<SliceInStream>();
    #[cfg(any(feature = "sdk-19-00", feature = "sdk-23-01", feature = "sdk-26-00"))]
    let stream = stream.cast_mut().cast::<SliceInStream>();

    // Fetch the number of bytes requested by SDK.
    let requested = unsafe { *size };

    // Compute bytes remaining from current "pos" to end of slice.
    let remaining = unsafe { (*stream).len - (*stream).pos };

    let count = remaining.min(requested);
    if count != 0 {
        // Copy bytes from the slice data to the requested C buffer.
        unsafe {
            ptr::copy_nonoverlapping((*stream).data.add((*stream).pos), buf.cast::<u8>(), count);
        }
    }

    // Advance position and commit the actual count read.
    unsafe {
        (*stream).pos += count;
        *size = count;
    }
    SZ_OK as i32
}

/// Adapter to present a mutable `Vec<u8>` as a C-style output stream (`ISeqOutStream`).
#[repr(C)]
pub(crate) struct VecOutStream {
    /// SDK output stream vtable with Rust callback
    vtable: ISeqOutStream,
    /// Pointer to a Vec<u8> that receives the written bytes
    output: *mut Vec<u8>,
}

impl VecOutStream {
    /// Construct a new `VecOutStream` adapter wrapping a mutable `Vec<u8>`.
    ///
    /// # Arguments
    ///
    /// * `output` - The mutable `Vec<u8>` to wrap as an output stream.
    ///
    /// # Returns
    ///
    /// A new [`VecOutStream`] instance wrapping the provided mutable `Vec<u8>`.
    pub(crate) fn new(output: &mut Vec<u8>) -> Self {
        Self {
            vtable: ISeqOutStream {
                Write: Some(vec_out_stream_write),
            },
            output,
        }
    }

    /// Return the vtable pointer as needed for FFI, applying type alias for SDK version.
    pub(crate) fn as_ptr(&self) -> OutStreamApiPtr {
        (&raw const self.vtable).cast_mut()
    }
}

/// Write callback for the LZMA SDK, as used with `VecOutStream`.
///
/// # Arguments
///
/// * `stream` - points to our wrapped `VecOutStream` (possibly cast by ABI)
/// * `buf` - pointer to C buffer where bytes must be written
/// * `size` - number of bytes to write
///
/// # Returns
///
/// Returns the number of bytes written.
///
/// # Safety
///
/// The output vector pointer must outlive the FFI use and must not alias with slices from
/// the input SDK buffers.
unsafe extern "C" fn vec_out_stream_write(
    stream: OutStreamCallbackPtr,
    buf: *const c_void,
    size: usize,
) -> usize {
    if size == 0 {
        return 0;
    }

    // Per-SDK ABI variances: turn user pointer into `&mut VecOutStream`
    #[cfg(any(feature = "sdk-9-20", feature = "sdk-16-04"))]
    let stream = stream.cast::<VecOutStream>();
    #[cfg(any(feature = "sdk-19-00", feature = "sdk-23-01", feature = "sdk-26-00"))]
    let stream = stream.cast_mut().cast::<VecOutStream>();

    // Turn the (void*, size) buffer into a Rust slice view.
    let bytes = unsafe { slice::from_raw_parts(buf.cast::<u8>(), size) };

    // Extend the output Vec<u8> with the received bytes.
    unsafe {
        (*(*stream).output).extend_from_slice(bytes);
    }
    size
}

/// Adapter struct providing an `ISeekInStream` implementation over any dynamically boxed type that
/// implements `Read + Seek + Send`.
#[repr(C)]
struct SeekInStream {
    /// LZMA vtable with pointers to our callbacks
    vtable: ISeekInStream,
    /// Boxed trait object for user data source
    reader: Box<dyn ReadSeek>,
    /// Allows tracking most recent error, for diagnosis after SDK operations
    last_error_kind: Option<io::ErrorKind>,
}

impl SeekInStream {
    /// Wrap a boxed ReadSeek-compatible object for use as a seekable LZMA input source.
    ///
    /// # Arguments
    ///
    /// * `reader` - The boxed trait object for user data source.
    ///
    /// # Returns
    ///
    /// A new [`SeekInStream`] instance wrapping the provided boxed trait object.
    fn new(reader: Box<dyn ReadSeek>) -> Self {
        Self {
            vtable: ISeekInStream {
                Read: Some(seek_in_stream_read),
                Seek: Some(seek_in_stream_seek),
            },
            reader,
            last_error_kind: None,
        }
    }
}

/// The "read" callback for LZMA ISeekInStream, called by SDK input routines.
///
/// Updates `*size` with the number of actual bytes read and tracks errors for subsequent inspection.
///
/// # Arguments
///
/// * `stream` - points to our wrapped `SeekInStream` (possibly cast by ABI)
/// * `buf` - pointer to C buffer where bytes must be written
/// * `size` - pointer to input/output variable: the requested byte count, set to bytes actually read
///
/// # Returns
///
/// Returns SZ_OK if read finished (could be less than requested, if at end).
unsafe extern "C" fn seek_in_stream_read(
    stream: SeekInStreamCallbackPtr,
    buf: *mut c_void,
    size: *mut usize,
) -> SRes {
    // Cast opaque SDK pointer to our adapter struct.
    #[cfg(any(feature = "sdk-9-20", feature = "sdk-16-04"))]
    let stream = stream.cast::<SeekInStream>();
    #[cfg(any(feature = "sdk-19-00", feature = "sdk-23-01", feature = "sdk-26-00"))]
    let stream = stream.cast_mut().cast::<SeekInStream>();

    // Get count requested; build a mutable Rust slice for reading.
    let requested = unsafe { *size };
    let target = if requested == 0 {
        &mut []
    } else {
        // Convert the SDK buffer into a Rust writable slice for the read.
        unsafe { slice::from_raw_parts_mut(buf.cast::<u8>(), requested) }
    };

    // Call the trait object's `read` method.
    let result: io::Result<usize> = unsafe { (*stream).reader.read(target) };
    match result {
        Ok(read) => {
            unsafe {
                *size = read;
                (*stream).last_error_kind = None;
            }
            SZ_OK as i32
        }
        Err(error) => {
            // If read fails, record error and signal error to SDK.
            unsafe {
                (*stream).last_error_kind = Some(error.kind());
                *size = 0;
            }
            SZ_ERROR_READ as i32
        }
    }
}

/// The "seek" callback for LZMA ISeekInStream, called by SDK for random access.
///
/// # Arguments
///
/// * `stream` - points to our wrapped `SeekInStream` (possibly cast by ABI)
/// * `pos` - pointer to C parameter: the offset to seek to
/// * `origin` - C enum for seek origin
///
/// # Returns
///
/// Returns SZ_OK if seek finished (could be less than requested, if at end).
unsafe extern "C" fn seek_in_stream_seek(
    stream: SeekInStreamCallbackPtr,
    pos: *mut Int64,
    origin: ESzSeek,
) -> SRes {
    // Cast underlying pointer to our adapter type.
    #[cfg(any(feature = "sdk-9-20", feature = "sdk-16-04"))]
    let stream = stream.cast::<SeekInStream>();
    #[cfg(any(feature = "sdk-19-00", feature = "sdk-23-01", feature = "sdk-26-00"))]
    let stream = stream.cast_mut().cast::<SeekInStream>();

    // Read offset from C parameter.
    let offset = unsafe { *pos };
    // Convert C enum for seek origin to Rust's SeekFrom, checking inputs as needed.
    let seek_from = match origin {
        value if value == ESzSeek_SZ_SEEK_SET => match u64::try_from(offset) {
            Ok(position) => SeekFrom::Start(position),
            Err(_) => {
                unsafe {
                    (*stream).last_error_kind = Some(io::ErrorKind::InvalidInput);
                }
                return SZ_ERROR_PARAM as i32;
            }
        },
        value if value == ESzSeek_SZ_SEEK_CUR => SeekFrom::Current(offset),
        value if value == ESzSeek_SZ_SEEK_END => SeekFrom::End(offset),
        _ => {
            unsafe {
                (*stream).last_error_kind = Some(io::ErrorKind::InvalidInput);
            }
            return SZ_ERROR_PARAM as i32;
        }
    };

    // Actually call Rust seek. Result is new offset if successful.
    let result: io::Result<u64> = unsafe { (*stream).reader.seek(seek_from) };
    match result {
        Ok(position) => {
            // Ensure offset fits in i64 if returning to C.
            let position = match i64::try_from(position) {
                Ok(position) => position,
                Err(_) => {
                    unsafe {
                        (*stream).last_error_kind = Some(io::ErrorKind::InvalidInput);
                    }
                    return SZ_ERROR_PARAM as i32;
                }
            };

            // Report back new absolute offset and clear error state.
            unsafe {
                *pos = position;
                (*stream).last_error_kind = None;
            }
            SZ_OK as i32
        }
        Err(error) => {
            unsafe {
                (*stream).last_error_kind = Some(error.kind());
            }
            SZ_ERROR_READ as i32
        }
    }
}

/// Buffered lookahead stream adapter bridging a standard Rust Read+Seek source with the SDK's
/// lookahead ("LookToRead") stream APIs for versions 9.20 and 16.04.
#[cfg(any(feature = "sdk-9-20", feature = "sdk-16-04"))]
pub(crate) struct LookStream {
    /// Our Rust->FFI adapter for Read+Seek
    seek_stream: SeekInStream,
    /// LZMA's internal buffering object (zero-init then vtable-inited)
    look_stream: lzma_sdk_sys::CLookToRead,
}

#[cfg(any(feature = "sdk-9-20", feature = "sdk-16-04"))]
impl LookStream {
    /// Instantiate a new boxed LookStream to provide LZMA lookahead to the native decoder.
    ///
    /// # Arguments
    ///
    /// * `reader` - The boxed trait object for user data source.
    ///
    /// # Returns
    ///
    /// A new [`LookStream`] instance wrapping the provided boxed trait object.
    pub(crate) fn new(reader: Box<dyn ReadSeek>) -> Box<Self> {
        let mut stream = Box::new(Self {
            seek_stream: SeekInStream::new(reader),
            look_stream: unsafe {
                std::mem::MaybeUninit::<lzma_sdk_sys::CLookToRead>::zeroed().assume_init()
            },
        });

        // Fill the vtable struct; native SDK will update it via LookToRead_CreateVTable.
        stream.look_stream.s = ILookInStream {
            Look: None,
            Skip: None,
            Read: None,
            Seek: None,
        };
        // Set pointer to our SeekInStream vtable
        stream.look_stream.realStream = (&raw const stream.seek_stream.vtable).cast_mut();
        // Set SDK-internal lookahead cursor and size to zero
        stream.look_stream.pos = 0;
        stream.look_stream.size = 0;

        // Ask the SDK to patch up our struct with function pointers for buffering.
        // This is safe since look_stream lives inside the Box.
        unsafe {
            lzma_sdk_sys::LookToRead_CreateVTable(&mut stream.look_stream, 0);
        }

        stream
    }

    /// Return a pointer to the LZMA-compatible lookahead interface,
    /// suitable for native decoder consumption.
    pub(crate) fn as_ptr(&self) -> LookInStreamApiPtr {
        (&raw const self.look_stream.s).cast_mut()
    }

    /// Expose the most recent I/O error, if present, from the underlying seekable reader.
    pub(crate) fn last_error_kind(&self) -> Option<io::ErrorKind> {
        self.seek_stream.last_error_kind
    }
}

/// Buffered lookahead stream for LZMA SDK, version 19.00.
#[cfg(feature = "sdk-19-00")]
pub(crate) struct LookStream {
    /// Our Rust->FFI adapter for Read+Seek
    seek_stream: SeekInStream,
    /// LZMA's internal buffering object (zero-init then vtable-inited)
    look_stream: lzma_sdk_sys::CLookToRead2,
    /// Internal buffer for lookahead reads
    look_buffer: Vec<u8>,
}

#[cfg(feature = "sdk-19-00")]
impl LookStream {
    /// Create a boxed LookStream for LZMA lookahead interfaces (v19.00),
    /// with a preallocated buffer.
    ///
    /// # Arguments
    ///
    /// * `reader` - The boxed trait object for user data source.
    ///
    /// # Returns
    ///
    /// A new [`LookStream`] instance wrapping the provided boxed trait object.
    pub(crate) fn new(reader: Box<dyn ReadSeek>) -> Box<Self> {
        let mut stream = Box::new(Self {
            seek_stream: SeekInStream::new(reader),
            look_stream: unsafe {
                std::mem::MaybeUninit::<lzma_sdk_sys::CLookToRead2>::zeroed().assume_init()
            },
            look_buffer: vec![0_u8; 1 << 18],
        });

        // Set vtable for look_stream (SDK will patch it).
        stream.look_stream.vt = ILookInStream {
            Look: None,
            Skip: None,
            Read: None,
            Seek: None,
        };
        stream.look_stream.realStream = (&raw const stream.seek_stream.vtable).cast();
        stream.look_stream.pos = 0;
        stream.look_stream.size = 0;
        // Assign our internal buffer for lookahead; it must always be valid
        stream.look_stream.buf = stream.look_buffer.as_mut_ptr();
        stream.look_stream.bufSize = stream.look_buffer.len();

        // Patch the vtable and function pointers using SDK initialization
        unsafe {
            lzma_sdk_sys::LookToRead2_CreateVTable(&mut stream.look_stream, 0);
        }

        stream
    }

    /// Return pointer to LZMA's lookahead interface for use by decoder
    pub(crate) fn as_ptr(&self) -> LookInStreamApiPtr {
        (&raw const self.look_stream.vt).cast_mut()
    }

    /// Returns the IO error (if any) encountered in underlying seekable stream
    pub(crate) fn last_error_kind(&self) -> Option<io::ErrorKind> {
        self.seek_stream.last_error_kind
    }
}

/// LookStream adapter for latest LZMA SDKs (currently 23.01 and 26.00).
#[cfg(any(feature = "sdk-23-01", feature = "sdk-26-00"))]
pub(crate) struct LookStream {
    /// Our Rust->FFI adapter for Read+Seek
    seek_stream: SeekInStream,
    /// LZMA's internal buffering object (zero-init then vtable-inited)
    look_stream: lzma_sdk_sys::CLookToRead2,
    /// Internal buffer for lookahead reads
    look_buffer: Vec<u8>,
}

#[cfg(any(feature = "sdk-23-01", feature = "sdk-26-00"))]
impl LookStream {
    /// Create a boxed LookStream for LZMA lookahead interface.
    ///
    /// # Arguments
    ///
    /// * `reader` - The boxed trait object for user data source.
    ///
    /// # Returns
    ///
    /// A new [`LookStream`] instance wrapping the provided boxed trait object.
    pub(crate) fn new(reader: Box<dyn ReadSeek>) -> Box<Self> {
        let mut stream = Box::new(Self {
            seek_stream: SeekInStream::new(reader),
            look_stream: unsafe {
                std::mem::MaybeUninit::<lzma_sdk_sys::CLookToRead2>::zeroed().assume_init()
            },
            look_buffer: vec![0_u8; 1 << 18],
        });

        // Prepare vtable and buffer pointers. Assign everything needed for successful FFI calls.
        stream.look_stream.vt = ILookInStream {
            Look: None,
            Skip: None,
            Read: None,
            Seek: None,
        };
        stream.look_stream.realStream = (&raw const stream.seek_stream.vtable).cast();
        stream.look_stream.pos = 0;
        stream.look_stream.size = 0;
        stream.look_stream.buf = stream.look_buffer.as_mut_ptr();
        stream.look_stream.bufSize = stream.look_buffer.len();

        // Complete vtable setup so SDK can perform lookahead reads.
        unsafe {
            lzma_sdk_sys::LookToRead2_CreateVTable(&mut stream.look_stream, 0);
        }

        stream
    }

    /// Expose pointer to base vtable for LZMA-native lookahead decoding.
    pub(crate) fn as_ptr(&self) -> LookInStreamApiPtr {
        (&raw const self.look_stream.vt).cast()
    }

    /// Yields the last interruption/error kind seen by the underlying seekable reader.
    pub(crate) fn last_error_kind(&self) -> Option<io::ErrorKind> {
        self.seek_stream.last_error_kind
    }
}
