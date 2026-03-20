//! Standalone pre/post-processing filters for executable code and data transformations.
//!
//! This module provides building blocks for in-place code and binary data transformation,
//! including support for stateful branch conversion (for various CPU architectures) and
//! delta filters (typically used for compressing image or other structured data).

use crate::error::{invalid_option, Result};

/// BranchMode indicates whether an associated branch conversion filter transforms code for compression
/// (Encode) or restores it after decompression (Decode).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BranchMode {
    /// Transform executable code before compression or storage to improve compressibility.
    Encode,

    /// Restore the original executable code after decompression or retrieval.
    Decode,
}

/// BranchFilter denotes the specific family of executable code (CPU ISA) for which branch
/// conversion should be applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BranchFilter {
    /// x86 and x86_64 instruction set (Intel/AMD/compatible CPUs)
    X86,
    /// ARM 32-bit mode (A32)
    Arm,
    /// ARM Thumb mode (T16/T32)
    ArmThumb,
    /// 64-bit ARM architecture (AArch64)
    Arm64,
    /// PowerPC family (PPC32/PPC64)
    PowerPc,
    /// SPARC instruction set
    Sparc,
    /// IA-64 (Intel Itanium)
    Ia64,
    /// RISC-V instruction set
    RiscV,
}

/// BranchConverter implements a stateful, incremental branch instruction code transformer for
/// various architectures. Used for branch conversion filters that operate in-place on byte buffers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BranchConverter {
    /// The target CPU instruction set architecture for branch conversion.
    filter: BranchFilter,
    /// The direction of transformation.
    mode: BranchMode,
    /// Virtual program counter at the start of this chunk.
    pc: u32,
    /// x86 branch converter internal state (ignored for non-x86).
    x86_state: u32,
}

impl BranchConverter {
    /// Construct a new [`BranchConverter`] for the provided architecture, conversion direction,
    /// and starting virtual address.
    ///
    /// # Arguments
    ///
    /// * `filter` - The desired CPU ISA transformation (see [`BranchFilter`]).
    /// * `mode` - Conversion direction: encode or decode (see [`BranchMode`]).
    /// * `start_pc` - Virtual instruction pointer to use as the base for code relative offsets.
    ///
    /// # Returns
    /// A new `BranchConverter` with clean internal state, ready for use.
    pub fn new(filter: BranchFilter, mode: BranchMode, start_pc: u32) -> Self {
        Self {
            filter,
            mode,
            pc: start_pc,
            x86_state: 0,
        }
    }

    /// Reset the converter's internal state and program counter to the beginning of a new stream or chunk.
    ///
    /// # Arguments
    ///
    /// * `start_pc` - The instruction pointer to use as a new base for future conversions.
    pub fn reset(&mut self, start_pc: u32) {
        self.pc = start_pc;
        self.x86_state = 0;
    }

