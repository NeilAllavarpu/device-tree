//! Parsing of raw bytes into more Rust-friendly formats
//!
//! The core struct encapsulates the raw, `u32`-aligned, big-endian device tree blob and provides utility functions for extracting meaningful, endianness-independent data.

use core::{
    ffi::{CStr, FromBytesUntilNulError},
    mem,
    num::NonZeroUsize,
    ptr::NonNull,
};

/// A `U32ByteSlice` encapsulates a slice of `u32`s in big-endian format.
/// This is the format used by the device tree header and struct portion of the device tree blob.
#[derive(Debug, Clone, Copy)]
pub struct U32ByteSlice<'bytes> {
    /// The actual bytes themselves
    bytes: &'bytes [u32],
    /// The number of padding bytes inserted into this byte slice
    padding: u8,
}

/// Width of a single element in the slice
const ELEMENT_WIDTH: usize = mem::size_of::<u32>();

impl<'bytes> U32ByteSlice<'bytes> {
    /// Wraps a big-endian slice of `u32`s into a parser
    #[expect(clippy::unwrap_in_result, reason = "Checks should never fail")]
    pub fn new(bytes: &'bytes [u32], length: usize) -> Option<Self> {
        if length.div_ceil(ELEMENT_WIDTH) == bytes.len() {
            let padding = bytes
                .len()
                .checked_mul(ELEMENT_WIDTH)
                .and_then(|byte_count| byte_count.checked_sub(length))
                .and_then(|padding| u8::try_from(padding).ok())
                .expect("Length should be 0..4 less than number of provided bytes");
            (!bytes.last().is_some_and(|&chunk| {
                u32::from_be(chunk)
                    .to_be_bytes()
                    .iter()
                    .rev()
                    .take(padding.into())
                    .any(|&byte| byte != 0)
            }))
            .then_some(Self { bytes, padding })
        } else {
            None
        }
    }

    /// Returns the number of *whole* `u32`s available, i.e. excluding any partial `u32` with padding
    fn remaining_u32s(&self) -> usize {
        self.len_u32s()
            .saturating_sub(usize::from(self.padding > 0))
    }

    /// Removes the first `u32` from this slice, if any are left
    pub fn consume_u32(&mut self) -> Option<u32> {
        if self.remaining_u32s() >= 1 {
            self.bytes.take_first().copied().map(u32::from_be)
        } else {
            None
        }
    }

    /// Removes the first two `u32`s from this slice and converts it to a `u64`, if there are enough `u32`s present
    pub fn consume_u64(&mut self) -> Option<u64> {
        if self.remaining_u32s() >= 2 {
            self.bytes
                .take(..2)
                .map(|pair| {
                    <&[u32; 2]>::try_from(pair).expect("`Take` should return a two-element array")
                })
                .map(|&[upper, lower]| {
                    (u64::from(u32::from_be(upper)) << u32::BITS) | u64::from(u32::from_be(lower))
                })
        } else {
            None
        }
    }

    /// Removes the first `cell_count` `u32`s and returns them as an integer
    ///
    /// Currently the implementation only handles returning up to `u64`s, and will "silently" fail for larger cell counts (but will print out a message)
    pub fn consume_cells(&mut self, cell_count: u8) -> Option<u64> {
        match cell_count {
            0 => Some(0),
            1 => self.consume_u32().map(u64::from),
            2 => self.consume_u64(),
            count => {
                let value = self.consume_u64();
                for _ in 2..count {
                    if self.consume_u32() != Some(0) {
                        eprintln!("WARNING: Cannot handle cell count {cell_count}");
                    }
                }
                value
            }
        }
    }

    /// Converts this byte slice into a single cell integer, if exactly `cell_count` integers are in the slice
    ///
    /// This has the same limitations as `consume_cells` with respect to cell counts
    pub fn into_cells(mut self, cell_count: u8) -> Option<u64> {
        self.consume_cells(cell_count).filter(|_| self.is_empty())
    }

    /// Converts this slice into a list of appropriate cell arrays, where the width of each element is determined by the corresponding size specified in `cell_counts`
    ///
    /// This has the same limitations as `consume_cells` with respect to cell counts
    #[expect(clippy::unwrap_in_result, reason = "Checks should never fail")]
    pub fn into_cells_slice<const N: usize>(
        mut self,
        cell_counts: &[u8; N],
    ) -> Option<Box<[[u64; N]]>> {
        if self.padding != 0 {
            return None;
        }
        let total_length = cell_counts
            .iter()
            .copied()
            .map(usize::from)
            .try_reduce(usize::checked_add)
            .expect("The total size of cells should not overflow a `usize`")
            .expect("There should be a nonzero number of cells");
        if let Some(length) = NonZeroUsize::new(total_length) {
            if self.len_u32s() % length != 0 {
                return None;
            }
            let num_groups = self.len_u32s() / length;
            let mut cell_list = Vec::with_capacity(num_groups);
            while !self.is_empty() {
                let mut cell_group = [0; N];
                for (&mut ref mut value, &size) in cell_group.iter_mut().zip(cell_counts.iter()) {
                    *value = self
                        .consume_cells(size)
                        .expect("Length should have been properly checked already");
                }
                cell_list.push(cell_group);
            }
            Some(cell_list.into_boxed_slice())
        } else {
            self.is_empty().then(|| Vec::new().into_boxed_slice())
        }
    }

