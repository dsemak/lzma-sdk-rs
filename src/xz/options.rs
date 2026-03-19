//! Typed `XZ` encoder options and builder helpers.
//!
//! The wrapper combines nested `LZMA2` options with container-level integrity checks, optional
//! filter-chain configuration, and newer SDK threading controls when available.

use std::num::NonZeroU32;

use crate::error::Result;
#[cfg(any(
    feature = "sdk-9-20",
    feature = "sdk-16-04",
    feature = "sdk-19-00",
    feature = "sdk-23-01"
))]
use crate::Error;
use crate::Lzma2Options;

/// Integrity check used by the `XZ` container.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Check {
    /// Disable integrity checks.
    None,
    /// Use CRC32 integrity checks.
    Crc32,
    /// Use CRC64 integrity checks.
    Crc64,
    /// Use SHA-256 integrity checks.
    Sha256,
}

impl Check {
    #[cfg(any(
        feature = "sdk-16-04",
        feature = "sdk-19-00",
        feature = "sdk-23-01",
        feature = "sdk-26-00"
    ))]
    fn as_raw(self) -> u32 {
        match self {
            Self::None => lzma_sdk_sys::XZ_CHECK_NO,
            Self::Crc32 => lzma_sdk_sys::XZ_CHECK_CRC32,
            Self::Crc64 => lzma_sdk_sys::XZ_CHECK_CRC64,
            Self::Sha256 => lzma_sdk_sys::XZ_CHECK_SHA256,
        }
    }
}

#[cfg(feature = "sdk-16-04")]
#[derive(Debug)]
pub struct RawProps {
    lzma2_props: lzma_sdk_sys::CLzma2EncProps,
    filter_props: lzma_sdk_sys::CXzFilterProps,
    props: lzma_sdk_sys::CXzProps,
}

#[cfg(feature = "sdk-16-04")]
impl RawProps {
    pub(crate) fn as_raw(&self) -> *const lzma_sdk_sys::CXzProps {
        &self.props
    }
}

/// Optional pre-LZMA2 filter in the `XZ` filter chain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Filter {
    /// Use only the default `LZMA2` filter.
    None,
    /// Apply the delta filter before `LZMA2`.
    Delta { distance: u32 },
    /// Apply the x86 BCJ filter.
    X86,
    /// Apply the PowerPC BCJ filter.
    PowerPc,
    /// Apply the IA-64 BCJ filter.
    IA64,
    /// Apply the ARM BCJ filter.
    Arm,
    /// Apply the ARM Thumb BCJ filter.
    ArmThumb,
    /// Apply the SPARC BCJ filter.
    Sparc,
    /// Apply the ARM64 BCJ filter.
    Arm64,
    /// Apply the RISC-V BCJ filter.
    RiscV,
}

/// High-level `XZ` encoder options.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    lzma2: Lzma2Options,
    check: Check,
    filter: Filter,
    block_size: Option<u64>,
    num_thread_groups: Option<u32>,
    num_total_threads: Option<u32>,
    num_block_threads_reduced: Option<u32>,
    num_block_threads_max: Option<u32>,
    force_write_sizes_in_header: bool,
    reduce_size: Option<u64>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            lzma2: Lzma2Options::default(),
            check: Check::Crc32,
            filter: Filter::None,
            block_size: None,
            num_thread_groups: None,
            num_total_threads: None,
            num_block_threads_reduced: None,
            num_block_threads_max: None,
            force_write_sizes_in_header: false,
            reduce_size: None,
        }
    }
}

impl Options {
    /// Creates a builder for `XZ` compression options.
    pub fn builder() -> OptionsBuilder {
        OptionsBuilder {
            options: Self::default(),
        }
    }

    /// Replaces the nested `LZMA2` options.
    pub fn with_lzma2_options(mut self, options: Lzma2Options) -> Self {
        self.lzma2 = options;
        self
    }

    /// Sets the integrity check written into the container.
    pub fn with_check(mut self, check: Check) -> Self {
        self.check = check;
        self
    }

    /// Sets the optional pre-`LZMA2` filter.
    pub fn with_filter(mut self, filter: Filter) -> Self {
        self.filter = filter;
        self
    }

    /// Sets the block size in bytes.
    pub fn with_block_size(mut self, block_size: u64) -> Self {
        self.block_size = Some(block_size);
        self
    }

    /// Sets the number of thread groups.
    pub fn with_num_thread_groups(mut self, groups: u32) -> Self {
        self.num_thread_groups = Some(groups);
        self
    }

