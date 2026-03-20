#![deny(unsafe_op_in_unsafe_fn)]
#![allow(non_camel_case_types, non_snake_case, non_upper_case_globals)]

include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

use std::ffi::c_void;
use std::ptr;
use std::sync::Once;

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

#[cfg(any(feature = "sdk-23-01", feature = "sdk-26-00"))]
pub fn initialize_sha256() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        unsafe {
            // SAFETY: The SDK documents this initializer as process-wide setup for SHA-256 dispatch.
            Sha256Prepare();
        }
    });
}

/// Frees memory allocated by the SDK default allocator.
///
/// # Safety
///
/// `address` must either be null or point to memory that was allocated by the
/// SDK through [`default_alloc`]. Passing any other pointer is undefined behavior.
pub unsafe fn free_with_default_alloc(address: *mut c_void) {
    if address.is_null() {
        return;
    }

    // SAFETY: `address` must have been allocated by the SDK through `default_alloc`.
    unsafe {
        default_sz_free(ptr::null_mut(), address);
    }
}
