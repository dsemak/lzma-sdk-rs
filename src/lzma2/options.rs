//! Typed `LZMA2` encoder options and builder helpers.
//!
//! The wrapper embeds raw `LZMA` options and extends them with `LZMA2`-specific thread and block
//! controls exposed by the SDK.

use std::mem::MaybeUninit;

use crate::error::{invalid_option, Error};
use crate::LzmaOptions;

/// High-level `LZMA2` compression options.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    lzma: LzmaOptions,
    block_size: Option<u64>,
    num_block_threads_reduced: Option<u32>,
    num_block_threads_max: Option<u32>,
    num_total_threads: u32,
    num_thread_groups: Option<u32>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            lzma: LzmaOptions::default(),
            block_size: None,
            num_block_threads_reduced: None,
            num_block_threads_max: None,
            num_total_threads: Self::DEFAULT_NUM_TOTAL_THREADS,
            num_thread_groups: None,
        }
    }
}

impl Options {
    /// Minimum total thread count accepted by the wrapper.
    pub const MIN_NUM_TOTAL_THREADS: u32 = 1;
    /// Default total thread count used by the wrapper.
    pub const DEFAULT_NUM_TOTAL_THREADS: u32 = 1;

    /// Creates a builder for `LZMA2` compression options.
    pub fn builder() -> OptionsBuilder {
        OptionsBuilder {
            options: Self::default(),
        }
    }

    /// Replaces the nested raw `LZMA` options.
    pub fn lzma_options(mut self, lzma: LzmaOptions) -> Self {
        self.lzma = lzma;
        self
    }

    /// Replaces the nested raw `LZMA` options.
    pub fn with_lzma_options(self, lzma: LzmaOptions) -> Self {
        self.lzma_options(lzma)
    }

    /// Sets the optional block size in bytes.
    pub fn with_block_size(mut self, block_size: u64) -> Self {
        self.block_size = Some(block_size);
        self
    }

    /// Sets the reduced number of block threads.
    pub fn with_num_block_threads_reduced(mut self, value: u32) -> Self {
        self.num_block_threads_reduced = Some(value);
        self
    }

    /// Sets the maximum number of block threads.
    pub fn with_num_block_threads_max(mut self, value: u32) -> Self {
        self.num_block_threads_max = Some(value);
        self
    }

    /// Sets the total number of threads.
    ///
    /// Returns an error when `value` is smaller than [`Options::MIN_NUM_TOTAL_THREADS`].
    pub fn with_num_total_threads(mut self, value: u32) -> Result<Self, Error> {
        if value < Self::MIN_NUM_TOTAL_THREADS {
            return Err(invalid_option(
                "num_total_threads".into(),
                "expected a positive value".into(),
            ));
        }
        self.num_total_threads = value;
        Ok(self)
    }

    /// Sets the number of thread groups.
    pub fn with_num_thread_groups(mut self, value: u32) -> Self {
        self.num_thread_groups = Some(value);
        self
    }

    /// Converts these options into the raw SDK encoder properties structure.
    pub fn to_raw_props(&self) -> lzma_sdk_sys::CLzma2EncProps {
        let mut props = MaybeUninit::<lzma_sdk_sys::CLzma2EncProps>::zeroed();

        // SAFETY: `props` points to valid storage for the SDK to initialize with defaults.
        unsafe {
            lzma_sdk_sys::Lzma2EncProps_Init(props.as_mut_ptr());
        }

        // SAFETY: `Lzma2EncProps_Init` initialized all fields in the structure.
        let mut props = unsafe { props.assume_init() };

        props.lzmaProps = self.lzma.to_raw_encoder_props();

        if let Some(block_size) = self.block_size {
            #[cfg(any(feature = "sdk-9-20", feature = "sdk-16-04"))]
            let block_size = match usize::try_from(block_size) {
                Ok(value) => value,
                Err(_) => usize::MAX,
            };
            props.blockSize = block_size;
        }

        #[cfg(any(feature = "sdk-19-00", feature = "sdk-23-01", feature = "sdk-26-00"))]
        if let Some(num_block_threads_reduced) = self.num_block_threads_reduced {
            props.numBlockThreads_Reduced = num_block_threads_reduced as i32;
        }

        #[cfg(any(feature = "sdk-19-00", feature = "sdk-23-01", feature = "sdk-26-00"))]
        if let Some(num_block_threads_max) = self.num_block_threads_max {
            props.numBlockThreads_Max = num_block_threads_max as i32;
        }

        #[cfg(any(feature = "sdk-9-20", feature = "sdk-16-04"))]
        if let Some(num_block_threads) = self
            .num_block_threads_max
            .or(self.num_block_threads_reduced)
        {
            props.numBlockThreads = num_block_threads as i32;
        }

        props.numTotalThreads = self.num_total_threads as i32;

        #[cfg(feature = "sdk-26-00")]
        if let Some(num_thread_groups) = self.num_thread_groups {
            props.numThreadGroups = num_thread_groups;
        }

        props
    }
}