    /// Sets the total number of threads.
    pub fn with_num_total_threads(mut self, threads: NonZeroU32) -> Self {
        self.num_total_threads = Some(threads.get());
        self
    }

    /// Sets the reduced number of block threads.
    pub fn with_num_block_threads_reduced(mut self, threads: u32) -> Self {
        self.num_block_threads_reduced = Some(threads);
        self
    }

    /// Sets the maximum number of block threads.
    pub fn with_num_block_threads_max(mut self, threads: u32) -> Self {
        self.num_block_threads_max = Some(threads);
        self
    }

    /// Controls whether block sizes are forced into the `XZ` header.
    pub fn with_force_write_sizes_in_header(mut self, enabled: bool) -> Self {
        self.force_write_sizes_in_header = enabled;
        self
    }

    /// Hints the source size so the SDK can tune its internal settings.
    pub fn with_reduce_size(mut self, reduce_size: u64) -> Self {
        self.reduce_size = Some(reduce_size);
        self
    }

    /// Converts these options into the raw SDK encoder properties structure.
    #[cfg(feature = "sdk-9-20")]
    pub fn to_raw_props(&self) -> Result<lzma_sdk_sys::CLzma2EncProps> {
        if self.check != Check::Crc32 {
            return Err(Error::UnsupportedFeature(
                "custom XZ checks require sdk-16-04 or newer".into(),
            ));
        }

        if self.filter != Filter::None {
            return Err(Error::UnsupportedFeature(
                "custom XZ filters require sdk-16-04 or newer".into(),
            ));
        }

        if self.block_size.is_some()
            || self.num_thread_groups.is_some()
            || self.num_total_threads.is_some()
            || self.num_block_threads_reduced.is_some()
            || self.num_block_threads_max.is_some()
            || self.force_write_sizes_in_header
            || self.reduce_size.is_some()
        {
            return Err(Error::UnsupportedFeature(
                "advanced XZ props require sdk-19-00 or newer".into(),
            ));
        }

        Ok(self.lzma2.to_raw_props())
    }

    /// Converts these options into the raw SDK encoder properties structure.
    #[cfg(feature = "sdk-16-04")]
    pub fn to_raw_props(&self) -> Result<RawProps> {
        if self.block_size.is_some()
            || self.num_thread_groups.is_some()
            || self.num_total_threads.is_some()
            || self.num_block_threads_reduced.is_some()
            || self.num_block_threads_max.is_some()
            || self.force_write_sizes_in_header
            || self.reduce_size.is_some()
        {
            return Err(Error::UnsupportedFeature(
                "advanced XZ props require sdk-19-00 or newer".into(),
            ));
        }

        let mut filter_props = std::mem::MaybeUninit::<lzma_sdk_sys::CXzFilterProps>::zeroed();

        // SAFETY: `filter_props` points to valid storage for the SDK to initialize with defaults.
        unsafe {
            lzma_sdk_sys::XzFilterProps_Init(filter_props.as_mut_ptr());
        }

        // SAFETY: `XzFilterProps_Init` populated the struct with valid defaults.
        let mut filter_props = unsafe { filter_props.assume_init() };

        let has_filter = match self.filter {
            Filter::None => false,
            Filter::Delta { distance } => {
                filter_props.id = lzma_sdk_sys::XZ_ID_Delta;
                filter_props.delta = distance;
                true
            }
            Filter::X86 => {
                filter_props.id = lzma_sdk_sys::XZ_ID_X86;
                true
            }
            Filter::PowerPc => {
                filter_props.id = lzma_sdk_sys::XZ_ID_PPC;
                true
            }
            Filter::IA64 => {
                filter_props.id = lzma_sdk_sys::XZ_ID_IA64;
                true
            }
            Filter::Arm => {
                filter_props.id = lzma_sdk_sys::XZ_ID_ARM;
                true
            }
            Filter::ArmThumb => {
                filter_props.id = lzma_sdk_sys::XZ_ID_ARMT;
                true
            }
            Filter::Sparc => {
                filter_props.id = lzma_sdk_sys::XZ_ID_SPARC;
                true
            }
            Filter::Arm64 => {
                return Err(Error::UnsupportedFeature(
                    "ARM64 XZ filters require sdk-23-01 or newer".into(),
                ));
            }
            Filter::RiscV => {
                return Err(Error::UnsupportedFeature(
                    "RISC-V XZ filters require sdk-26-00 or newer".into(),
                ));
            }
        };

        let mut props = std::mem::MaybeUninit::<lzma_sdk_sys::CXzProps>::zeroed();

        // SAFETY: `props` is valid storage for the SDK to initialize with defaults.
        unsafe {
            lzma_sdk_sys::XzProps_Init(props.as_mut_ptr());
        }

        // SAFETY: `XzProps_Init` populated the struct with valid defaults.
        let props = unsafe { props.assume_init() };

        let mut raw = RawProps {
            lzma2_props: self.lzma2.to_raw_props(),
            filter_props,
            props,
        };
        raw.props.lzma2Props = &raw.lzma2_props;
        raw.props.filterProps = if has_filter {
            &raw.filter_props
        } else {
            std::ptr::null()
        };
        raw.props.checkId = self.check.as_raw();

        Ok(raw)
    }

