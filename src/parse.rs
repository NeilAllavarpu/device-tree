use core::{
    ffi::{CStr, FromBytesUntilNulError},
    mem,
    num::NonZeroUsize,
    ptr::NonNull,
    str,
};
#[derive(Debug, Clone, Copy)]
pub struct U32ByteSlice<'bytes> {
    bytes: &'bytes [u32],
}

impl<'bytes> U32ByteSlice<'bytes> {
    pub const fn new(bytes: &'bytes [u32]) -> Self {
        Self { bytes }
    }

    pub fn consume_u32(&mut self) -> Option<u32> {
        self.bytes.take_first().copied().map(u32::from_be)
    }

    pub fn consume_u64(&mut self) -> Option<u64> {
        self.bytes
            .take(..2)
            .map(|bytes| u64::from(bytes[0]) << 32 | u64::from(bytes[1]))
    }

    pub fn consume_cells(&mut self, cell_count: u8) -> Option<u64> {
        match cell_count {
            0 => Some(0),
            1 => self.consume_u32().map(u64::from),
            2 => self.consume_u64(),
            count => {
                let value = self.consume_u64();
                for _ in 2..count {
                    if self.consume_u32() != Some(0) {
                        println!("Cannot handle cell count {cell_count}");
                    }
                }
                value
            }
        }
    }

    /// c
    #[expect(clippy::unwrap_in_result)]
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
            if self.len() % length != 0 {
                return None;
            }
            let num_groups = self.len() / length;
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

    pub fn take(&mut self, count: usize) -> Option<Self> {
        self.bytes.take(..count).map(Self::new)
    }

    pub fn consume_c_str(&mut self) -> Option<&'bytes CStr> {
        let c_str = CStr::from_bytes_until_nul((*self).into())
            .map_err(|_err| ParseStrError::NotNullTerminated)
            .unwrap();

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

    pub const fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    pub const fn len(&self) -> usize {
        self.bytes.len()
    }
}

#[derive(Debug)]
pub enum TryFromError {
    Empty,
    Excess,
}

impl TryFrom<U32ByteSlice<'_>> for u32 {
    type Error = TryFromError;

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
    fn from(value: U32ByteSlice<'bytes>) -> Self {
        value.bytes
    }
}

impl<'bytes> TryFrom<U32ByteSlice<'bytes>> for &'bytes CStr {
    type Error = FromBytesUntilNulError;

    fn try_from(value: U32ByteSlice<'bytes>) -> Result<Self, Self::Error> {
        CStr::from_bytes_until_nul(value.into())
    }
}

#[derive(Debug)]
pub(crate) enum ParseStrError {
    NotNullTerminated,
    Utf8Error(str::Utf8Error),
}

/// Attempts to convert a byte slice representing a `CStr` into a proper `str`
pub(crate) fn parse_str(bytes: &[u8]) -> Result<&str, ParseStrError> {
    let c_str =
        CStr::from_bytes_until_nul(bytes).map_err(|_err| ParseStrError::NotNullTerminated)?;
    c_str.to_str().map_err(ParseStrError::Utf8Error)
}