/// Builder for [`Options`].
pub struct OptionsBuilder {
    options: Options,
}

impl OptionsBuilder {
    /// Replaces the nested raw `LZMA` options.
    pub fn lzma_options(mut self, options: LzmaOptions) -> Self {
        self.options.lzma = options;
        self
    }

    /// Sets the optional block size in bytes.
    pub fn block_size(mut self, block_size: u64) -> Self {
        self.options.block_size = Some(block_size);
        self
    }

    /// Sets the reduced number of block threads.
    pub fn num_block_threads_reduced(mut self, value: u32) -> Self {
        self.options.num_block_threads_reduced = Some(value);
        self
    }

    /// Sets the maximum number of block threads.
    pub fn num_block_threads_max(mut self, value: u32) -> Self {
        self.options.num_block_threads_max = Some(value);
        self
    }

    /// Sets the total number of threads.
    ///
    /// Returns an error when `value` is smaller than [`Options::MIN_NUM_TOTAL_THREADS`].
    pub fn num_total_threads(mut self, value: u32) -> Result<Self, Error> {
        if value < Options::MIN_NUM_TOTAL_THREADS {
            return Err(invalid_option(
                "num_total_threads".into(),
                "expected a positive value".into(),
            ));
        }
        self.options.num_total_threads = value;
        Ok(self)
    }

    /// Sets the number of thread groups.
    pub fn num_thread_groups(mut self, value: u32) -> Self {
        self.options.num_thread_groups = Some(value);
        self
    }

    /// Builds the configured `LZMA2` options value.
    pub fn build(self) -> Options {
        self.options
    }
}

#[cfg(test)]
mod tests {
    use crate::lzma::EndMarkerMode;
    use crate::LzmaOptions;

    use super::Options;

    #[test]
    fn default_options_match_documented_defaults() {
        let options = Options::default();

        assert_eq!(options.lzma, LzmaOptions::default());
        assert_eq!(options.block_size, None);
        assert_eq!(options.num_block_threads_reduced, None);
        assert_eq!(options.num_block_threads_max, None);
        assert_eq!(
            options.num_total_threads,
            Options::DEFAULT_NUM_TOTAL_THREADS
        );
        assert_eq!(options.num_thread_groups, None);
    }

    #[test]
    fn builder_populates_raw_props() {
        let lzma = LzmaOptions::default().with_end_marker(EndMarkerMode::Enabled);
        let options = Options::builder()
            .lzma_options(lzma)
            .block_size(1 << 20)
            .num_block_threads_reduced(2)
            .num_block_threads_max(4)
            .num_total_threads(3)
            .unwrap()
            .num_thread_groups(5)
            .build();

        let props = options.to_raw_props();

        assert_eq!(props.blockSize, 1 << 20);
        #[cfg(any(feature = "sdk-19-00", feature = "sdk-23-01", feature = "sdk-26-00"))]
        assert_eq!(props.numBlockThreads_Reduced, 2);
        #[cfg(any(feature = "sdk-19-00", feature = "sdk-23-01", feature = "sdk-26-00"))]
        assert_eq!(props.numBlockThreads_Max, 4);
        #[cfg(any(feature = "sdk-9-20", feature = "sdk-16-04"))]
        assert_eq!(props.numBlockThreads, 4);
        assert_eq!(props.numTotalThreads, 3);
        #[cfg(feature = "sdk-26-00")]
        assert_eq!(props.numThreadGroups, 5);
        assert_eq!(props.lzmaProps.writeEndMark, EndMarkerMode::Enabled as u32);
    }

    #[test]
    fn rejects_zero_total_threads() {
        assert!(Options::default().with_num_total_threads(0).is_err());
        assert!(Options::builder().num_total_threads(0).is_err());
    }
}