    /// Converts these options into the raw SDK encoder properties structure.
    #[cfg(any(feature = "sdk-19-00", feature = "sdk-23-01", feature = "sdk-26-00"))]
    pub fn to_raw_props(&self) -> Result<lzma_sdk_sys::CXzProps> {
        let mut props = std::mem::MaybeUninit::<lzma_sdk_sys::CXzProps>::zeroed();

        // SAFETY: `props` is valid storage for the SDK to initialize with defaults.
        unsafe {
            lzma_sdk_sys::XzProps_Init(props.as_mut_ptr());
        }

        // SAFETY: `XzProps_Init` populated the struct with valid defaults.
        let mut props = unsafe { props.assume_init() };

        props.lzma2Props = self.lzma2.to_raw_props();
        props.checkId = self.check.as_raw();

        if let Some(block_size) = self.block_size {
            props.blockSize = block_size;
        }

        #[cfg(feature = "sdk-26-00")]
        if let Some(groups) = self.num_thread_groups {
            props.numThreadGroups = groups;
        }

        #[cfg(any(feature = "sdk-19-00", feature = "sdk-23-01"))]
        if self.num_thread_groups.is_some() {
            return Err(Error::UnsupportedFeature(
                "XZ thread groups require sdk-26-00 or newer".into(),
            ));
        }

        if let Some(threads) = self.num_total_threads {
            props.numTotalThreads = threads as i32;
        }

        if let Some(threads) = self.num_block_threads_reduced {
            props.numBlockThreads_Reduced = threads as i32;
        }

        if let Some(threads) = self.num_block_threads_max {
            props.numBlockThreads_Max = threads as i32;
        }

        if self.force_write_sizes_in_header {
            props.forceWriteSizesInHeader = 1;
        }

        if let Some(reduce_size) = self.reduce_size {
            props.reduceSize = reduce_size;
        }

        // SAFETY: `props` points to a live filter props struct that the SDK initializes in place.
        unsafe {
            lzma_sdk_sys::XzFilterProps_Init(&mut props.filterProps);
        }

        match self.filter {
            Filter::None => {}
            Filter::Delta { distance } => {
                props.filterProps.id = lzma_sdk_sys::XZ_ID_Delta;
                props.filterProps.delta = distance;
            }
            Filter::X86 => props.filterProps.id = lzma_sdk_sys::XZ_ID_X86,
            Filter::PowerPc => props.filterProps.id = lzma_sdk_sys::XZ_ID_PPC,
            Filter::IA64 => props.filterProps.id = lzma_sdk_sys::XZ_ID_IA64,
            Filter::Arm => props.filterProps.id = lzma_sdk_sys::XZ_ID_ARM,
            Filter::ArmThumb => props.filterProps.id = lzma_sdk_sys::XZ_ID_ARMT,
            Filter::Sparc => props.filterProps.id = lzma_sdk_sys::XZ_ID_SPARC,
            #[cfg(any(feature = "sdk-23-01", feature = "sdk-26-00"))]
            Filter::Arm64 => props.filterProps.id = lzma_sdk_sys::XZ_ID_ARM64,
            #[cfg(any(feature = "sdk-19-00", feature = "sdk-16-04"))]
            Filter::Arm64 => {
                return Err(Error::UnsupportedFeature(
                    "ARM64 XZ filters require sdk-23-01 or newer".into(),
                ));
            }
            #[cfg(feature = "sdk-26-00")]
            Filter::RiscV => props.filterProps.id = lzma_sdk_sys::XZ_ID_RISCV,
            #[cfg(any(feature = "sdk-19-00", feature = "sdk-23-01", feature = "sdk-16-04"))]
            Filter::RiscV => {
                return Err(Error::UnsupportedFeature(
                    "RISC-V XZ filters require sdk-26-00 or newer".into(),
                ));
            }
        }

        Ok(props)
    }
}

