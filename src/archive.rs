//! Read-only `.7z` archive support built on the portable C decoder from the upstream SDK.
//!
//! The current implementation supports listing entries and extracting files into memory.
//! Encrypted archives are not supported by this safe wrapper.

use std::io::{Cursor, Read, Seek};
use std::mem::MaybeUninit;
use std::ptr;
use std::slice;

use crate::error::{map_status, Error, Result};
use crate::stream::{LookStream, ReadSeek};
use crate::{alloc, initialize_xz_crc_tables};

/// Sentinel value indicating an empty solid block cache.
const SOLID_CACHE_EMPTY: u32 = u32::MAX;

/// Metadata for a file or directory stored in a `.7z` archive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchiveEntry {
    /// Zero-based index used by the SDK extraction routines.
    pub index: u32,
    /// UTF-8 file name decoded from the archive header.
    pub name: String,
    /// Whether the entry is a directory.
    pub is_dir: bool,
    /// Uncompressed size reported by the archive metadata.
    pub unpacked_size: u64,
    /// Optional platform attributes if present in the archive.
    pub attributes: Option<u32>,
    /// Optional NTFS creation timestamp encoded as a raw 64-bit value.
    pub created_time: Option<u64>,
    /// Optional NTFS modification timestamp encoded as a raw 64-bit value.
    pub modified_time: Option<u64>,
}

/// Owned archive handle to a read-only `.7z` archive in memory or from an input reader.
pub struct Archive {
    /// Lower-level SDK archive descriptor structure.
    db: lzma_sdk_sys::CSzArEx,
    /// Wrapped lookahead stream, used by the SDK for seek/read operations.
    stream: Box<LookStream>,
    /// Cached enumeration of all `ArchiveEntry` items found in the archive header.
    entries: Vec<ArchiveEntry>,
    /// SDK-managed cache: current solid block index. Used to avoid redundant decompression.
    cached_block_index: u32,
    /// SDK-managed pointer to currently cached block buffer (if any).
    cached_block_buffer: *mut u8,
    /// SDK-managed byte count for the current cached block.
    cached_block_buffer_size: usize,
}

impl Archive {
    /// Opens an archive from a contiguous in-memory byte slice.
    ///
    /// The slice is copied into owned storage, so the resulting archive can outlive the slice.
    ///
    /// # Arguments
    ///
    /// * `data` - The input byte slice to open as an archive.
    ///
    /// # Returns
    ///
    /// A new `Archive` instance if the input is a valid `.7z` archive.
    ///
    /// # Errors
    ///
    /// Returns an error if the input is not a valid `.7z` archive.
    pub fn open(data: &[u8]) -> Result<Self> {
        Self::open_reader(Cursor::new(data.to_vec()))
    }

    /// Opens an archive from any seekable, readable source.
    ///
    /// # Arguments
    ///
    /// * `reader` - The seekable, readable source to open as an archive.
    ///
    /// # Returns
    ///
    /// A new `Archive` instance if the input is a valid `.7z` archive.
    ///
    /// # Errors
    ///
    /// Returns an error if the input is not a valid `.7z` archive.
    pub fn open_reader<R>(reader: R) -> Result<Self>
    where
        R: Read + Seek + Send + 'static,
    {
        Self::open_boxed(Box::new(reader))
    }

