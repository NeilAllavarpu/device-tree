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
}

impl<'bytes> U32ByteSlice<'bytes> {
    /// Wraps a big-endian slice of `u32`s into a parser
    pub const fn new(bytes: &'bytes [u32]) -> Self {
        Self { bytes }
    }

    /// Removes the first `u32` from this slice, if any are left
    pub fn consume_u32(&mut self) -> Option<u32> {
        self.bytes.take_first().copied().map(u32::from_be)
    }

    /// Removes the first two `u32`s from this slice and converts it to a `u64`, if there are enough `u32`s present
    pub fn consume_u64(&mut self) -> Option<u64> {
        self.bytes
            .take(..2)
            .map(|pair| {
                <&[u32; 2]>::try_from(pair).expect("`Take` should return a two-element array")
            })
            .map(|&[upper, lower]| {
                (u64::from(u32::from_be(upper)) << u32::BITS) | u64::from(u32::from_be(lower))
            })
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

    /// Takes the first `count` `u32`s from the slice, if there are enough
    pub fn take(&mut self, count: usize) -> Option<Self> {
        self.bytes.take(..count).map(Self::new)
    }

    /// Extracts the first C string (i.e. up to the first null byte) from this slice, or fails if there is no null byte.
    ///
    /// Rounds up to the nearest `u32` boundary after removing the bytes corresponding to the C string
    #[expect(clippy::unwrap_in_result, reason = "Checks should never fail")]
    pub fn consume_c_str(&mut self) -> Option<&'bytes CStr> {
        let c_str = CStr::from_bytes_until_nul((*self).into()).ok()?;

        self.bytes
            .take(
                ..c_str
                    .to_bytes_with_nul()
                    .len()
                    .div_ceil(mem::size_of::<u32>()),
            )
            .expect("The CStr should remain within the bounds of the slice");

        Some(c_str)
    }

    /// Returns whether or not there are any `u32`s left in this slice
    pub const fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    /// Returns the number of `u32`s in this slice, NOT the number of bytes
    pub const fn len_u32s(&self) -> usize {
        self.bytes.len()
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
                mem::size_of_val(value.bytes),
            )
            .as_ref()
        }
    }
}

impl<'bytes> From<U32ByteSlice<'bytes>> for &'bytes [u32] {
    #[inline]
    fn from(value: U32ByteSlice<'bytes>) -> Self {
        value.bytes
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