    /// Perform in-place branch transformation on a buffer slice using the current state of the converter,
    /// updating the internal instruction pointer as appropriate.
    ///
    /// # Arguments
    ///
    /// * `data` - The mutable byte slice containing machine code to transform.
    ///
    /// # Returns
    ///
    /// The number of bytes processed (may be less than input length if an incomplete instruction was left at end).
    pub fn transform_in_place(&mut self, data: &mut [u8]) -> usize {
        if data.is_empty() {
            return 0;
        }

        let processed = match (self.filter, self.mode) {
            (BranchFilter::X86, BranchMode::Encode) => {
                convert_x86(data, self.pc, &mut self.x86_state, true)
            }
            (BranchFilter::X86, BranchMode::Decode) => {
                convert_x86(data, self.pc, &mut self.x86_state, false)
            }
            (BranchFilter::Arm, BranchMode::Encode) => {
                convert_branch(data, self.pc, BranchFilter::Arm, true)
            }
            (BranchFilter::Arm, BranchMode::Decode) => {
                convert_branch(data, self.pc, BranchFilter::Arm, false)
            }
            (BranchFilter::ArmThumb, BranchMode::Encode) => {
                convert_branch(data, self.pc, BranchFilter::ArmThumb, true)
            }
            (BranchFilter::ArmThumb, BranchMode::Decode) => {
                convert_branch(data, self.pc, BranchFilter::ArmThumb, false)
            }
            (BranchFilter::PowerPc, BranchMode::Encode) => {
                convert_branch(data, self.pc, BranchFilter::PowerPc, true)
            }
            (BranchFilter::PowerPc, BranchMode::Decode) => {
                convert_branch(data, self.pc, BranchFilter::PowerPc, false)
            }
            (BranchFilter::Sparc, BranchMode::Encode) => {
                convert_branch(data, self.pc, BranchFilter::Sparc, true)
            }
            (BranchFilter::Sparc, BranchMode::Decode) => {
                convert_branch(data, self.pc, BranchFilter::Sparc, false)
            }
            (BranchFilter::Ia64, BranchMode::Encode) => {
                convert_branch(data, self.pc, BranchFilter::Ia64, true)
            }
            (BranchFilter::Ia64, BranchMode::Decode) => {
                convert_branch(data, self.pc, BranchFilter::Ia64, false)
            }
            #[cfg(any(feature = "sdk-23-01", feature = "sdk-26-00"))]
            (BranchFilter::Arm64, BranchMode::Encode) => {
                convert_branch(data, self.pc, BranchFilter::Arm64, true)
            }
            #[cfg(any(feature = "sdk-23-01", feature = "sdk-26-00"))]
            (BranchFilter::Arm64, BranchMode::Decode) => {
                convert_branch(data, self.pc, BranchFilter::Arm64, false)
            }
            #[cfg(any(feature = "sdk-9-20", feature = "sdk-16-04", feature = "sdk-19-00"))]
            (BranchFilter::Arm64, _) => {
                unsupported_branch_filter("ARM64 branch filters require sdk-23-01 or newer")
            }
            #[cfg(feature = "sdk-26-00")]
            (BranchFilter::RiscV, BranchMode::Encode) => {
                convert_branch(data, self.pc, BranchFilter::RiscV, true)
            }
            #[cfg(feature = "sdk-26-00")]
            (BranchFilter::RiscV, BranchMode::Decode) => {
                convert_branch(data, self.pc, BranchFilter::RiscV, false)
            }
            #[cfg(any(
                feature = "sdk-9-20",
                feature = "sdk-16-04",
                feature = "sdk-19-00",
                feature = "sdk-23-01"
            ))]
            (BranchFilter::RiscV, _) => {
                unsupported_branch_filter("RISC-V branch filters require sdk-26-00 or newer")
            }
        };

        self.pc = self.pc.wrapping_add(processed as u32);

        processed
    }
}

/// Helper function to convert x86 branch instructions.
#[cfg(any(feature = "sdk-9-20", feature = "sdk-16-04", feature = "sdk-19-00"))]
fn convert_x86(data: &mut [u8], pc: u32, state: &mut u32, encode: bool) -> usize {
    unsafe {
        lzma_sdk_sys::x86_Convert(data.as_mut_ptr(), data.len(), pc, state, i32::from(encode))
    }
}

/// Helper function to convert x86 branch instructions.
#[cfg(any(feature = "sdk-23-01", feature = "sdk-26-00"))]
fn convert_x86(data: &mut [u8], pc: u32, state: &mut u32, encode: bool) -> usize {
    unsafe {
        let end = if encode {
            lzma_sdk_sys::z7_BranchConvSt_X86_Enc(data.as_mut_ptr(), data.len(), pc, state)
        } else {
            lzma_sdk_sys::z7_BranchConvSt_X86_Dec(data.as_mut_ptr(), data.len(), pc, state)
        };
        ptr_diff(data.as_mut_ptr(), end)
    }
}

/// Helper function to convert branch instructions for other architectures.
#[cfg(any(feature = "sdk-9-20", feature = "sdk-16-04", feature = "sdk-19-00"))]
fn convert_branch(data: &mut [u8], pc: u32, filter: BranchFilter, encode: bool) -> usize {
    let encoding = i32::from(encode);
    unsafe {
        match filter {
            BranchFilter::Arm => {
                lzma_sdk_sys::ARM_Convert(data.as_mut_ptr(), data.len(), pc, encoding)
            }
            BranchFilter::ArmThumb => {
                lzma_sdk_sys::ARMT_Convert(data.as_mut_ptr(), data.len(), pc, encoding)
            }
            BranchFilter::PowerPc => {
                lzma_sdk_sys::PPC_Convert(data.as_mut_ptr(), data.len(), pc, encoding)
            }
            BranchFilter::Sparc => {
                lzma_sdk_sys::SPARC_Convert(data.as_mut_ptr(), data.len(), pc, encoding)
            }
            BranchFilter::Ia64 => {
                lzma_sdk_sys::IA64_Convert(data.as_mut_ptr(), data.len(), pc, encoding)
            }
            BranchFilter::Arm64 => {
                unsupported_branch_filter("ARM64 branch filters require sdk-23-01 or newer")
            }
            BranchFilter::RiscV => {
                unsupported_branch_filter("RISC-V branch filters require sdk-26-00 or newer")
            }
            BranchFilter::X86 => unreachable!("x86 conversion is handled separately"),
        }
    }
}

