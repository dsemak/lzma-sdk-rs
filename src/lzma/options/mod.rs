//! Typed high-level and low-level raw `LZMA` encoder options and builder helpers.

use std::mem::MaybeUninit;
use std::num::NonZeroU32;

use crate::error::{invalid_option, Error};

mod fb;
mod lc;
mod level;
mod lp;
mod pb;

pub use fb::FastBytes;
pub use lc::LiteralContextBits;
pub use level::CompressionLevel;
pub use lp::LiteralPosBits;
pub use pb::PosBits;

/// Minimum dictionary size supported by the encoder.
pub const MIN_DICT_SIZE: u32 = 1 << 12;

/// Encoder algorithm selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum Algorithm {
    /// Favor encoding speed over ratio.
    Fast = 0,
    /// Favor compression ratio over speed.
    Normal = 1,
}

/// Match finder strategy used by the encoder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum MatchFinderMode {
    /// Use a hash-chain match finder.
    HashChain = 0,
    /// Use a binary-tree match finder.
    BinaryTree = 1,
}

/// Number of hash bytes used by the match finder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum HashBytes {
    /// Use two hash bytes.
    Two = 2,
    /// Use three hash bytes.
    Three = 3,
    /// Use four hash bytes.
    Four = 4,
}

/// Controls whether an end marker is written into the stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum EndMarkerMode {
    /// Omit the end marker from the encoded stream.
    Disabled = 0,
    /// Append an end marker to the encoded stream.
    Enabled = 1,
}

/// High-level raw `LZMA` compression options.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    level: CompressionLevel,
    dict_size: Option<u32>,
    lc: LiteralContextBits,
    lp: LiteralPosBits,
    pb: PosBits,
    algorithm: Algorithm,
    fast_bytes: FastBytes,
    match_finder: MatchFinderMode,
    hash_bytes: HashBytes,
    hash_out_bits: Option<u32>,
    match_cycles: Option<u32>,
    end_marker: EndMarkerMode,
    num_threads: u32,
    reduce_size: Option<u64>,
    affinity_group: Option<i32>,
    affinity: Option<u64>,
    affinity_in_group: Option<u64>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            level: CompressionLevel::DEFAULT,
            dict_size: None,
            lc: LiteralContextBits::DEFAULT,
            lp: LiteralPosBits::DEFAULT,
            pb: PosBits::DEFAULT,
            algorithm: Algorithm::Normal,
            fast_bytes: FastBytes::DEFAULT,
            match_finder: MatchFinderMode::BinaryTree,
            hash_bytes: HashBytes::Four,
            hash_out_bits: None,
            match_cycles: None,
            end_marker: EndMarkerMode::Disabled,
            num_threads: Self::DEFAULT_NUM_THREADS,
            reduce_size: None,
            affinity_group: None,
            affinity: None,
            affinity_in_group: None,
        }
    }
}

impl Options {
    /// Minimum number of encoder threads supported by the encoder.
    pub const MIN_NUM_THREADS: u32 = 1;
    /// Maximum number of encoder threads supported by the encoder.
    pub const MAX_NUM_THREADS: u32 = 2;
    /// Default number of encoder threads used by the safe wrapper.
    pub const DEFAULT_NUM_THREADS: u32 = 1;

    /// Creates a builder for raw `LZMA` encoder options.
    pub fn builder() -> OptionsBuilder {
        OptionsBuilder {
            options: Self::default(),
        }
    }

    /// Sets the dictionary size in bytes.
    ///
    /// Returns an error when `dict_size` is smaller than `MIN_DICT_SIZE`.
    pub fn with_dict_size(mut self, dict_size: u32) -> Result<Self, Error> {
        if dict_size < MIN_DICT_SIZE {
            return Err(invalid_option(
                "dict_size".into(),
                format!("expected at least {} bytes", MIN_DICT_SIZE),
            ));
        }
        self.dict_size = Some(dict_size);
        Ok(self)
    }

