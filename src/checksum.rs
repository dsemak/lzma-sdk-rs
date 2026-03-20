//! Checksum and digest utilities backed by the upstream LZMA SDK implementations.

use std::mem::MaybeUninit;

/// Helper function to initialize SHA-256.
#[cfg(any(feature = "sdk-23-01", feature = "sdk-26-00"))]
fn initialize_sha256() {
    lzma_sdk_sys::initialize_sha256();
}

/// Helper function to initialize SHA-256.
#[cfg(any(feature = "sdk-9-20", feature = "sdk-16-04", feature = "sdk-19-00"))]
fn initialize_sha256() {
    // No initialization needed for older SDK versions.
}

/// Computes the CRC64 digest of a byte slice in a single call using the upstream SDK's implementation.
///
/// # Arguments
///
/// * `data` - The input byte slice to hash.
///
/// # Returns
///
/// A 64-bit CRC64 checksum of the data.
pub fn crc64(data: &[u8]) -> u64 {
    let mut hasher = Crc64::new();
    hasher.update(data);
    hasher.finish()
}

/// Computes the SHA-256 digest of a byte slice in a single call using the upstream SDK's implementation.
///
/// # Arguments
///
/// * `data` - The input byte slice to hash.
///
/// # Returns
///
/// The 32-byte SHA-256 hash of the data.
pub fn sha256(data: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hasher.finish()
}

/// Stateful/incremental CRC64 hasher backed by the LZMA SDK logic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Crc64 {
    /// The current CRC64 value (in-progress or finalized state).
    value: u64,
}

impl Crc64 {
    /// Creates a new CRC64 hasher, initializing the SDK tables if required.
    /// The starting value matches the SDK's expectations (`0xFFFF_FFFF_FFFF_FFFF`).
    ///
    /// # Returns
    ///
    /// A new [`Crc64`] hasher, ready for `update` calls.
    pub fn new() -> Self {
        crate::initialize_xz_crc_tables();
        Self { value: u64::MAX }
    }

    /// Update the CRC64 digest with additional data.
    ///
    /// # Arguments
    ///
    /// * `data` - The input byte slice to add to the hash calculation.
    ///
    /// Safe to call repeatedly with zero or more chunks.
    pub fn update(&mut self, data: &[u8]) {
        // SAFETY: `data` is a valid, live slice for the duration of the FFI call.
        self.value =
            unsafe { lzma_sdk_sys::Crc64Update(self.value, data.as_ptr().cast(), data.len()) };
    }

    /// Finalizes and returns the CRC64 digest for all previously-updated bytes.
    ///
    /// This method is idempotent and non-mutating. Returns the CRC64 checksum that matches
    /// the xz/LZMA/7-zip reference implementations.
    ///
    /// # Returns
    ///
    /// The finalized CRC64 value.
    pub fn finish(self) -> u64 {
        self.value ^ u64::MAX
    }
}

impl Default for Crc64 {
    fn default() -> Self {
        Self::new()
    }
}

/// Stateful/incremental SHA-256 hasher, backed by the LZMA SDK/7-zip implementation.
pub struct Sha256 {
    /// SDK-internal hasher state.
    state: lzma_sdk_sys::CSha256,
}

impl Sha256 {
    /// Creates a new SHA-256 hasher and initializes its internal state.
    ///
    /// # Returns
    ///
    /// A new [`Sha256`] ready for updates and digesting.
    pub fn new() -> Self {
        initialize_sha256();

        // SAFETY: We allocate zeroed state storage which is required by the SDK's init routine.
        let mut state = unsafe { MaybeUninit::<lzma_sdk_sys::CSha256>::zeroed().assume_init() };

        // SAFETY: `state` points to valid storage for the SDK hasher state and will be initialized by the SDK.
        unsafe {
            lzma_sdk_sys::Sha256_Init(&mut state);
        }
        Self { state }
    }

    /// Update the SHA-256 digest with a byte slice.
    ///
    /// # Arguments
    ///
    /// * `data` - The input data to hash.
    ///
    /// Can be called zero or more times. Input is not buffered.
    pub fn update(&mut self, data: &[u8]) {
        // SAFETY: `self.state` is fully initialized and `data` is a valid slice for FFI duration.
        unsafe {
            lzma_sdk_sys::Sha256_Update(&mut self.state, data.as_ptr(), data.len());
        }
    }

    /// Finalizes the SHA-256 computation and returns the 32-byte hash.
    ///
    /// This method consumes the hasher; after calling, no more `update` is allowed.
    ///
    /// # Returns
    ///
    /// A `[u8; 32]` array containing the SHA-256 hash.
    pub fn finish(mut self) -> [u8; 32] {
        let mut digest = [0_u8; 32];
        // SAFETY: `self.state` is initialized and `digest` points to 32 writable bytes.
        unsafe {
            lzma_sdk_sys::Sha256_Final(&mut self.state, digest.as_mut_ptr());
        }
        digest
    }
}

impl Default for Sha256 {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::{crc64, sha256, Crc64, Sha256};

    #[test]
    fn crc64_matches_incremental_digest() {
        let payload = b"checksum payload";
        let mut hasher = Crc64::new();
        hasher.update(&payload[..8]);
        hasher.update(&payload[8..]);
        assert_eq!(hasher.finish(), crc64(payload));
    }

    #[test]
    fn sha256_matches_known_vector() {
        let digest = sha256(b"abc");
        assert_eq!(
            digest,
            [
                0xba, 0x78, 0x16, 0xbf, 0x8f, 0x01, 0xcf, 0xea, 0x41, 0x41, 0x40, 0xde, 0x5d, 0xae,
                0x22, 0x23, 0xb0, 0x03, 0x61, 0xa3, 0x96, 0x17, 0x7a, 0x9c, 0xb4, 0x10, 0xff, 0x61,
                0xf2, 0x00, 0x15, 0xad,
            ]
        );
    }

    #[test]
    fn sha256_incremental_matches_one_shot() {
        let payload = b"split digest payload";
        let mut hasher = Sha256::new();
        hasher.update(&payload[..5]);
        hasher.update(&payload[5..]);
        assert_eq!(hasher.finish(), sha256(payload));
    }
}