    /// Takes the first `count` *bytes* from the slice, if there are enough.
    /// After the removal, this slice is still aligned to `u32`s, i.e. padding may be discarded
    pub fn take(&mut self, bytes: usize) -> Option<Self> {
        if bytes <= self.len_bytes() {
            self.bytes
                .take(..bytes.div_ceil(ELEMENT_WIDTH))
                .map(|slice| Self::new(slice, bytes).expect("Enough bytes should have been taken"))
        } else {
            None
        }
    }

    /// Extracts the first C string (i.e. up to the first null byte) from this slice, or fails if there is no null byte.
    ///
    /// Rounds up to the nearest `u32` boundary after removing the bytes corresponding to the C string
    #[expect(clippy::unwrap_in_result, reason = "Checks should never fail")]
    pub fn consume_c_str(&mut self) -> Option<&'bytes CStr> {
        let c_str = CStr::from_bytes_until_nul((*self).into()).ok()?;

        let bytes_consumed = c_str
            .count_bytes()
            .checked_add(1) // NUL byte
            .expect("Number of total bytes should fit into a `usize`");

        let taken = self
            .bytes
            .take(..bytes_consumed.div_ceil(ELEMENT_WIDTH))
            .expect("The CStr should remain within the bounds of the slice");

        let padding = bytes_consumed
            .next_multiple_of(ELEMENT_WIDTH)
            .checked_sub(bytes_consumed)
            .expect("Padding should never be negative");

        // Extra padding should always be zeroes
        if !u32::from_be(
            *taken
                .last()
                .expect("At least one NUL byte should be consumed for each CStr"),
        )
        .to_be_bytes()
        .iter()
        .rev()
        .take(padding)
        .all(|&byte| byte == 0)
        {
            return None;
        }

        Some(c_str)
    }

    /// Returns whether or not there are any bytes left in this slice
    pub const fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    /// Returns the number of `u32`s in this slice, NOT the number of bytes
    const fn len_u32s(&self) -> usize {
        self.bytes.len()
    }

    /// Returns the number of bytes in this slice, NOT the number of `u32`s
    fn len_bytes(&self) -> usize {
        self.len_u32s()
            .checked_mul(ELEMENT_WIDTH)
            .and_then(|bytes| bytes.checked_sub(self.padding.into()))
            .expect("Number of bytes should not overflow a `usize`")
    }
}

/// Error from converting a byte slice to an integer
#[derive(Debug)]
pub enum TryFromError {
    /// Insufficient contents in the byte slice
    Empty,
    /// Too many contents in the byte slice
    Excess,
}

impl TryFrom<U32ByteSlice<'_>> for u32 {
    type Error = TryFromError;

    #[inline]
    fn try_from(mut value: U32ByteSlice<'_>) -> Result<Self, Self::Error> {
        let result = value.consume_u32().ok_or(TryFromError::Empty);
        if value.is_empty() {
            result
        } else {
            Err(TryFromError::Excess)
        }
    }
}

impl TryFrom<U32ByteSlice<'_>> for u64 {
    type Error = TryFromError;

    #[inline]
    fn try_from(mut value: U32ByteSlice<'_>) -> Result<Self, Self::Error> {
        let result = value.consume_u64().ok_or(TryFromError::Empty);
        if value.is_empty() {
            result
        } else {
            Err(TryFromError::Excess)
        }
    }
}

impl<'bytes> From<U32ByteSlice<'bytes>> for &'bytes [u8] {
    #[inline]
    fn from(value: U32ByteSlice<'bytes>) -> Self {
        // SAFETY: The pointer is valid, accessible, and initalized because it comes from a valid, initialized region of memory and `u32`s are safe to transmute to `u8`s
        // The lifetime of the underlying data is guaranteed to be immutable because the original shared slice guarantees the underlying bytes are not mutated
        unsafe {
            NonNull::slice_from_raw_parts(
                NonNull::from(value.bytes).as_non_null_ptr().cast(),
                value.len_bytes(),
            )
            .as_ref()
        }
    }
}

impl<'bytes> TryFrom<U32ByteSlice<'bytes>> for &'bytes [u32] {
    type Error = ();
    #[inline]
    fn try_from(value: U32ByteSlice<'bytes>) -> Result<Self, Self::Error> {
        (value.padding == 0).then_some(value.bytes).ok_or(())
    }
}

impl<'bytes> TryFrom<U32ByteSlice<'bytes>> for &'bytes CStr {
    type Error = FromBytesUntilNulError;

    #[inline]
    fn try_from(value: U32ByteSlice<'bytes>) -> Result<Self, Self::Error> {
        CStr::from_bytes_until_nul(value.into())
    }
}

/// Converts the given byte slice to a C string at compile time
pub const fn to_c_str(string: &[u8]) -> &CStr {
    if let Ok(c_string) = CStr::from_bytes_with_nul(string) {
        c_string
    } else {
        unreachable!()
    }
}