    /// Sets the number of encoder threads accepted by the safe wrapper.
    ///
    /// Returns an error when `num_threads` is outside
    /// [`Self::MIN_NUM_THREADS`]..=[`Self::MAX_NUM_THREADS`].
    pub fn with_num_threads(mut self, num_threads: u32) -> Result<Self, Error> {
        if !(Self::MIN_NUM_THREADS..=Self::MAX_NUM_THREADS).contains(&num_threads) {
            return Err(invalid_option(
                "num_threads".into(),
                format!(
                    "expected a value in {}..={}",
                    Self::MIN_NUM_THREADS,
                    Self::MAX_NUM_THREADS,
                ),
            ));
        }

        self.num_threads = num_threads;
        Ok(self)
    }

    /// Hints the source size so the SDK can tune its internal settings.
    pub fn with_reduce_size(mut self, reduce_size: u64) -> Self {
        self.reduce_size = Some(reduce_size);
        self
    }

    /// Sets whether the encoded stream should include an end marker.
    pub fn with_end_marker(mut self, end_marker: EndMarkerMode) -> Self {
        self.end_marker = end_marker;
        self
    }

    /// Sets the match-cycle limit used by the match finder.
    pub fn with_match_cycles(mut self, match_cycles: NonZeroU32) -> Self {
        self.match_cycles = Some(match_cycles.get());
        self
    }

    /// Sets the number of output hash bits used by the SDK match finder.
    pub fn with_hash_out_bits(mut self, hash_out_bits: u32) -> Self {
        self.hash_out_bits = Some(hash_out_bits);
        self
    }

    /// Sets the affinity group for SDK worker threads.
    pub fn with_affinity_group(mut self, affinity_group: i32) -> Self {
        self.affinity_group = Some(affinity_group);
        self
    }

    /// Sets the CPU affinity mask for SDK worker threads.
    pub fn with_affinity(mut self, affinity: u64) -> Self {
        self.affinity = Some(affinity);
        self
    }

    /// Sets the affinity mask within the selected SDK thread group.
    pub fn with_affinity_in_group(mut self, affinity: u64) -> Self {
        self.affinity_in_group = Some(affinity);
        self
    }

    /// Returns the compression level used by the encoder.
    pub fn level(&self) -> CompressionLevel {
        self.level
    }

    /// Returns the dictionary size in bytes used by the encoder.
    pub fn dict_size(&self) -> Option<u32> {
        self.dict_size
    }

    /// Returns the literal context bits (`lc`) used by the encoder.
    pub fn lc(&self) -> LiteralContextBits {
        self.lc
    }

    /// Returns the literal position bits (`lp`) used by the encoder.
    pub fn lp(&self) -> LiteralPosBits {
        self.lp
    }

    /// Returns the position bits (`pb`) used by the encoder.
    pub fn pb(&self) -> PosBits {
        self.pb
    }

    /// Returns the encoder algorithm used by the encoder.
    pub fn algorithm(&self) -> Algorithm {
        self.algorithm
    }

    /// Returns the number of fast bytes used by the encoder.
    pub fn fast_bytes(&self) -> FastBytes {
        self.fast_bytes
    }

    /// Returns the match finder implementation used by the encoder.
    pub fn match_finder(&self) -> MatchFinderMode {
        self.match_finder
    }

    /// Returns the number of hash bytes used by the match finder.
    pub fn hash_bytes(&self) -> HashBytes {
        self.hash_bytes
    }

    /// Returns the number of output hash bits used by the match finder.
    pub fn hash_out_bits(&self) -> Option<u32> {
        self.hash_out_bits
    }

    /// Returns the match-cycle limit used by the match finder.
    pub fn match_cycles(&self) -> Option<u32> {
        self.match_cycles
    }

    /// Returns whether the encoded stream should include an end marker.
    pub fn end_marker(&self) -> EndMarkerMode {
        self.end_marker
    }