/// Helper function to convert branch instructions for other architectures.
#[cfg(any(feature = "sdk-23-01", feature = "sdk-26-00"))]
fn convert_branch(data: &mut [u8], pc: u32, filter: BranchFilter, encode: bool) -> usize {
    unsafe {
        let end = match (filter, encode) {
            (BranchFilter::Arm64, true) => {
                lzma_sdk_sys::z7_BranchConv_ARM64_Enc(data.as_mut_ptr(), data.len(), pc)
            }
            (BranchFilter::Arm64, false) => {
                lzma_sdk_sys::z7_BranchConv_ARM64_Dec(data.as_mut_ptr(), data.len(), pc)
            }
            (BranchFilter::Arm, true) => {
                lzma_sdk_sys::z7_BranchConv_ARM_Enc(data.as_mut_ptr(), data.len(), pc)
            }
            (BranchFilter::Arm, false) => {
                lzma_sdk_sys::z7_BranchConv_ARM_Dec(data.as_mut_ptr(), data.len(), pc)
            }
            (BranchFilter::ArmThumb, true) => {
                lzma_sdk_sys::z7_BranchConv_ARMT_Enc(data.as_mut_ptr(), data.len(), pc)
            }
            (BranchFilter::ArmThumb, false) => {
                lzma_sdk_sys::z7_BranchConv_ARMT_Dec(data.as_mut_ptr(), data.len(), pc)
            }
            (BranchFilter::PowerPc, true) => {
                lzma_sdk_sys::z7_BranchConv_PPC_Enc(data.as_mut_ptr(), data.len(), pc)
            }
            (BranchFilter::PowerPc, false) => {
                lzma_sdk_sys::z7_BranchConv_PPC_Dec(data.as_mut_ptr(), data.len(), pc)
            }
            (BranchFilter::Sparc, true) => {
                lzma_sdk_sys::z7_BranchConv_SPARC_Enc(data.as_mut_ptr(), data.len(), pc)
            }
            (BranchFilter::Sparc, false) => {
                lzma_sdk_sys::z7_BranchConv_SPARC_Dec(data.as_mut_ptr(), data.len(), pc)
            }
            (BranchFilter::Ia64, true) => {
                lzma_sdk_sys::z7_BranchConv_IA64_Enc(data.as_mut_ptr(), data.len(), pc)
            }
            (BranchFilter::Ia64, false) => {
                lzma_sdk_sys::z7_BranchConv_IA64_Dec(data.as_mut_ptr(), data.len(), pc)
            }
            #[cfg(feature = "sdk-26-00")]
            (BranchFilter::RiscV, true) => {
                lzma_sdk_sys::z7_BranchConv_RISCV_Enc(data.as_mut_ptr(), data.len(), pc)
            }
            #[cfg(feature = "sdk-26-00")]
            (BranchFilter::RiscV, false) => {
                lzma_sdk_sys::z7_BranchConv_RISCV_Dec(data.as_mut_ptr(), data.len(), pc)
            }
            #[cfg(feature = "sdk-23-01")]
            (BranchFilter::RiscV, _) => {
                unsupported_branch_filter("RISC-V branch filters require sdk-26-00 or newer")
            }
            (BranchFilter::X86, _) => unreachable!("x86 conversion is handled separately"),
        };
        ptr_diff(data.as_mut_ptr(), end)
    }
}

/// Helper function to report unsupported branch filters.
#[cfg(not(feature = "sdk-26-00"))]
fn unsupported_branch_filter(message: &str) -> ! {
    panic!("{message}")
}

/// DeltaFilter implements a general stateful delta filter for binary data, useful for
/// data that exhibits repeatable values at regular intervals (e.g., image, sound samples).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeltaFilter {
    /// Byte distance (delta order) for the filter, must be in 1..=256.
    distance: u32,
    /// SDK-managed running state, always 256 bytes as required by the LZMA SDK.
    state: [u8; 256],
}

impl DeltaFilter {
    /// Construct a new delta filter for the requested byte distance/order.
    ///
    /// # Arguments
    ///
    /// * `distance` - Number of bytes to look back (offset) in the delta operation.
    ///
    /// # Returns
    ///
    /// A new [`DeltaFilter`] with the requested byte distance.
    ///
    /// # Errors
    ///
    /// Returns an error if the supplied `distance` is outside the valid range.
    pub fn new(distance: u32) -> Result<Self> {
        if !(1..=256).contains(&distance) {
            return Err(invalid_option(
                "distance".to_string(),
                "delta distance must be in 1..=256".to_string(),
            ));
        }

        // Pre-clear state and call SDK initializer.
        let mut filter = Self {
            distance,
            state: [0_u8; 256],
        };
        filter.reset();
        Ok(filter)
    }