    /// Helper that wraps an existing boxed `ReadSeek` object and builds the archive.
    ///
    /// # Arguments
    ///
    /// * `reader` - The boxed trait object for user data source.
    ///
    /// # Returns
    ///
    /// A new `Archive` instance if the input is a valid `.7z` archive.
    ///
    /// # Errors
    ///
    /// Returns an error if the input is not a valid `.7z` archive.
    fn open_boxed(reader: Box<dyn ReadSeek>) -> Result<Self> {
        // The upstream SDK decoder must have its CRC tables initialized before any archive op.
        initialize_xz_crc_tables();

        // Wrap the reader in a `LookStream`, which provides the adapter struct the SDK expects.
        let stream = LookStream::new(reader);

        // SAFETY: `MaybeUninit::zeroed()` gives us fully zeroed storage — required for SDK init.
        let mut db = unsafe { MaybeUninit::<lzma_sdk_sys::CSzArEx>::zeroed().assume_init() };

        // SAFETY: `SzArEx_Init` must be called before using this archive struct.
        unsafe {
            lzma_sdk_sys::SzArEx_Init(&mut db);
        }

        // SAFETY: Both `db` and `stream` pointers must be valid and initialized here.
        let code = unsafe { lzma_sdk_sys::SzArEx_Open(&mut db, stream.as_ptr(), alloc(), alloc()) };
        if code != lzma_sdk_sys::SZ_OK as i32 {
            // If open fails, `SzArEx_Free` ensures all partially allocated structures are released.
            unsafe {
                lzma_sdk_sys::SzArEx_Free(&mut db, alloc());
            }
            // If the error was due to a lower-level I/O condition, report it specifically.
            return Err(stream
                .last_error_kind()
                .map(|kind| Error::Io {
                    kind,
                    context: "open archive".to_string(),
                })
                .unwrap_or_else(|| map_status(code)));
        }

        // Scan archive header and build a vector of entry metadata.
        let entries = collect_entries(&db)?;

        Ok(Self {
            db,
            stream,
            entries,
            cached_block_index: SOLID_CACHE_EMPTY,
            cached_block_buffer: ptr::null_mut(),
            cached_block_buffer_size: 0,
        })
    }

    /// Returns an immutable slice of all archive entries, in the order they appear in the header.
    pub fn entries(&self) -> &[ArchiveEntry] {
        &self.entries
    }

    /// Returns a reference to a specific archive entry by index, or `None` if out-of-bounds.
    pub fn entry(&self, index: usize) -> Option<&ArchiveEntry> {
        self.entries.get(index)
    }

    /// Extracts a file entry by index and returns its uncompressed contents as a new Vec<u8>.
    ///
    /// # Arguments
    ///
    /// * `index` - The index of the entry to extract.
    ///
    /// # Returns
    ///
    /// A new `Vec<u8>` containing the uncompressed contents of the entry.
    ///
    /// # Errors
    ///
    /// Returns `Err(Error::Param)` if the entry is out of bounds or refers to a directory.
    pub fn read_file(&mut self, index: usize) -> Result<Vec<u8>> {
        let entry = self.entries.get(index).ok_or(Error::Param)?;
        if entry.is_dir {
            // Directories cannot be extracted.
            return Err(Error::Param);
        }

        // Parameters for SDK extraction routine.
        let mut offset = 0usize;
        let mut processed = 0usize;
        // SAFETY:
        //   - `self.db` and `self.stream` must remain valid during the call.
        //   - SDK contract is followed for extracting a single file by index.
        //   - Solid block cache management must follow SDK's allocation expectations.
        let code = unsafe {
            lzma_sdk_sys::SzArEx_Extract(
                &self.db,
                self.stream.as_ptr(),
                entry.index,
                &mut self.cached_block_index,
                &mut self.cached_block_buffer,
                &mut self.cached_block_buffer_size,
                &mut offset,
                &mut processed,
                alloc(),
                alloc(),
            )
        };
        if code != lzma_sdk_sys::SZ_OK as i32 {
            // Return any lower-level I/O error, otherwise convert SDK status to error enum.
            return Err(self
                .stream
                .last_error_kind()
                .map(|kind| Error::Io {
                    kind,
                    context: "extract archive entry".to_string(),
                })
                .unwrap_or_else(|| map_status(code)));
        }

        if processed == 0 {
            // A valid (but empty) file: return an empty Vec.
            return Ok(Vec::new());
        }

        // SAFETY:
        //   - The SDK guarantees that `cached_block_buffer` + offset is valid for `processed` bytes.
        //   - This references the correct portion of the decompressed (cache) buffer.
        let data =
            unsafe { slice::from_raw_parts(self.cached_block_buffer.add(offset), processed) };
        Ok(data.to_vec())
    }