    /// Returns the number of encoder threads used by the encoder.
    pub fn num_threads(&self) -> u32 {
        self.num_threads
    }

    /// Returns the source size hint used by the encoder.
    pub fn reduce_size(&self) -> Option<u64> {
        self.reduce_size
    }

    /// Returns the affinity group used by the encoder.
    pub fn affinity_group(&self) -> Option<i32> {
        self.affinity_group
    }

    /// Returns the CPU affinity mask used by the encoder.
    pub fn affinity(&self) -> Option<u64> {
        self.affinity
    }

    /// Returns the affinity mask within the selected SDK thread group used by the encoder.
    pub fn affinity_in_group(&self) -> Option<u64> {
        self.affinity_in_group
    }

    /// Converts these options into the raw SDK encoder properties structure.
    pub(crate) fn to_raw_encoder_props(&self) -> lzma_sdk_sys::CLzmaEncProps {
        let mut props = MaybeUninit::<lzma_sdk_sys::CLzmaEncProps>::zeroed();

        // SAFETY: `props` points to valid storage for the SDK to initialize with defaults.
        unsafe {
            lzma_sdk_sys::LzmaEncProps_Init(props.as_mut_ptr());
        }

        // SAFETY: `LzmaEncProps_Init` initialized all fields in the structure.
        let mut props = unsafe { props.assume_init() };

        props.level = i32::from(self.level);

        if let Some(dict_size) = self.dict_size {
            props.dictSize = dict_size;
        }

        props.lc = i32::from(self.lc);
        props.lp = i32::from(self.lp);
        props.pb = i32::from(self.pb);
        props.algo = self.algorithm as i32;
        props.fb = i32::from(self.fast_bytes);
        props.btMode = self.match_finder as i32;
        props.numHashBytes = self.hash_bytes as i32;

        #[cfg(any(feature = "sdk-23-01", feature = "sdk-26-00"))]
        if let Some(hash_out_bits) = self.hash_out_bits {
            props.numHashOutBits = hash_out_bits;
        }

        if let Some(match_cycles) = self.match_cycles {
            props.mc = match_cycles;
        }

        props.writeEndMark = self.end_marker as u32;
        props.numThreads = self.num_threads as i32;

        #[cfg(feature = "sdk-26-00")]
        if let Some(affinity_group) = self.affinity_group {
            props.affinityGroup = affinity_group;
        }

        #[cfg(any(feature = "sdk-23-01", feature = "sdk-26-00"))]
        if let Some(reduce_size) = self.reduce_size {
            props.reduceSize = reduce_size;
        }

        #[cfg(any(feature = "sdk-23-01", feature = "sdk-26-00"))]
        if let Some(affinity) = self.affinity {
            props.affinity = affinity;
        }

        #[cfg(feature = "sdk-26-00")]
        if let Some(affinity) = self.affinity_in_group {
            props.affinityInGroup = affinity;
        }

        props
    }
}

/// Builder for [`Options`].
pub struct OptionsBuilder {
    options: Options,
}

impl OptionsBuilder {
    /// Sets the compression level.
    pub fn level(mut self, level: CompressionLevel) -> Self {
        self.options.level = level;
        self
    }

    /// Sets the dictionary size in bytes.
    ///
    /// Returns an error when `dict_size` is smaller than `MIN_DICT_SIZE`.
    pub fn dict_size(mut self, dict_size: u32) -> Result<Self, Error> {
        self.options = self.options.with_dict_size(dict_size)?;
        Ok(self)
    }

    /// Sets the literal context bits (`lc`).
    pub fn literal_context_bits(mut self, lc: LiteralContextBits) -> Self {
        self.options.lc = lc;
        self
    }

    /// Sets the literal position bits (`lp`).
    pub fn literal_pos_bits(mut self, lp: LiteralPosBits) -> Self {
        self.options.lp = lp;
        self
    }