/// Builder for [`Options`].
pub struct OptionsBuilder {
    options: Options,
}

impl OptionsBuilder {
    /// Replaces the nested `LZMA2` options.
    pub fn lzma2_options(mut self, options: Lzma2Options) -> Self {
        self.options.lzma2 = options;
        self
    }

    /// Sets the integrity check written into the container.
    pub fn check(mut self, check: Check) -> Self {
        self.options.check = check;
        self
    }

    /// Sets the optional pre-`LZMA2` filter.
    pub fn filter(mut self, filter: Filter) -> Self {
        self.options.filter = filter;
        self
    }

    /// Sets the block size in bytes.
    pub fn block_size(mut self, block_size: u64) -> Self {
        self.options.block_size = Some(block_size);
        self
    }

    /// Sets the number of thread groups.
    pub fn num_thread_groups(mut self, groups: u32) -> Self {
        self.options.num_thread_groups = Some(groups);
        self
    }

    /// Sets the total number of threads.
    pub fn num_total_threads(mut self, threads: NonZeroU32) -> Self {
        self.options.num_total_threads = Some(threads.get());
        self
    }

    /// Sets the reduced number of block threads.
    pub fn num_block_threads_reduced(mut self, threads: u32) -> Self {
        self.options.num_block_threads_reduced = Some(threads);
        self
    }

    /// Sets the maximum number of block threads.
    pub fn num_block_threads_max(mut self, threads: u32) -> Self {
        self.options.num_block_threads_max = Some(threads);
        self
    }

    /// Controls whether block sizes are forced into the `XZ` header.
    pub fn force_write_sizes_in_header(mut self, enabled: bool) -> Self {
        self.options.force_write_sizes_in_header = enabled;
        self
    }

    /// Hints the source size so the SDK can tune its internal settings.
    pub fn reduce_size(mut self, reduce_size: u64) -> Self {
        self.options.reduce_size = Some(reduce_size);
        self
    }

    /// Returns the fully configured `XZ` options value.
    pub fn build(self) -> Options {
        self.options
    }
}

#[cfg(test)]
mod tests {
    #[cfg(any(feature = "sdk-23-01", feature = "sdk-26-00"))]
    use std::num::NonZeroU32;

