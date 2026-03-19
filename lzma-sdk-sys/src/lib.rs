#![deny(unsafe_op_in_unsafe_fn)]
#![allow(non_camel_case_types, non_snake_case, non_upper_case_globals)]

include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

use std::ffi::c_void;
use std::ptr;
use std::slice;
use std::sync::Once;

#[cfg(any(feature = "sdk-9-20", feature = "sdk-16-04"))]
type InStreamApiPtr = *mut ISeqInStream;
#[cfg(feature = "sdk-19-00")]
type InStreamApiPtr = *mut ISeqInStream;
#[cfg(any(feature = "sdk-23-01", feature = "sdk-26-00"))]
type InStreamApiPtr = ISeqInStreamPtr;

#[cfg(any(feature = "sdk-9-20", feature = "sdk-16-04"))]
type OutStreamApiPtr = *mut ISeqOutStream;
#[cfg(feature = "sdk-19-00")]
type OutStreamApiPtr = *mut ISeqOutStream;
#[cfg(any(feature = "sdk-23-01", feature = "sdk-26-00"))]
type OutStreamApiPtr = ISeqOutStreamPtr;

#[cfg(any(feature = "sdk-9-20", feature = "sdk-16-04"))]
type InStreamCallbackPtr = *mut c_void;
#[cfg(feature = "sdk-19-00")]
type InStreamCallbackPtr = *const ISeqInStream;
#[cfg(any(feature = "sdk-23-01", feature = "sdk-26-00"))]
type InStreamCallbackPtr = ISeqInStreamPtr;

#[cfg(any(feature = "sdk-9-20", feature = "sdk-16-04"))]
type OutStreamCallbackPtr = *mut c_void;
#[cfg(feature = "sdk-19-00")]
type OutStreamCallbackPtr = *const ISeqOutStream;
#[cfg(any(feature = "sdk-23-01", feature = "sdk-26-00"))]
type OutStreamCallbackPtr = ISeqOutStreamPtr;

#[cfg(any(feature = "sdk-9-20", feature = "sdk-16-04"))]
type AllocCallbackPtr = *mut c_void;
#[cfg(any(feature = "sdk-19-00", feature = "sdk-23-01", feature = "sdk-26-00"))]
type AllocCallbackPtr = *const ISzAlloc;

unsafe extern "C" {
    fn malloc(size: usize) -> *mut c_void;
    fn free(ptr: *mut c_void);
}

unsafe extern "C" fn default_sz_alloc(_p: AllocCallbackPtr, size: usize) -> *mut c_void {
    if size == 0 {
        return ptr::null_mut();
    }

    // SAFETY: `malloc` allocates `size` bytes and returns a pointer owned by the caller.
    unsafe { malloc(size) }
}

unsafe extern "C" fn default_sz_free(_p: AllocCallbackPtr, address: *mut c_void) {
    if address.is_null() {
        return;
    }

    // SAFETY: `address` came from `default_sz_alloc`, which delegates to `malloc`.
    unsafe { free(address) }
}

static DEFAULT_ALLOC: ISzAlloc = ISzAlloc {
    Alloc: Some(default_sz_alloc),
    Free: Some(default_sz_free),
};

pub fn default_alloc() -> *mut ISzAlloc {
    (&raw const DEFAULT_ALLOC).cast_mut()
}

pub fn initialize_xz_crc_tables() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        // SAFETY: The SDK documents these initializers as one-time global table setup.
        unsafe {
            CrcGenerateTable();
            Crc64GenerateTable();
        }
    });
}

#[repr(C)]
pub struct SliceInStream {
    vtable: ISeqInStream,
    data: *const u8,
    len: usize,
    pos: usize,
}

impl SliceInStream {
    pub fn new(data: &[u8]) -> Self {
        Self {
            vtable: ISeqInStream {
                Read: Some(slice_in_stream_read),
            },
            data: data.as_ptr(),
            len: data.len(),
            pos: 0,
        }
    }

    pub fn as_ptr(&self) -> InStreamApiPtr {
        (&raw const self.vtable).cast_mut()
    }
}

unsafe extern "C" fn slice_in_stream_read(
    stream: InStreamCallbackPtr,
    buf: *mut c_void,
    size: *mut usize,
) -> SRes {
    #[cfg(any(feature = "sdk-9-20", feature = "sdk-16-04"))]
    let stream = stream.cast::<SliceInStream>();
    #[cfg(any(feature = "sdk-19-00", feature = "sdk-23-01", feature = "sdk-26-00"))]
    let stream = stream.cast_mut().cast::<SliceInStream>();
    // SAFETY: `size` is provided by the SDK callback contract and points to writable memory.
    let requested = unsafe { *size };
    // SAFETY: `stream` is the `SliceInStream` instance that created this callback pointer.
    let remaining = unsafe { (*stream).len - (*stream).pos };
    let count = remaining.min(requested);
    if count != 0 {
        // SAFETY: Source and destination are valid for `count` bytes and do not overlap.
        unsafe {
            ptr::copy_nonoverlapping((*stream).data.add((*stream).pos), buf.cast::<u8>(), count);
        }
    }
    // SAFETY: `stream` and `size` are valid callback pointers owned by the caller for this call.
    unsafe {
        (*stream).pos += count;
        *size = count;
    }
    SZ_OK as i32
}

#[repr(C)]
pub struct VecOutStream {
    vtable: ISeqOutStream,
    output: *mut Vec<u8>,
}

impl VecOutStream {
    pub fn new(output: &mut Vec<u8>) -> Self {
        Self {
            vtable: ISeqOutStream {
                Write: Some(vec_out_stream_write),
            },
            output,
        }
    }

    pub fn as_ptr(&self) -> OutStreamApiPtr {
        (&raw const self.vtable).cast_mut()
    }
}

unsafe extern "C" fn vec_out_stream_write(
    stream: OutStreamCallbackPtr,
    buf: *const c_void,
    size: usize,
) -> usize {
    if size == 0 {
        return 0;
    }

    #[cfg(any(feature = "sdk-9-20", feature = "sdk-16-04"))]
    let stream = stream.cast::<VecOutStream>();
    #[cfg(any(feature = "sdk-19-00", feature = "sdk-23-01", feature = "sdk-26-00"))]
    let stream = stream.cast_mut().cast::<VecOutStream>();
    // SAFETY: The SDK guarantees that `buf` points to `size` initialized bytes for this callback.
    let bytes = unsafe { slice::from_raw_parts(buf.cast::<u8>(), size) };
    // SAFETY: `stream.output` points to the `Vec<u8>` borrowed for the lifetime of the stream wrapper.
    unsafe {
        (*(*stream).output).extend_from_slice(bytes);
    }
    size
}