    /// Releases any current cached decompression buffer for solid blocks.
    pub fn clear_cache(&mut self) {
        if !self.cached_block_buffer.is_null() {
            // SAFETY:
            //   - The solid cache buffer (if present) is owned by this archive and allocated by the SDK.
            //   - Only free via the matching SDK allocator.
            unsafe {
                lzma_sdk_sys::free_with_default_alloc(self.cached_block_buffer.cast());
            }
        }

        self.cached_block_index = SOLID_CACHE_EMPTY;
        self.cached_block_buffer = ptr::null_mut();
        self.cached_block_buffer_size = 0;
    }
}

impl Drop for Archive {
    fn drop(&mut self) {
        // Ensure the solid block cache is deallocated before we free the archive descriptor.
        self.clear_cache();

        // SAFETY:
        //   - The archive descriptor must have been initialized by `SzArEx_Init`.
        //   - This is the final step in the drop chain; after this point the archive must not be touched.
        unsafe {
            lzma_sdk_sys::SzArEx_Free(&mut self.db, alloc());
        }
    }
}

/// Enumerates all entries in the archive header and constructs a Vec of ArchiveEntry.
///
/// # Arguments
///
/// * `db` - The archive descriptor to collect entries from.
///
/// # Returns
///
/// A `Vec<ArchiveEntry>` containing all entries in the archive header.
///
/// # Errors
///
/// Returns an error if the number of files in the archive header cannot be converted to a `usize`.
fn collect_entries(db: &lzma_sdk_sys::CSzArEx) -> Result<Vec<ArchiveEntry>> {
    let num_files = archive_num_files(db)?;
    let mut entries = Vec::with_capacity(num_files);
    for index in 0..num_files {
        // Query the length (in u16s) needed to hold the file name, including null terminator.
        let name_len = unsafe { lzma_sdk_sys::SzArEx_GetFileNameUtf16(db, index, ptr::null_mut()) };
        // Allocate space for the UTF-16 name plus potential null terminator.
        let mut name_utf16 = vec![0_u16; name_len];
        // Actually read the file name UTF-16 data from the archive.
        let written =
            unsafe { lzma_sdk_sys::SzArEx_GetFileNameUtf16(db, index, name_utf16.as_mut_ptr()) };
        // Remove the trailing null (if present).
        name_utf16.truncate(written.saturating_sub(1));

        entries.push(ArchiveEntry {
            index: index as u32,
            name: String::from_utf16_lossy(&name_utf16),
            is_dir: archive_is_dir(db, index),
            unpacked_size: unpacked_size(db, index)?,
            attributes: archive_attributes(db, index),
            created_time: archive_created_time(db, index),
            modified_time: archive_modified_time(db, index),
        });
    }
    Ok(entries)
}

/// Fetches the number of files in the archive, as reported by the SDK.
#[cfg(feature = "sdk-9-20")]
fn archive_num_files(db: &lzma_sdk_sys::CSzArEx) -> Result<usize> {
    usize::try_from(db.db.NumFiles).map_err(|_| Error::SizeOverflow)
}

/// Fetches the number of files in the archive for older SDK layouts.
#[cfg(not(feature = "sdk-9-20"))]
fn archive_num_files(db: &lzma_sdk_sys::CSzArEx) -> Result<usize> {
    usize::try_from(db.NumFiles).map_err(|_| Error::SizeOverflow)
}

/// Determines if the given entry index is a directory (for SDK 9.20+ field layout).
#[cfg(feature = "sdk-9-20")]
fn archive_is_dir(db: &lzma_sdk_sys::CSzArEx, index: usize) -> bool {
    // SAFETY: `Files` array is allocated to hold at least NumFiles entries after `SzArEx_Open`.
    unsafe { (*db.db.Files.add(index)).IsDir != 0 }
}