    /// Reset the delta filter's internal state to its initial condition.
    /// Call this before beginning to encode/decode a new stream or after using different input.
    ///
    /// After reset, the history buffer is cleared and the filter will treat all new chunks as independent.
    pub fn reset(&mut self) {
        // SAFETY: `self.state` points to the exact 256-byte state buffer required by the SDK.
        unsafe {
            lzma_sdk_sys::Delta_Init(self.state.as_mut_ptr());
        }
    }

    /// Apply delta encoding on the provided data buffer, mutating the buffer in place.
    ///
    /// The encoding is cumulative and depends on the internal state, so the filter can
    /// be used with chunks split anywhere within the data stream.
    ///
    /// # Arguments
    ///
    /// * `data` - Mutable byte slice to delta-encode.
    pub fn encode_in_place(&mut self, data: &mut [u8]) {
        // SAFETY: The SDK mutates the provided byte slice in place and the state buffer is valid.
        unsafe {
            lzma_sdk_sys::Delta_Encode(
                self.state.as_mut_ptr(),
                self.distance,
                data.as_mut_ptr(),
                data.len(),
            );
        }
    }

    /// Apply delta decoding on the provided data buffer, mutating the buffer in place.
    ///
    /// The decoding is cumulative and depends on the internal state, so the filter can
    /// handle data in multiple chunks processed in sequence.
    ///
    /// # Arguments
    ///
    /// * `data` - Mutable byte slice to delta-decode.
    pub fn decode_in_place(&mut self, data: &mut [u8]) {
        // SAFETY: The SDK mutates the provided byte slice in place and the state buffer is valid.
        unsafe {
            lzma_sdk_sys::Delta_Decode(
                self.state.as_mut_ptr(),
                self.distance,
                data.as_mut_ptr(),
                data.len(),
            );
        }
    }
}

/// Calculate the offset (number of bytes) between two pointers.
///
/// Used internally to determine how many bytes were processed by the SDK's in-place
/// transform functions, which return a pointer to the position after the last processed byte.
///
/// # Safety
///
/// Both pointers must belong to the same allocated object or slice.
#[cfg(any(feature = "sdk-23-01", feature = "sdk-26-00"))]
fn ptr_diff(start: *mut u8, end: *mut u8) -> usize {
    (end as usize).saturating_sub(start as usize)
}

#[cfg(test)]
mod tests {
    use super::{BranchConverter, BranchFilter, BranchMode, DeltaFilter};

    #[test]
    fn delta_round_trips_in_place() {
        let original = b"delta filter payload".repeat(8);
        let mut encoded = original.clone();
        let mut encoder = DeltaFilter::new(4).unwrap();
        encoder.encode_in_place(&mut encoded);
        assert_ne!(encoded, original);

        let mut decoder = DeltaFilter::new(4).unwrap();
        decoder.decode_in_place(&mut encoded);
        assert_eq!(encoded, original);
    }

    #[test]
    fn x86_branch_filter_round_trips() {
        let original = vec![
            0xE8, 0x00, 0x00, 0x00, 0x00, 0xC3, 0xE9, 0x00, 0x00, 0x00, 0x00,
        ];
        let mut encoded = original.clone();

        let mut encoder = BranchConverter::new(BranchFilter::X86, BranchMode::Encode, 0);
        let processed = encoder.transform_in_place(&mut encoded);
        assert!(processed != 0);

        let mut decoder = BranchConverter::new(BranchFilter::X86, BranchMode::Decode, 0);
        decoder.transform_in_place(&mut encoded);
        assert_eq!(encoded, original);
    }

    #[cfg(any(feature = "sdk-9-20", feature = "sdk-16-04", feature = "sdk-19-00"))]
    #[test]
    #[should_panic(expected = "ARM64 branch filters require sdk-23-01 or newer")]
    fn arm64_filter_reports_unsupported_sdk() {
        let mut converter = BranchConverter::new(BranchFilter::Arm64, BranchMode::Encode, 0);
        let mut data = [0_u8; 4];
        converter.transform_in_place(&mut data);
    }

    #[cfg(any(
        feature = "sdk-9-20",
        feature = "sdk-16-04",
        feature = "sdk-19-00",
        feature = "sdk-23-01"
    ))]
    #[test]
    #[should_panic(expected = "RISC-V branch filters require sdk-26-00 or newer")]
    fn riscv_filter_reports_unsupported_sdk() {
        let mut converter = BranchConverter::new(BranchFilter::RiscV, BranchMode::Encode, 0);
        let mut data = [0_u8; 8];
        converter.transform_in_place(&mut data);
    }
}