    #[cfg(any(
        feature = "sdk-9-20",
        feature = "sdk-16-04",
        feature = "sdk-19-00",
        feature = "sdk-23-01"
    ))]
    use crate::Error;

    use super::{Check, Filter, Options};

    #[test]
    fn default_options_match_documented_defaults() {
        let options = Options::default();

        assert_eq!(options.lzma2, crate::Lzma2Options::default());
        assert_eq!(options.check, Check::Crc32);
        assert_eq!(options.filter, Filter::None);
        assert_eq!(options.block_size, None);
        assert_eq!(options.num_thread_groups, None);
        assert_eq!(options.num_total_threads, None);
        assert_eq!(options.num_block_threads_reduced, None);
        assert_eq!(options.num_block_threads_max, None);
        assert!(!options.force_write_sizes_in_header);
        assert_eq!(options.reduce_size, None);
    }

    #[cfg(feature = "sdk-9-20")]
    #[test]
    fn sdk_9_20_rejects_newer_xz_features() {
        let unsupported_check = Options::default().with_check(Check::Sha256).to_raw_props();
        assert_eq!(
            unsupported_check.unwrap_err(),
            Error::UnsupportedFeature("custom XZ checks require sdk-16-04 or newer".into())
        );

        let unsupported_filter = Options::default()
            .with_filter(Filter::Delta { distance: 4 })
            .to_raw_props();
        assert_eq!(
            unsupported_filter.unwrap_err(),
            Error::UnsupportedFeature("custom XZ filters require sdk-16-04 or newer".into())
        );

        let unsupported_advanced = Options::default().with_block_size(1 << 20).to_raw_props();
        assert_eq!(
            unsupported_advanced.unwrap_err(),
            Error::UnsupportedFeature("advanced XZ props require sdk-19-00 or newer".into())
        );
    }

    #[cfg(feature = "sdk-16-04")]
    #[test]
    fn sdk_16_04_rejects_newer_xz_features() {
        let unsupported_advanced = Options::default().with_block_size(1 << 20).to_raw_props();
        assert_eq!(
            unsupported_advanced.unwrap_err(),
            Error::UnsupportedFeature("advanced XZ props require sdk-19-00 or newer".into())
        );

        let unsupported_arm64 = Options::default().with_filter(Filter::Arm64).to_raw_props();
        assert_eq!(
            unsupported_arm64.unwrap_err(),
            Error::UnsupportedFeature("ARM64 XZ filters require sdk-23-01 or newer".into())
        );

        let unsupported_riscv = Options::default().with_filter(Filter::RiscV).to_raw_props();
        assert_eq!(
            unsupported_riscv.unwrap_err(),
            Error::UnsupportedFeature("RISC-V XZ filters require sdk-26-00 or newer".into())
        );
    }

    #[cfg(feature = "sdk-19-00")]
    #[test]
    fn sdk_19_00_rejects_newer_xz_features() {
        let unsupported_arm64 = Options::default().with_filter(Filter::Arm64).to_raw_props();
        assert_eq!(
            unsupported_arm64.unwrap_err(),
            Error::UnsupportedFeature("ARM64 XZ filters require sdk-23-01 or newer".into())
        );

        let unsupported_riscv = Options::default().with_filter(Filter::RiscV).to_raw_props();
        assert_eq!(
            unsupported_riscv.unwrap_err(),
            Error::UnsupportedFeature("RISC-V XZ filters require sdk-26-00 or newer".into())
        );

        let unsupported_thread_groups = Options::default().with_num_thread_groups(2).to_raw_props();
        assert_eq!(
            unsupported_thread_groups.unwrap_err(),
            Error::UnsupportedFeature("XZ thread groups require sdk-26-00 or newer".into())
        );
    }

    #[cfg(feature = "sdk-23-01")]
    #[test]
    fn sdk_23_01_maps_xz_properties_without_thread_groups() {
        let options = Options::builder()
            .check(Check::Sha256)
            .filter(Filter::Delta { distance: 8 })
            .block_size(1 << 20)
            .num_total_threads(NonZeroU32::new(3).unwrap())
            .num_block_threads_reduced(4)
            .num_block_threads_max(5)
            .force_write_sizes_in_header(true)
            .reduce_size(1234)
            .build();

        let props = options.to_raw_props().unwrap();

        assert_eq!(props.checkId, lzma_sdk_sys::XZ_CHECK_SHA256);
        assert_eq!(props.blockSize, 1 << 20);
        assert_eq!(props.numTotalThreads, 3);
        assert_eq!(props.numBlockThreads_Reduced, 4);
        assert_eq!(props.numBlockThreads_Max, 5);
        assert_eq!(props.forceWriteSizesInHeader, 1);
        assert_eq!(props.reduceSize, 1234);
        assert_eq!(props.filterProps.id, lzma_sdk_sys::XZ_ID_Delta);
        assert_eq!(props.filterProps.delta, 8);

        let unsupported_thread_groups = Options::default().with_num_thread_groups(2).to_raw_props();
        assert_eq!(
            unsupported_thread_groups.unwrap_err(),
            Error::UnsupportedFeature("XZ thread groups require sdk-26-00 or newer".into())
        );
    }

    #[cfg(feature = "sdk-26-00")]
    #[test]
    fn sdk_26_00_maps_xz_properties() {
        let options = Options::builder()
            .check(Check::Sha256)
            .filter(Filter::Delta { distance: 8 })
            .block_size(1 << 20)
            .num_thread_groups(2)
            .num_total_threads(NonZeroU32::new(3).unwrap())
            .num_block_threads_reduced(4)
            .num_block_threads_max(5)
            .force_write_sizes_in_header(true)
            .reduce_size(1234)
            .build();

        let props = options.to_raw_props().unwrap();

        assert_eq!(props.checkId, lzma_sdk_sys::XZ_CHECK_SHA256);
        assert_eq!(props.blockSize, 1 << 20);
        assert_eq!(props.numThreadGroups, 2);
        assert_eq!(props.numTotalThreads, 3);
        assert_eq!(props.numBlockThreads_Reduced, 4);
        assert_eq!(props.numBlockThreads_Max, 5);
        assert_eq!(props.forceWriteSizesInHeader, 1);
        assert_eq!(props.reduceSize, 1234);
        assert_eq!(props.filterProps.id, lzma_sdk_sys::XZ_ID_Delta);
        assert_eq!(props.filterProps.delta, 8);
    }
}