/// Determines if the entry is a directory using bit flag arrays from the legacy SDK.
#[cfg(not(feature = "sdk-9-20"))]
fn archive_is_dir(db: &lzma_sdk_sys::CSzArEx, index: usize) -> bool {
    bit_is_set(db.IsDirs, index)
}

/// Retrieves the unpacked (decompressed) size for a given entry, modern SDK style.
#[cfg(feature = "sdk-9-20")]
fn unpacked_size(db: &lzma_sdk_sys::CSzArEx, index: usize) -> Result<u64> {
    // SAFETY: `Files` is allocated for all entries.
    Ok(unsafe { (*db.db.Files.add(index)).Size })
}

/// Retrieves the unpacked size for a given entry from legacy archives using file position arrays.
#[cfg(not(feature = "sdk-9-20"))]
fn unpacked_size(db: &lzma_sdk_sys::CSzArEx, index: usize) -> Result<u64> {
    // SAFETY: `UnpackPositions` holds NumFiles+1 items, giving start+end for each file.
    let start = unsafe { *db.UnpackPositions.add(index) };
    let end = unsafe { *db.UnpackPositions.add(index + 1) };
    end.checked_sub(start).ok_or(Error::SizeOverflow)
}

/// Retrieves platform-specific (e.g. Windows) attributes for a file, if defined (SDK 9.20+).
#[cfg(feature = "sdk-9-20")]
fn archive_attributes(db: &lzma_sdk_sys::CSzArEx, index: usize) -> Option<u32> {
    // SAFETY: Access Files for the given index.
    let file = unsafe { *db.db.Files.add(index) };
    (file.AttribDefined != 0).then_some(file.Attrib)
}

/// Retrieves optional attribute value, if it is defined, from bitfields in legacy archives.
#[cfg(not(feature = "sdk-9-20"))]
fn archive_attributes(db: &lzma_sdk_sys::CSzArEx, index: usize) -> Option<u32> {
    bit_u32(&db.Attribs, index)
}

/// The legacy SDK does not store NTFS creation times, so always returns None.
#[cfg(feature = "sdk-9-20")]
fn archive_created_time(_db: &lzma_sdk_sys::CSzArEx, _index: usize) -> Option<u64> {
    None
}

/// Extracts the NTFS creation time if present in the legacy SDK layout.
#[cfg(not(feature = "sdk-9-20"))]
fn archive_created_time(db: &lzma_sdk_sys::CSzArEx, index: usize) -> Option<u64> {
    bit_ntfs_time(&db.CTime, index)
}

/// Fetches NTFS modification timestamp, if defined, for newer SDK layout.
#[cfg(feature = "sdk-9-20")]
fn archive_modified_time(db: &lzma_sdk_sys::CSzArEx, index: usize) -> Option<u64> {
    // SAFETY: File time members must be read from the Files array for the entry.
    let file = unsafe { *db.db.Files.add(index) };
    if file.MTimeDefined == 0 {
        return None;
    }

    Some((u64::from(file.MTime.High) << 32) | u64::from(file.MTime.Low))
}

/// Retrieves the NTFS modification time for old SDK archive layouts.
#[cfg(not(feature = "sdk-9-20"))]
fn archive_modified_time(db: &lzma_sdk_sys::CSzArEx, index: usize) -> Option<u64> {
    bit_ntfs_time(&db.MTime, index)
}

/// Checks if the given bit flag is set in a bitfield (used for boolean per-entry properties).
///
/// Returns false if the flag array is null.
#[cfg(not(feature = "sdk-9-20"))]
fn bit_is_set(bits: *mut u8, index: usize) -> bool {
    if bits.is_null() {
        return false;
    }

    // SAFETY: The SDK ensures these arrays have enough elements for all entries in the archive.
    let value = unsafe { *bits.add(index >> 3) };
    (value & (0x80 >> (index & 7))) != 0
}