    /// Sets the position bits (`pb`).
    pub fn pos_bits(mut self, pb: PosBits) -> Self {
        self.options.pb = pb;
        self
    }

    /// Sets the encoder algorithm.
    pub fn algorithm(mut self, algorithm: Algorithm) -> Self {
        self.options.algorithm = algorithm;
        self
    }

    /// Sets the number of fast bytes.
    pub fn fast_bytes(mut self, fast_bytes: FastBytes) -> Self {
        self.options.fast_bytes = fast_bytes;
        self
    }

    /// Sets the match finder implementation.
    pub fn match_finder(mut self, match_finder: MatchFinderMode) -> Self {
        self.options.match_finder = match_finder;
        self
    }

    /// Sets the number of hash bytes used by the match finder.
    pub fn hash_bytes(mut self, hash_bytes: HashBytes) -> Self {
        self.options.hash_bytes = hash_bytes;
        self
    }

    /// Sets the number of output hash bits used by the SDK.
    pub fn hash_out_bits(mut self, hash_out_bits: u32) -> Self {
        self.options.hash_out_bits = Some(hash_out_bits);
        self
    }

    /// Sets the match-cycle limit used by the match finder.
    pub fn match_cycles(mut self, match_cycles: NonZeroU32) -> Self {
        self.options.match_cycles = Some(match_cycles.get());
        self
    }

    /// Sets whether the encoder should write an end marker.
    pub fn end_marker(mut self, end_marker: EndMarkerMode) -> Self {
        self.options.end_marker = end_marker;
        self
    }

    /// Sets the number of encoder threads accepted by the safe wrapper.
    ///
    /// Returns an error when `num_threads` is outside
    /// [`Options::MIN_NUM_THREADS`]..=[`Options::MAX_NUM_THREADS`].
    pub fn num_threads(mut self, num_threads: u32) -> Result<Self, Error> {
        self.options = self.options.with_num_threads(num_threads)?;
        Ok(self)
    }

    /// Hints the source size so the SDK can tune its internal settings.
    pub fn reduce_size(mut self, reduce_size: u64) -> Self {
        self.options.reduce_size = Some(reduce_size);
        self
    }

    /// Sets the affinity group for SDK worker threads.
    pub fn affinity_group(mut self, affinity_group: i32) -> Self {
        self.options.affinity_group = Some(affinity_group);
        self
    }

    /// Sets the CPU affinity mask for SDK worker threads.
    pub fn affinity(mut self, affinity: u64) -> Self {
        self.options.affinity = Some(affinity);
        self
    }

    /// Sets the affinity mask within the selected SDK thread group.
    pub fn affinity_in_group(mut self, affinity: u64) -> Self {
        self.options.affinity_in_group = Some(affinity);
        self
    }

    /// Builds the configured raw `LZMA` options value.
    pub fn build(self) -> Options {
        self.options
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CompressionLevel, EndMarkerMode, FastBytes, LiteralContextBits, LiteralPosBits,
        MatchFinderMode, Options, PosBits,
    };

    #[test]
    fn default_options_match_documented_defaults() {
        let options = Options::default();

        assert_eq!(options.level(), CompressionLevel::DEFAULT);
        assert_eq!(options.level().get(), 5);
        assert_eq!(options.lc(), LiteralContextBits::DEFAULT);
        assert_eq!(options.lc().get(), 3);
        assert_eq!(options.lp(), LiteralPosBits::DEFAULT);
        assert_eq!(options.lp().get(), 0);
        assert_eq!(options.pb(), PosBits::DEFAULT);
        assert_eq!(options.pb().get(), 2);
        assert_eq!(options.fast_bytes(), FastBytes::DEFAULT);
        assert_eq!(options.fast_bytes().get(), 32);
        assert_eq!(options.match_finder(), MatchFinderMode::BinaryTree);
        assert_eq!(options.end_marker(), EndMarkerMode::Disabled);
        assert_eq!(options.num_threads(), Options::DEFAULT_NUM_THREADS);
    }
}