/// Reads a `u32` value from an array if it is defined (i.e. the associated bit is set).
#[cfg(not(feature = "sdk-9-20"))]
fn bit_u32(values: &lzma_sdk_sys::CSzBitUi32s, index: usize) -> Option<u32> {
    if !bit_is_set(values.Defs, index) || values.Vals.is_null() {
        return None;
    }

    // SAFETY: The values pointer is valid for the number of defined entries.
    Some(unsafe { *values.Vals.add(index) })
}

/// Reads a 64-bit time value from a bit array if present for the given entry.
#[cfg(not(feature = "sdk-9-20"))]
fn bit_ntfs_time(values: &lzma_sdk_sys::CSzBitUi64s, index: usize) -> Option<u64> {
    if !bit_is_set(values.Defs, index) || values.Vals.is_null() {
        return None;
    }

    // SAFETY: Value must be valid for all bits that are set.
    let value = unsafe { *values.Vals.add(index) };
    Some((u64::from(value.High) << 32) | u64::from(value.Low))
}

#[cfg(test)]
mod tests {
    use std::io::{self, Read, Seek, SeekFrom};

    use crate::Error;

    use super::Archive;
    #[cfg(not(feature = "sdk-9-20"))]
    use super::{bit_ntfs_time, bit_u32};

    /// Dummy reader that always fails for testing error handling and propagation.
    struct FailingReader {
        kind: io::ErrorKind,
    }

    impl Read for FailingReader {
        fn read(&mut self, _buf: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::from(self.kind))
        }
    }

    impl Seek for FailingReader {
        fn seek(&mut self, _pos: SeekFrom) -> io::Result<u64> {
            Err(io::Error::from(self.kind))
        }
    }

    /// Test that open returns an archive-format error for non-7z input.
    #[test]
    fn rejects_non_archive_input() {
        let error = Archive::open(b"not a 7z archive").err().unwrap();
        assert!(matches!(
            error,
            Error::NoArchive | Error::Archive | Error::Data
        ));
    }

    /// Test that I/O errors from the upstream reader are preserved during open.
    #[test]
    fn open_reader_preserves_io_error_context() {
        let error = Archive::open_reader(FailingReader {
            kind: io::ErrorKind::PermissionDenied,
        })
        .err()
        .unwrap();

        assert_eq!(
            error,
            Error::Io {
                kind: io::ErrorKind::PermissionDenied,
                context: "open archive".to_string(),
            }
        );
    }

    /// Confirm that `bit_u32` only reads entries if the defining bit is set.
    #[cfg(not(feature = "sdk-9-20"))]
    #[test]
    fn bit_u32_reads_only_defined_entries() {
        let mut defs = [0b1000_0000_u8];
        let mut vals = [0xDEAD_BEEF_u32, 0xCAFE_BABE_u32];
        let values = lzma_sdk_sys::CSzBitUi32s {
            Defs: defs.as_mut_ptr(),
            Vals: vals.as_mut_ptr(),
        };

        assert_eq!(bit_u32(&values, 0), Some(0xDEAD_BEEF));
        assert_eq!(bit_u32(&values, 1), None);
    }

    /// Confirm that `bit_ntfs_time` correctly combines high and low bits for NTFS times.
    #[cfg(not(feature = "sdk-9-20"))]
    #[test]
    fn bit_ntfs_time_combines_high_and_low_parts() {
        let mut defs = [0b1000_0000_u8];
        let mut vals = [
            lzma_sdk_sys::CNtfsFileTime {
                Low: 0x5566_7788,
                High: 0x1122_3344,
            },
            lzma_sdk_sys::CNtfsFileTime { Low: 0, High: 0 },
        ];
        let values = lzma_sdk_sys::CSzBitUi64s {
            Defs: defs.as_mut_ptr(),
            Vals: vals.as_mut_ptr(),
        };

        assert_eq!(bit_ntfs_time(&values, 0), Some(0x1122_3344_5566_7788));
        assert_eq!(bit_ntfs_time(&values, 1), None);
    }
}
